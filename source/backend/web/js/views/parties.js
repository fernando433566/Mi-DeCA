// Ajustes -> Empresas y ubicaciones (base de datos compartida).
// Cada empresa puede tener sus ubicaciones: al elegirla en un DeCA se proponen como dirección.

import { h, clear } from '../core/dom.js';
import { api } from '../core/api.js';
import { page, sign, section, field, input, toast, modal, loading, errorView, listItem, confirm, panel } from '../core/ui.js';
import { addressInput } from '../components/address.js';
import * as catalog from '../components/catalog.js';

async function editParty(tipo, party) {
  const isT = tipo === 'transportista';
  const name = input({ value: party?.nombre || '', placeholder: isT ? 'Empresa que transporta' : 'Empresa que carga el camión' });
  const nif = input({ value: party?.nif || '', plate: true, placeholder: 'B12345678' });
  const dom = input({ value: party?.domicilio || '', placeholder: 'Calle, número, población' });
  const titular = isT ? input({ value: party?.titular || '', placeholder: 'Dueño de los vehículos (opcional)' }) : null;

  const places = (party?.ubicaciones || []).map((u) => u.direccion);
  const list = h('div');
  const draw = () => {
    clear(list);
    if (!places.length) list.appendChild(h('p.hint', 'Todavía no tiene ubicaciones.'));
    places.forEach((address, i) => list.appendChild(h('div.place-row',
      h('span', address),
      sign({ text: 'Quitar', ico: 'trash', color: 'red', small: true, onClick: () => { places.splice(i, 1); draw(); } }),
    )));
  };
  draw();
  const newPlace = addressInput({});
  const add = sign({
    text: 'Añadir ubicación', ico: 'plus', color: 'white', small: true,
    onClick: () => {
      const v = newPlace.getValue();
      if (!v) return toast('Escribe la dirección completa', 'err');
      if (!places.includes(v)) places.push(v);
      newPlace.setValue('');
      draw();
    },
  });

  const buttons = [{ text: 'Cancelar', value: 'no', color: 'white' }, { text: 'Guardar', value: 'save', color: 'green' }];
  if (party) buttons.unshift({ text: 'Borrar', value: 'del', color: 'red' });
  const v = await modal({
    title: party ? 'Editar empresa' : (isT ? 'Añadir empresa transportista' : 'Añadir empresa cargadora'),
    body: [
      field('Nombre de la empresa', name),
      field('NIF / CIF', nif),
      field('Domicilio', dom),
      titular ? field('Nombre transportista (dueño de los vehículos)', titular) : null,
      h('h3', 'Ubicaciones'),
      list,
      h('div.field', newPlace),
      add,
    ],
    buttons,
  });
  try {
    if (v === 'save') {
      const pending = newPlace.getValue();
      if (pending && !places.includes(pending)) places.push(pending);
      const body = { tipo, nombre: name.value.trim(), nif: nif.value.trim(), domicilio: dom.value.trim(), titular: titular ? titular.value.trim() : '', ubicaciones: places };
      await api(party ? `/api/catalog/parties/${party.id}` : '/api/catalog/parties', { method: party ? 'PUT' : 'POST', body });
      toast('Empresa guardada', 'ok');
    } else if (v === 'del' && (await confirm(`¿Borrar «${party.nombre}»?`, { danger: true, ok: 'Borrar' }))) {
      await api(`/api/catalog/parties/${party.id}`, { method: 'DELETE' });
      toast('Empresa borrada', 'ok');
    }
  } catch (e) {
    toast(e.message, 'err');
  }
}

function group(title, tipo, all, refresh) {
  const items = all.filter((p) => p.tipo === tipo);
  const n = (p) => (p.ubicaciones.length === 1 ? '1 ubicación' : `${p.ubicaciones.length} ubicaciones`);
  return [
    section(title),
    sign({ text: `Añadir empresa ${tipo === 'cargador' ? 'cargadora' : 'transportista'}`, ico: 'plus', color: 'green', small: true, onClick: async () => { await editParty(tipo, null); refresh(); } }),
    h('div.mt', items.map((p) => listItem({ title: p.nombre, sub: `${p.nif} · ${n(p)}`, ico: 'building', onClick: async () => { await editParty(tipo, p); refresh(); } }))),
  ];
}

export async function view() {
  page({ title: 'Empresas y ubicaciones', back: '/ajustes', body: loading() });
  let parties;
  try {
    parties = await api('/api/catalog/parties');
  } catch (e) {
    return page({ title: 'Empresas y ubicaciones', back: '/ajustes', body: errorView(e, view) });
  }
  const refresh = async () => { catalog.invalidate('parties'); catalog.invalidate('places'); await view(); };
  page({
    title: 'Empresas y ubicaciones',
    back: '/ajustes',
    body: [
      h('p.hint', 'Al elegir una empresa en un DeCA, sus ubicaciones se proponen como origen y destino. Las pueden usar todos los trabajadores.'),
      ...group('Empresas cargadoras', 'cargador', parties, refresh),
      ...group('Empresas transportistas', 'transportista', parties, refresh),
      !parties.length ? panel(h('p.muted.center', 'Todavía no hay empresas. También se guardan solas al emitir un DeCA.')) : null,
    ],
  });
}
