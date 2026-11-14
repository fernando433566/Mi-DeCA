// Crear o modificar un DeCA: asistente por pasos con botones grandes.
// Los datos fijados se rellenan solos (y se pueden cambiar). Se guarda un borrador.
// En Android, si no hay conexión, el DeCA queda en cola y se crea al volver la cobertura.

import { h, icon } from '../core/dom.js';
import { api, cache, cachedGet } from '../core/api.js';
import { page, sign, field, input, textarea, select, panel, notice, loading, errorView, toast, modal, busy, section } from '../core/ui.js';
import { go } from '../core/router.js';
import { isAndroid, native } from '../core/native.js';
import { isAdmin, setNet } from '../core/state.js';
import { combo, matches } from '../components/combo.js';
import { addressInput } from '../components/address.js';
import * as catalog from '../components/catalog.js';
import { hoy, parseNum, uuid, fecha, kg, num } from '../core/format.js';
import { showOnWidget, refreshLatest } from '../components/latest.js';
import { enqueue, queueAvailable, sendsInBackground } from '../core/queue.js';
import { plate as normPlate } from '../core/validate.js';

const DRAFT = 'draft';

/** Campo de preferencias -> ruta dentro de los datos del DeCA. */
const PIN_MAP = {
  cargador_empresa: 'cargador.nombre', cargador_nif: 'cargador.nif', cargador_domicilio: 'cargador.domicilio',
  transportista_empresa: 'transportista.nombre', transportista_nif: 'transportista.nif',
  transportista_domicilio: 'transportista.domicilio', transportista_titular: 'transportista.titular',
  conductor: 'conductor', mercancia: 'envio.mercancia', peso_kg: 'envio.peso_kg', volumen_m3: 'envio.volumen_m3',
  origen: 'envio.origen', destino: 'envio.destino', matricula_camion: 'matricula_camion',
  matricula_remolque: 'matricula_remolque', autorizacion_especial: 'autorizacion_especial',
};

function emptyEnvio() {
  return { origen: '', destino: '', mercancia: '', peso_kg: null, volumen_m3: null };
}

function fromTemplate(t) {
  const d = JSON.parse(JSON.stringify(t || {}));
  d.cargador = d.cargador || { nombre: '', nif: '', domicilio: '' };
  d.transportista = d.transportista || { nombre: '', nif: '', domicilio: '', titular: '' };
  d.envios = d.envios && d.envios.length ? d.envios : [emptyEnvio()];
  d.fecha_transporte = d.fecha_transporte || hoy();
  if (!d.fecha_transporte || d.fecha_transporte < hoy()) d.fecha_transporte = hoy();
  delete d.numero_viajes; // campo de versiones anteriores (viajes de ida y vuelta)
  return d;
}

/** Limpia el borrador antes de enviarlo al servidor. */
function toPayload(d) {
  const opt = (v) => (v && String(v).trim() ? String(v).trim() : null);
  return {
    cargador: { ...d.cargador },
    transportista: { ...d.transportista, titular: opt(d.transportista.titular) },
    conductor: opt(d.conductor),
    envios: d.envios.map((e) => ({
      origen: e.origen, destino: e.destino, mercancia: e.mercancia,
      peso_kg: Number(e.peso_kg), volumen_m3: e.volumen_m3 === null || e.volumen_m3 === '' ? null : Number(e.volumen_m3),
    })),
    fecha_transporte: d.fecha_transporte,
    fecha_fin: opt(d.fecha_fin),
    matricula_camion: normPlate(d.matricula_camion),
    matricula_remolque: opt(normPlate(d.matricula_remolque)),
    autorizacion_especial: opt(d.autorizacion_especial),
    observaciones: opt(d.observaciones),
  };
}

function need(v, label) {
  if (v === null || v === undefined || String(v).trim() === '') throw new Error(`Falta rellenar: ${label}`);
}

// ======================= Pasos =======================

function stepVehicle(ctx) {
  const { d, pin } = ctx;
  const truck = input({ value: d.matricula_camion, plate: true, placeholder: '1234 BCD' });
  const trailer = input({ value: d.matricula_remolque || '', plate: true, placeholder: 'R 1234 BCD (opcional)' });
  truck.oninput = () => (d.matricula_camion = truck.value);
  trailer.oninput = () => (d.matricula_remolque = trailer.value);
  const truckCombo = combo({
    input: truck,
    suggest: async (q) => (await catalog.load('trucks'))
      .filter((t) => !q || matches(q, t.nombre, t.matricula))
      .map((t) => ({ label: `${t.nombre}`, sub: `${t.matricula}${t.remolque_matricula ? ' · remolque ' + t.remolque_matricula : ''}`, value: t })),
    onPick: ({ value: t }) => {
      d.matricula_camion = t.matricula;
      truck.value = t.matricula;
      if (t.remolque_matricula) {
        d.matricula_remolque = t.remolque_matricula;
        trailer.value = t.remolque_matricula;
        toast(`Remolque ${t.remolque_matricula} rellenado`, 'ok');
      }
    },
  });
  const trailerCombo = combo({
    input: trailer,
    suggest: async (q) => (await catalog.load('trailers'))
      .filter((t) => !q || matches(q, t.nombre, t.matricula))
      .map((t) => ({ label: t.nombre, sub: t.matricula, value: t })),
    onPick: ({ value: t }) => { d.matricula_remolque = t.matricula; trailer.value = t.matricula; },
  });
  const driver = input({ value: d.conductor || '', placeholder: 'Nombre del conductor' });
  driver.oninput = () => (d.conductor = driver.value);
  return {
    title: 'Vehículo', ico: 'truck',
    nodes: [panel(
      field('Matrícula del camión', truckCombo, { pin: pin('matricula_camion'), hint: 'Elige un camión guardado o escribe la matrícula.' }),
      field('Matrícula del remolque', trailerCombo, { pin: pin('matricula_remolque') }),
      field('Conductor', driver, { pin: pin('conductor') }),
    )],
    validate: () => need(d.matricula_camion, 'Matrícula del camión'),
  };
}

function partyStep(ctx, kind) {
  const { d, pin } = ctx;
  const p = d[kind];
  const isT = kind === 'transportista';
  const name = input({ value: p.nombre, placeholder: isT ? 'Empresa que transporta' : 'Empresa que carga el camión' });
  const nif = input({ value: p.nif, plate: true, placeholder: 'B12345678' });
  const dom = input({ value: p.domicilio, placeholder: 'Calle, número, población' });
  const tit = isT ? input({ value: p.titular || '', placeholder: 'Dueño de los vehículos (opcional)' }) : null;
  name.oninput = () => (p.nombre = name.value);
  nif.oninput = () => (p.nif = nif.value);
  dom.oninput = () => (p.domicilio = dom.value);
  if (tit) tit.oninput = () => (p.titular = tit.value);
  const nameCombo = combo({
    input: name,
    suggest: async (q) => (await catalog.load('parties'))
      .filter((x) => x.tipo === kind && (!q || matches(q, x.nombre, x.nif)))
      .map((x) => ({ label: x.nombre, sub: `${x.nif} · ${x.domicilio}`, value: x })),
    onPick: ({ value: x }) => {
      Object.assign(p, { nombre: x.nombre, nif: x.nif, domicilio: x.domicilio });
      name.value = x.nombre; nif.value = x.nif; dom.value = x.domicilio;
      if (tit && x.titular) { p.titular = x.titular; tit.value = x.titular; }
      // Como el remolque de un camión: la ubicación de la cargadora se rellena sola.
      const places = x.ubicaciones || [];
      if (!isT && places.length === 1 && !ctx.d.envios[0].origen && !pin('origen')) {
        ctx.d.envios[0].origen = places[0].direccion;
        toast(`Origen rellenado: ${places[0].nombre}`, 'ok');
      } else if (places.length > 1) {
        toast(`${x.nombre} tiene ${places.length} ubicaciones: aparecerán al elegir origen y destino`);
      }
    },
  });
  const pre = isT ? 'transportista' : 'cargador';
  return {
    title: isT ? 'Empresa transportista' : 'Empresa cargadora',
    ico: isT ? 'truck' : 'building',
    nodes: [
      h('p.hint', isT ? 'La empresa que hace el transporte (transportista efectivo).' : 'La empresa que carga el camión (cargador contractual).'),
      panel(
        field(isT ? 'Empresa transportista' : 'Empresa cargadora', nameCombo, { pin: pin(isT ? 'transportista_empresa' : 'cargador_empresa') }),
        field(isT ? 'NIF transportista' : 'NIF / CIF', nif, { pin: pin(`${pre}_nif`) }),
        field('Domicilio', dom, { pin: pin(`${pre}_domicilio`) }),
        tit ? field('Nombre transportista (dueño de los vehículos)', tit, { pin: pin('transportista_titular') }) : null,
      ),
    ],
    validate: () => {
      need(p.nombre, isT ? 'Empresa transportista' : 'Empresa cargadora');
      need(p.nif, 'NIF');
      need(p.domicilio, 'Domicilio');
    },
  };
}

/** Ubicaciones de la empresa cargadora y de la transportista elegidas en este DeCA. */
async function companyPlaces(d) {
  const parties = await catalog.load('parties');
  const same = (a, b) => String(a || '').trim().toUpperCase() === String(b || '').trim().toUpperCase();
  const chosen = parties.filter((p) => (p.tipo === 'cargador' && same(p.nif, d.cargador.nif))
    || (p.tipo === 'transportista' && same(p.nif, d.transportista.nif)));
  return chosen.flatMap((p) => p.ubicaciones || []);
}

function stepLoad(ctx) {
  const { d, pin, rerender } = ctx;
  const nodes = [];
  d.envios.forEach((e, i) => {
    const goods = input({ value: e.mercancia, placeholder: 'Ej.: Minerales a granel' });
    goods.oninput = () => (e.mercancia = goods.value);
    const goodsCombo = combo({
      input: goods,
      suggest: async (q) => (await catalog.load('goods')).filter((g) => !q || matches(q, g.descripcion)).map((g) => ({ label: g.descripcion, value: g.descripcion })),
      onPick: ({ value }) => { e.mercancia = value; goods.value = value; },
    });
    const weight = input({ value: e.peso_kg ?? '', inputmode: 'decimal', placeholder: '24000' });
    weight.oninput = () => (e.peso_kg = parseNum(weight.value));
    const vol = input({ value: e.volumen_m3 ?? '', inputmode: 'decimal', placeholder: 'Opcional' });
    vol.oninput = () => (e.volumen_m3 = parseNum(vol.value));
    const first = i === 0;
    nodes.push(
      d.envios.length > 1 ? section(`Envío ${i + 1}`) : null,
      panel(
        field('Origen (dirección completa)', addressInput({ value: e.origen, onChange: (v) => (e.origen = v), companyPlaces: () => companyPlaces(d) }), { pin: first && pin('origen') }),
        field('Destino (dirección completa)', addressInput({ value: e.destino, onChange: (v) => (e.destino = v), companyPlaces: () => companyPlaces(d) }), { pin: first && pin('destino') }),
        field('Mercancía', goodsCombo, { pin: first && pin('mercancia') }),
        h('div.row',
          field('Peso (kg)', weight, { pin: first && pin('peso_kg') }),
          field('Volumen (m³)', vol, { pin: first && pin('volumen_m3') }),
        ),
        d.envios.length > 1
          ? sign({ text: 'Quitar este envío', ico: 'trash', color: 'red', small: true, onClick: () => { d.envios.splice(i, 1); rerender(); } })
          : null,
      ),
    );
  });
  nodes.push(sign({
    text: 'Añadir otro envío', sub: 'Mismo cargador y transportista', ico: 'plus', color: 'white', small: true,
    onClick: () => {
      const last = d.envios[d.envios.length - 1];
      d.envios.push({ ...emptyEnvio(), origen: last.origen, mercancia: last.mercancia });
      rerender();
    },
  }));
  return {
    title: 'Carga y ruta', ico: 'route', nodes,
    validate: () => d.envios.forEach((e, i) => {
      const n = d.envios.length > 1 ? ` (envío ${i + 1})` : '';
      need(e.origen, `Origen${n}`);
      need(e.destino, `Destino${n}`);
      need(e.mercancia, `Mercancía${n}`);
      if (!(Number(e.peso_kg) > 0)) throw new Error(`Escribe el peso en kg${n}`);
      if (Number.isNaN(e.volumen_m3)) throw new Error(`Volumen no válido${n}`);
    }),
  };
}

function stepDates(ctx) {
  const { d, pin, companies, mode } = ctx;
  const date = input({ type: 'date', value: d.fecha_transporte });
  date.oninput = () => (d.fecha_transporte = date.value);
  const end = input({ type: 'date', value: d.fecha_fin || '' });
  end.oninput = () => (d.fecha_fin = end.value || null);
  const auth = input({ value: d.autorizacion_especial || '', placeholder: 'Solo si procede' });
  auth.oninput = () => (d.autorizacion_especial = auth.value);
  const obs = textarea({ value: d.observaciones || '' });
  obs.oninput = () => (d.observaciones = obs.value);
  let compSel = null;
  if (mode === 'create' && isAdmin() && companies.length > 1) {
    compSel = select(companies.map((c) => [String(c.id), c.name]), String(d.empresa_id || companies[0].id));
    compSel.onchange = () => (d.empresa_id = Number(compSel.value));
  }
  return {
    title: 'Fecha', ico: 'calendar',
    nodes: [panel(
      field('Fecha de realización (día que sales)', date),
      field('Fin previsto del servicio', end, { hint: 'Opcional. Si lo dejas vacío, el mismo día.' }),
      field('Autorización especial de circulación', auth, { pin: pin('autorizacion_especial') }),
      field('Observaciones', obs),
      compSel ? field('Empresa que emite', compSel) : null,
    )],
    validate: () => {
      need(d.fecha_transporte, 'Fecha de realización');
      if (d.fecha_fin && d.fecha_fin < d.fecha_transporte) throw new Error('La fecha de fin no puede ser anterior a la de inicio');
    },
  };
}

function stepReview(ctx) {
  const { d, mode } = ctx;
  const kvs = (pairs) => h('dl.kv', pairs.filter(([, v]) => v).flatMap(([k, v]) => [h('dt', k), h('dd', String(v))]));
  const nodes = [
    panel(h('h3', 'Empresa cargadora'), kvs([['Nombre', d.cargador.nombre], ['NIF', d.cargador.nif], ['Domicilio', d.cargador.domicilio]])),
    panel(h('h3', 'Empresa transportista'), kvs([['Nombre', d.transportista.nombre], ['NIF', d.transportista.nif], ['Domicilio', d.transportista.domicilio], ['Nombre transportista', d.transportista.titular]])),
    panel(h('h3', 'Vehículo y fecha'), kvs([
      ['Camión', normPlate(d.matricula_camion)], ['Remolque', normPlate(d.matricula_remolque)], ['Conductor', d.conductor],
      ['Fecha', fecha(d.fecha_transporte)], ['Fin', d.fecha_fin ? fecha(d.fecha_fin) : ''],
      ['Autorización', d.autorizacion_especial],
    ])),
    ...d.envios.map((e, i) => panel(h('h3', d.envios.length > 1 ? `Envío ${i + 1}` : 'Envío'), kvs([
      ['Origen', e.origen], ['Destino', e.destino], ['Mercancía', e.mercancia], ['Peso', kg(e.peso_kg)], ['Volumen', e.volumen_m3 ? `${num(e.volumen_m3)} m³` : ''],
    ]))),
  ];
  let motivo = null;
  if (mode === 'modify') {
    motivo = textarea({ value: ctx.motivo || '', placeholder: 'Ej.: Cambio de remolque por avería' });
    motivo.oninput = () => (ctx.motivo = motivo.value);
    nodes.unshift(
      notice('Se mantiene el mismo QR. En el PDF, los datos anteriores aparecerán tachados como NO VÁLIDOS junto con el motivo.', 'info', 'edit'),
      panel(field('Motivo de la modificación', motivo)),
    );
  }
  return {
    title: mode === 'modify' ? 'Revisar cambios' : 'Revisar y emitir', ico: 'check', nodes,
    validate: () => { if (mode === 'modify') need(ctx.motivo, 'Motivo de la modificación'); },
  };
}

// ======================= Asistente =======================

function wizard(ctx) {
  const steps = [stepVehicle, (c) => partyStep(c, 'cargador'), (c) => partyStep(c, 'transportista'), stepLoad, stepDates, stepReview];
  const draftSave = () => ctx.mode === 'create' && cache.set(DRAFT, { d: ctx.d, step: ctx.step, ref: ctx.ref });

  const render = () => {
    const st = steps[ctx.step](ctx);
    const last = ctx.step === steps.length - 1;
    const backBtn = sign({
      text: ctx.step === 0 ? 'Cancelar' : 'Atrás', ico: 'back', color: 'white', center: true,
      onClick: () => (ctx.step === 0 ? leave() : (ctx.step--, draftSave(), render())),
    });
    const nextBtn = last
      ? sign({ text: ctx.mode === 'modify' ? 'Guardar cambios' : 'Emitir DeCA', ico: 'send', color: 'green', center: true, onClick: () => busy(nextBtn, submit) })
      : sign({ text: 'Siguiente', ico: 'chev', color: 'green', center: true, onClick: next });
    function next() {
      try {
        st.validate();
      } catch (e) {
        return toast(e.message, 'err');
      }
      ctx.step++;
      draftSave();
      render();
    }
    async function submit() {
      st.validate();
      for (const s of steps.slice(0, -1)) s(ctx).validate();
      await (ctx.mode === 'modify' ? doModify(ctx) : doCreate(ctx));
    }
    page({
      title: ctx.mode === 'modify' ? `Modificar ${ctx.numero}` : 'Nuevo DeCA',
      back: ctx.mode === 'modify' ? `/deca/${ctx.id}` : '/',
      body: [
        h('div.steps', steps.map((_, i) => h('i', { class: i < ctx.step ? 'done' : i === ctx.step ? 'now' : '' }))),
        h('div.step-title', icon(st.ico), h('span', `${ctx.step + 1}. ${st.title}`)),
        ...st.nodes,
      ],
      actions: [backBtn, nextBtn],
    });
  };
  ctx.rerender = () => { draftSave(); render(); };

  async function leave() {
    if (ctx.mode === 'create') {
      const v = await modal({
        title: '¿Salir del nuevo DeCA?',
        body: h('p', 'Los datos se guardan como borrador por si vuelves.'),
        buttons: [{ text: 'Seguir aquí', value: false, color: 'white' }, { text: 'Salir', value: true, color: 'red' }],
      });
      if (!v) return;
    }
    go(ctx.mode === 'modify' ? `/deca/${ctx.id}` : '/');
  }
  render();
}

async function doCreate(ctx) {
  const d = ctx.d;
  const payload = { ...toPayload(d), empresa_id: d.empresa_id || undefined, client_ref: ctx.ref };
  try {
    const res = await api('/api/decas', { method: 'POST', body: payload, timeout: 30000 });
    setNet(true);
    cache.set(DRAFT, null);
    catalog.invalidate('trucks'); catalog.invalidate('parties'); catalog.invalidate('places'); catalog.invalidate('goods');
    // La respuesta es una lista con el DeCA creado.
    const first = res.decas[0];
    cache.set('ultimo', [first]);
    showOnWidget(first);
    if (res.aviso) await modal({ title: 'Atención', body: notice(res.aviso), buttons: [{ text: 'Entendido', value: true, color: 'green' }] });
    toast(`DeCA ${first.numero} creado`, 'ok');
    go(`/deca/${first.id}`, { replace: true });
  } catch (e) {
    if (!e.offline) throw e;
    setNet(false);
    if (queueAvailable()) {
      enqueue(payload);
      cache.set(DRAFT, null);
      await modal({
        title: 'Sin conexión',
        body: [
          notice(sendsInBackground()
            ? 'El DeCA se ha guardado y se creará automáticamente en cuanto vuelva la conexión. Te avisaremos con una notificación.'
            : 'El DeCA se ha guardado. Se creará cuando haya conexión y la aplicación esté abierta: ábrela en cuanto recuperes la señal.', 'info', 'wifioff'),
          notice('Hasta tener el QR el DeCA NO es válido: no inicies el transporte.', 'danger'),
        ],
        buttons: [{ text: 'Entendido', value: true, color: 'green' }],
      });
      go('/', { replace: true });
    } else {
      throw new Error('Sin conexión con el servidor. Tus datos siguen aquí: vuelve a pulsar «Emitir DeCA» cuando tengas conexión.');
    }
  }
}

async function doModify(ctx) {
  try {
    await api(`/api/decas/${ctx.id}`, { method: 'PUT', body: { datos: toPayload(ctx.d), motivo: ctx.motivo } });
    refreshLatest();
    toast('DeCA modificado. El QR sigue siendo el mismo.', 'ok');
    go(`/deca/${ctx.id}`, { replace: true });
  } catch (e) {
    if (e.offline) throw new Error('Sin conexión. Las modificaciones necesitan conexión con el servidor.');
    throw e;
  }
}

async function loadContext() {
  const [eff, companies] = await Promise.all([
    cachedGet('/api/decas/plantilla', 'plantilla'),
    isAdmin() ? cachedGet('/api/companies', 'companies').catch(() => []) : Promise.resolve([]),
  ]);
  const pins = {};
  for (const c of eff.campos || []) {
    if (c.valor === null || c.valor === undefined || c.origen === 'automatica') continue;
    pins[c.campo] = c.bloqueado ? 'admin' : 'fijado';
  }
  return { eff, companies, pin: (k) => pins[k] || null };
}

export async function createView() {
  page({ title: 'Nuevo DeCA', back: '/', body: loading() });
  let base;
  try {
    base = await loadContext();
  } catch (e) {
    return page({ title: 'Nuevo DeCA', back: '/', body: errorView(e, createView) });
  }
  catalog.loadAll().catch(() => {});
  const draft = cache.get(DRAFT);
  const ctx = { ...base, mode: 'create', step: 0, ref: uuid(), d: fromTemplate(base.eff.plantilla) };
  if (draft && draft.d) {
    const v = await modal({
      title: 'Tienes un DeCA a medio rellenar',
      body: h('p', '¿Quieres seguir donde lo dejaste?'),
      buttons: [{ text: 'Empezar de nuevo', value: false, color: 'white' }, { text: 'Continuar', value: true, color: 'green' }],
    });
    if (v) Object.assign(ctx, { d: draft.d, step: draft.step || 0, ref: draft.ref || ctx.ref });
    else cache.set(DRAFT, null);
  }
  wizard(ctx);
}

export async function modifyView({ id }) {
  page({ title: 'Modificar DeCA', back: `/deca/${id}`, body: loading() });
  try {
    const [res, base] = await Promise.all([api(`/api/decas/${id}`), loadContext()]);
    const d = fromTemplate(res.deca.datos);
    d.fecha_transporte = res.deca.datos.fecha_transporte; // se respeta la fecha original
    wizard({ ...base, mode: 'modify', step: 0, id, numero: res.deca.numero, d, motivo: '' });
  } catch (e) {
    page({ title: 'Modificar DeCA', back: `/deca/${id}`, body: errorView(e, () => modifyView({ id })) });
  }
}
