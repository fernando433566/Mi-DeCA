// Ajustes -> Camiones y remolques (base de datos compartida).
// Cada camión puede tener un remolque predeterminado que se rellena solo.

import { h } from '../core/dom.js';
import { api } from '../core/api.js';
import { page, sign, section, panel, field, input, select, toast, modal, loading, errorView, listItem, confirm } from '../core/ui.js';
import * as catalog from '../components/catalog.js';
import { state, isAdmin } from '../core/state.js';

async function editTruck(t, trailers) {
  const name = input({ value: t?.nombre || '', placeholder: 'Ej.: Volvo azul' });
  const plate = input({ value: t?.matricula || '', plate: true, placeholder: '1234 BCD' });
  const trailer = select([['', 'Sin remolque predeterminado'], ...trailers.map((r) => [String(r.id), `${r.nombre} (${r.matricula})`])], t?.remolque_id ? String(t.remolque_id) : '');
  const buttons = [{ text: 'Cancelar', value: 'no', color: 'white' }, { text: 'Guardar', value: 'save', color: 'green' }];
  if (t) buttons.unshift({ text: 'Borrar', value: 'del', color: 'red' });
  const v = await modal({
    title: t ? 'Editar camión' : 'Añadir camión',
    body: [field('Nombre', name), field('Matrícula', plate), field('Remolque predeterminado', trailer, { hint: 'Al elegir este camión en un DeCA, el remolque se rellena solo.' })],
    buttons,
  });
  try {
    if (v === 'save') {
      const body = { nombre: name.value.trim(), matricula: plate.value, remolque_id: trailer.value ? Number(trailer.value) : null };
      await api(t ? `/api/catalog/trucks/${t.id}` : '/api/catalog/trucks', { method: t ? 'PUT' : 'POST', body });
      toast('Camión guardado', 'ok');
    } else if (v === 'del' && (await confirm(`¿Borrar el camión ${t.matricula}?`, { danger: true, ok: 'Borrar' }))) {
      await api(`/api/catalog/trucks/${t.id}`, { method: 'DELETE' });
      toast('Camión borrado', 'ok');
    }
  } catch (e) {
    toast(e.message, 'err');
  }
}

async function editTrailer(r) {
  const name = input({ value: r?.nombre || '', placeholder: 'Ej.: Bañera 1' });
  const plate = input({ value: r?.matricula || '', plate: true, placeholder: 'R 1234 BCD' });
  const buttons = [{ text: 'Cancelar', value: 'no', color: 'white' }, { text: 'Guardar', value: 'save', color: 'green' }];
  if (r) buttons.unshift({ text: 'Borrar', value: 'del', color: 'red' });
  const v = await modal({ title: r ? 'Editar remolque' : 'Añadir remolque', body: [field('Nombre', name), field('Matrícula', plate)], buttons });
  try {
    if (v === 'save') {
      await api(r ? `/api/catalog/trailers/${r.id}` : '/api/catalog/trailers', { method: r ? 'PUT' : 'POST', body: { nombre: name.value.trim(), matricula: plate.value } });
      toast('Remolque guardado', 'ok');
    } else if (v === 'del' && (await confirm(`¿Borrar el remolque ${r.matricula}?`, { danger: true, ok: 'Borrar' }))) {
      await api(`/api/catalog/trailers/${r.id}`, { method: 'DELETE' });
      toast('Remolque borrado', 'ok');
    }
  } catch (e) {
    toast(e.message, 'err');
  }
}

export async function view() {
  page({ title: 'Camiones y remolques', back: '/ajustes', body: loading() });
  let trucks;
  let trailers;
  try {
    [trucks, trailers] = await Promise.all([api('/api/catalog/trucks'), api('/api/catalog/trailers')]);
  } catch (e) {
    return page({ title: 'Camiones y remolques', back: '/ajustes', body: errorView(e, view) });
  }
  const refresh = async () => { catalog.invalidate('trucks'); catalog.invalidate('trailers'); await view(); };
  page({
    title: 'Camiones y remolques',
    back: '/ajustes',
    body: [
      h('p.hint', 'Los camiones y remolques que añadas los pueden usar todos los trabajadores.'),
      section('Camiones'),
      sign({ text: 'Añadir camión', ico: 'plus', color: 'green', small: true, onClick: async () => { await editTruck(null, trailers); refresh(); } }),
      h('div.mt', trucks.map((t) => listItem({
        title: `${t.nombre}`,
        sub: `${t.matricula}${t.remolque_matricula ? ' · remolque ' + t.remolque_matricula : ''}`,
        ico: 'truck',
        onClick: async () => { await editTruck(t, trailers); refresh(); },
      }))),
      section('Remolques'),
      sign({ text: 'Añadir remolque', ico: 'plus', color: 'green', small: true, onClick: async () => { await editTrailer(null); refresh(); } }),
      h('div.mt', trailers.map((r) => listItem({ title: r.nombre, sub: r.matricula, ico: 'truck', onClick: async () => { await editTrailer(r); refresh(); } }))),
      !trucks.length && !trailers.length ? panel(h('p.muted.center', 'Todavía no hay vehículos. También se guardan solos al emitir un DeCA.')) : null,
    ],
  });

}
