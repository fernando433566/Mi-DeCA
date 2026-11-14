// Editor de preferencias: "fijar" campos que se rellenan solos en cada DeCA y
// las preferencias visuales (sí/no).
// Se usa en: Ajustes -> Preferencias (propias), Preferencias de la empresa y
// Preferencias de un trabajador (administrador).

import { h } from '../core/dom.js';
import { api } from '../core/api.js';
import { page, sign, loading, errorView, input, busy, toast, notice, select, toggle, panel, field } from '../core/ui.js';
import { setUser } from '../core/state.js';
import { go } from '../core/router.js';
import { combo, matches } from '../components/combo.js';
import { addressInput } from '../components/address.js';
import * as catalog from '../components/catalog.js';
import { parseNum } from '../core/format.js';

const NUMERIC = ['peso_kg', 'volumen_m3'];
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

/**
 * «Preferencias visuales» (sí/no).
 * - Del propio usuario: un interruptor; si el administrador lo fijó, solo se muestra.
 * - Del administrador (empresa o trabajador): «No fijar», «Activado por defecto» o
 *   «Desactivado por defecto». Lo que se fija aquí no lo puede cambiar el trabajador.
 * @param {Array<{campo, nombre, bloqueado?, valor?}>} fields
 * @param {object} values  valores guardados en este nivel
 * @param {{ admin: boolean }} options
 */
function visualEditor(fields, values, { admin }) {
  const read = {};
  const nodes = (fields || []).map(({ campo, nombre, bloqueado, valor }) => {
    if (!admin && bloqueado) {
      return h('div.panel', { style: { opacity: '.85' } },
        h('div.switch', h('span', h('strong', nombre), h('div.muted', `Fijado por el administrador: ${valor ? 'activado' : 'desactivado'}`)), h('span.pin.lock', 'Bloqueado')));
    }
    if (admin) {
      const current = values[campo] === true ? 'on' : values[campo] === false ? 'off' : '';
      const sel = select([['', 'No fijar (lo elige cada usuario)'], ['on', 'Activado por defecto'], ['off', 'Desactivado por defecto']], current);
      read[campo] = () => (sel.value === '' ? undefined : sel.value === 'on');
      return panel(field(nombre, sel));
    }
    let on = values[campo] === true;
    read[campo] = () => on;
    return toggle(nombre, on, (v) => { on = v; });
  });
  const container = h('div', nodes);
  container.collect = () => {
    const out = {};
    for (const [campo, get] of Object.entries(read)) {
      const v = get();
      if (v !== undefined) out[campo] = v;
    }
    return out;
  };
  return container;
}

/** Los dos apartados de cada pantalla de preferencias. */
const PARTS = {
  autocompletado: { title: 'Preferencias de autocompletado', sub: 'Datos que se rellenan solos al crear un DeCA', ico: 'pin' },
  visuales: { title: 'Preferencias visuales', sub: 'Cómo se muestran los DeCA en la app', ico: 'eye' },
};

/** Menú «Preferencias»: un botón por apartado. */
function menu({ title, back, base, intro }) {
  page({
    title,
    back,
    body: [
      intro,
      ...Object.entries(PARTS).map(([part, p]) => sign({ text: p.title, sub: p.sub, ico: p.ico, chev: true, onClick: () => go(`${base}/${part}`) })),
    ],
  });
}

/**
 * Página de un apartado. El servidor guarda todas las preferencias de un nivel de una
 * vez, así que al guardar un apartado se conservan tal cual los valores del otro.
 * @param {'autocompletado'|'visuales'} part
 */
async function screen({ part, back, load, save, intro, admin = false }) {
  if (!PARTS[part]) return go(back, { replace: true });
  const { title } = PARTS[part];
  page({ title, back, body: loading() });
  let data;
  try {
    data = await load();
  } catch (e) {
    return page({ title, back, body: errorView(e, () => screen({ part, back, load, save, intro, admin })) });
  }
  const ed = part === 'autocompletado'
    ? editor(data.fields, data.values, data.locked)
    : visualEditor(data.visual, data.values, { admin });
  const otherKeys = (part === 'autocompletado' ? data.visual : data.fields).map((f) => f.campo);
  const kept = Object.fromEntries(Object.entries(data.values).filter(([k]) => otherKeys.includes(k)));
  const btn = sign({ text: 'Guardar preferencias', ico: 'check', color: 'green', center: true });
  btn.onclick = () => busy(btn, async () => {
    await save({ ...kept, ...ed.collect() });
    toast('Preferencias guardadas', 'ok');
  });
  page({ title, back, body: [part === 'autocompletado' ? intro : null, ed], actions: [btn] });
}

const str = (v) => (v === null || v === undefined ? '' : String(v));

// ---------- Ajustes → Preferencias (propias) ----------

const OWN = '/ajustes/preferencias';

export function ownMenu() {
  menu({ title: 'Preferencias', back: '/ajustes', base: OWN });
}

export function ownView({ part }) {
  return screen({
    part,
    back: OWN,
    intro: notice('Los datos que fijes se rellenan solos al crear un DeCA. Siempre puedes cambiarlos en cada DeCA.', 'info', 'pin'),
    load: async () => {
      const r = await api('/api/me/preferences');
      const locked = {};
      for (const [k, v] of Object.entries({ ...r.fijadas_por_administrador.empresa, ...r.fijadas_por_administrador.trabajador })) locked[k] = str(v);
      return { fields: r.campos, visual: r.visuales, values: r.propias, locked };
    },
    save: async (v) => {
      await api('/api/me/preferences', { method: 'PUT', body: v });
      // Las preferencias visuales efectivas llegan con los datos del usuario.
      setUser(await api('/api/auth/me'));
    },
  });
}

// ---------- Administración: preferencias de la empresa ----------

export function companyMenu({ cid }) {
  menu({
    title: 'Preferencias de la empresa',
    back: `/empresa/${cid}/preferencias`,
    base: `/empresa/${cid}/preferencias/empresa`,
    intro: notice('Se aplican a todos los trabajadores de la empresa. Ellos no podrán cambiar estas preferencias.', 'info', 'building'),
  });
}

export function companyView({ cid, part }) {
  return screen({
    part,
    back: `/empresa/${cid}/preferencias/empresa`,
    intro: notice('Se aplican a todos los trabajadores de la empresa. Ellos no podrán cambiar estas preferencias.', 'info', 'building'),
    load: async () => {
      const r = await api(`/api/companies/${cid}/preferences`);
      return { fields: r.campos, visual: r.visuales, values: r.fijadas, locked: {} };
    },
    admin: true,
    save: (v) => api(`/api/companies/${cid}/preferences`, { method: 'PUT', body: v }),
  });
}

// ---------- Administración: preferencias de un trabajador ----------

export function workerMenu({ wid }) {
  menu({
    title: 'Preferencias del trabajador',
    back: `/trabajador/${wid}`,
    base: `/trabajador/${wid}/preferencias`,
    intro: notice('Solo para este trabajador. Tienen prioridad sobre las de la empresa.', 'info', 'user'),
  });
}

export function workerView({ wid, part }) {
  return screen({
    part,
    back: `/trabajador/${wid}/preferencias`,
    intro: notice('Solo para este trabajador. Tienen prioridad sobre las de la empresa.', 'info', 'user'),
    load: async () => {
      const r = await api(`/api/workers/${wid}/preferences`);
      return { fields: r.campos, visual: r.visuales, values: r.fijadas, locked: {} };
    },
    admin: true,
    save: (v) => api(`/api/workers/${wid}/preferences`, { method: 'PUT', body: v }),
  });
}
