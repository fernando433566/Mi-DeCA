// Editor de preferencias ("fijar" campos que se rellenan solos en cada DeCA).
// Se usa en: Ajustes -> Preferencias (propias), Preferencias de la empresa y
// Preferencias de un trabajador (administrador).

import { h } from '../core/dom.js';
import { api } from '../core/api.js';
import { page, sign, loading, errorView, input, busy, toast, notice } from '../core/ui.js';
import { combo, matches } from '../components/combo.js';
import { addressInput } from '../components/address.js';
import * as catalog from '../components/catalog.js';
import { parseNum } from '../core/format.js';

const NUMERIC = ['peso_kg', 'volumen_m3', 'numero_viajes'];
const PLATES = ['matricula_camion', 'matricula_remolque', 'cargador_nif', 'transportista_nif'];
const ADDRESS = ['origen', 'destino'];

/**
 * @param {Array<{campo, nombre}>} fields
 * @param {object} values   valores fijados en este nivel
 * @param {object} locked   { campo: valor } fijado por un nivel superior (solo lectura)
 */
function editor(fields, values, locked = {}) {
  const rows = {};
  const container = h('div');

  const setRow = (campo, v) => {
    const r = rows[campo];
    if (!r || r.locked) return;
    r.cb.checked = true;
    r.box.hidden = false;
    r.set(v);
  };

  for (const { campo, nombre } of fields) {
    if (campo in locked) {
      container.appendChild(h('div.panel', { style: { opacity: '.85' } },
        h('div.switch', h('span', h('strong', nombre), h('div.muted', `Fijado por la empresa: ${locked[campo]}`)), h('span.pin.lock', 'Bloqueado')),
      ));
      rows[campo] = { locked: true };
      continue;
    }
    let control;
    let get;
    let set;
    if (ADDRESS.includes(campo)) {
      control = addressInput({ value: values[campo] ?? '' });
      get = () => control.getValue();
      set = (v) => control.setValue(v);
    } else {
      const inp = input({
        value: values[campo] ?? '',
        plate: PLATES.includes(campo),
        inputmode: NUMERIC.includes(campo) ? 'decimal' : undefined,
      });
      get = () => inp.value.trim();
      set = (v) => { inp.value = v ?? ''; };
      control = inp;
      // Sugerencias de la base de datos compartida.
      if (campo === 'cargador_empresa' || campo === 'transportista_empresa') {
        const tipo = campo.split('_')[0];
        control = combo({
          input: inp,
          suggest: async (q) => (await catalog.load('parties')).filter((p) => p.tipo === tipo && (!q || matches(q, p.nombre, p.nif))).map((p) => ({ label: p.nombre, sub: p.nif, value: p })),
          onPick: ({ value: p }) => {
            set(p.nombre);
            setRow(`${tipo}_nif`, p.nif);
            setRow(`${tipo}_domicilio`, p.domicilio);
            if (tipo === 'transportista' && p.titular) setRow('transportista_titular', p.titular);
          },
        });
      } else if (campo === 'matricula_camion') {
        control = combo({
          input: inp,
          suggest: async (q) => (await catalog.load('trucks')).filter((t) => !q || matches(q, t.nombre, t.matricula)).map((t) => ({ label: t.nombre, sub: t.matricula, value: t })),
          onPick: ({ value: t }) => { set(t.matricula); if (t.remolque_matricula) setRow('matricula_remolque', t.remolque_matricula); },
        });
      } else if (campo === 'mercancia') {
        control = combo({
          input: inp,
          suggest: async (q) => (await catalog.load('goods')).filter((g) => !q || matches(q, g.descripcion)).map((g) => ({ label: g.descripcion, value: g.descripcion })),
          onPick: ({ value }) => set(value),
        });
      }
    }
    const fixed = values[campo] !== undefined && values[campo] !== null && values[campo] !== '';
    const box = h('div', { style: { marginTop: '12px' }, hidden: !fixed }, control);
    const cb = h('input', { type: 'checkbox', checked: fixed });
    cb.addEventListener('change', () => { box.hidden = !cb.checked; });
    container.appendChild(h('div.panel',
      h('label.switch', h('span', h('strong', nombre), h('div.muted', 'Fijar este dato')), cb, h('span.lever')),
      box,
    ));
    rows[campo] = { cb, box, get, set };
  }

  container.collect = () => {
    const out = {};
    for (const [campo, r] of Object.entries(rows)) {
      if (r.locked || !r.cb.checked) continue;
      let v = r.get();
      if (!v) continue;
      if (NUMERIC.includes(campo)) {
        v = parseNum(v);
        if (Number.isNaN(v)) throw new Error(`Número no válido en «${fields.find((f) => f.campo === campo).nombre}»`);
      }
      out[campo] = v;
    }
    return out;
  };
  return container;
}

async function screen({ title, back, load, save, intro }) {
  page({ title, back, body: loading() });
  let data;
  try {
    data = await load();
  } catch (e) {
    return page({ title, back, body: errorView(e, () => screen({ title, back, load, save, intro })) });
  }
  const ed = editor(data.fields, data.values, data.locked);
  const btn = sign({ text: 'Guardar preferencias', ico: 'check', color: 'green', center: true });
  btn.onclick = () => busy(btn, async () => {
    await save(ed.collect());
    toast('Preferencias guardadas', 'ok');
  });
  page({ title, back, body: [intro, ed], actions: [btn] });
}

const str = (v) => (v === null || v === undefined ? '' : String(v));

export function ownView() {
  return screen({
    title: 'Preferencias',
    back: '/ajustes',
    intro: notice('Los datos que fijes se rellenan solos al crear un DeCA. Siempre puedes cambiarlos en cada DeCA.', 'info', 'pin'),
    load: async () => {
      const r = await api('/api/me/preferences');
      const locked = {};
      for (const [k, v] of Object.entries({ ...r.fijadas_por_administrador.empresa, ...r.fijadas_por_administrador.trabajador })) locked[k] = str(v);
      return { fields: r.campos, values: r.propias, locked };
    },
    save: (v) => api('/api/me/preferences', { method: 'PUT', body: v }),
  });
}

export function companyView({ cid }) {
  return screen({
    title: 'Preferencias de la empresa',
    back: `/empresa/${cid}/preferencias`,
    intro: notice('Se aplican a todos los trabajadores de la empresa. Ellos no podrán cambiar estas preferencias.', 'info', 'building'),
    load: async () => {
      const r = await api(`/api/companies/${cid}/preferences`);
      return { fields: r.campos, values: r.fijadas, locked: {} };
    },
    save: (v) => api(`/api/companies/${cid}/preferences`, { method: 'PUT', body: v }),
  });
}

export function workerView({ wid }) {
  return screen({
    title: 'Preferencias del trabajador',
    back: `/trabajador/${wid}`,
    intro: notice('Solo para este trabajador. Tienen prioridad sobre las de la empresa.', 'info', 'user'),
    load: async () => {
      const r = await api(`/api/workers/${wid}/preferences`);
      return { fields: r.campos, values: r.fijadas, locked: {} };
    },
    save: (v) => api(`/api/workers/${wid}/preferences`, { method: 'PUT', body: v }),
  });
}
