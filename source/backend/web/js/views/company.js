// Ajustes -> <Nombre de la empresa> (solo administrador).

import { h } from '../core/dom.js';
import { api, cachedGet } from '../core/api.js';
import { page, sign, section, loading, errorView, panel, field, input, select, busy, toast, confirm, listItem, notice } from '../core/ui.js';
import { go } from '../core/router.js';
import { ticket } from '../components/ticket.js';
import { hoy } from '../core/format.js';

async function companyName(cid) {
  const list = await cachedGet('/api/companies', 'companies').catch(() => []);
  return (list.find((c) => String(c.id) === String(cid)) || {}).name || 'Empresa';
}

export async function menuView({ cid }) {
  const name = await companyName(cid);
  page({
    title: name,
    back: '/ajustes',
    body: [
      sign({ text: 'Trabajadores', sub: 'Crear cuentas, código QR de acceso', ico: 'users', chev: true, onClick: () => go(`/empresa/${cid}/trabajadores`) }),
      sign({ text: 'Administración de Preferencias', sub: 'Autocompletado y preferencias visuales', ico: 'pin', chev: true, onClick: () => go(`/empresa/${cid}/preferencias`) }),
      sign({ text: 'Administración de trabajadores', sub: 'DeCA emitidos por cada trabajador', ico: 'deca', chev: true, onClick: () => go(`/empresa/${cid}/administracion`) }),
      sign({ text: 'Datos de la empresa', sub: 'Nombre, CIF, domicilio, dueño de los vehículos', ico: 'building', color: 'white', chev: true, onClick: () => go(`/empresa/${cid}/datos`) }),
    ],
  });
}

export async function workersMenuView({ cid }) {
  const name = await companyName(cid);
  page({
    title: `${name} · Trabajadores`,
    back: `/empresa/${cid}`,
    body: [sign({ text: 'Trabajadores en mi empresa', ico: 'users', chev: true, onClick: () => go(`/empresa/${cid}/trabajadores/lista`) })],
  });
}

export async function prefsMenuView({ cid }) {
  const name = await companyName(cid);
  page({
    title: `${name} · Preferencias`,
    back: `/empresa/${cid}`,
    body: [
      h('p.hint', 'Lo que fijes aquí se rellena solo en los DeCA y el trabajador no lo puede cambiar en sus preferencias (sí puede corregirlo en un DeCA concreto).'),
      sign({ text: 'Preferencias de la empresa', sub: 'Para todos los trabajadores', ico: 'building', chev: true, onClick: () => go(`/empresa/${cid}/preferencias/empresa`) }),
      sign({ text: 'Preferencias de un trabajador', sub: 'Solo para la persona que elijas', ico: 'user', chev: true, onClick: () => go(`/empresa/${cid}/preferencias/trabajador`) }),
    ],
  });
}

export async function adminMenuView({ cid }) {
  const name = await companyName(cid);
  page({
    title: `${name} · Administración`,
    back: `/empresa/${cid}`,
    body: [sign({ text: 'Administración de DeCA emitidos', ico: 'deca', chev: true, onClick: () => go(`/empresa/${cid}/decas`) })],
  });
}

function companyForm(c = {}) {
  const f = {
    nombre: input({ value: c.name || '' }),
    cif: input({ value: c.cif || '', plate: true }),
    domicilio: input({ value: c.domicilio || '' }),
    telefono: input({ value: c.telefono || '', type: 'tel' }),
    titular: input({ value: c.titular || '', placeholder: 'Persona o sociedad dueña de los camiones' }),
  };
  const node = panel(
    field('Nombre de la empresa transportista', f.nombre),
    field('CIF / NIF', f.cif),
    field('Domicilio', f.domicilio),
    field('Teléfono', f.telefono),
    field('Nombre transportista (dueño de los vehículos)', f.titular),
  );
  const values = () => Object.fromEntries(Object.entries(f).map(([k, el]) => [k, el.value.trim()]));
  return { node, values };
}

export async function createView() {
  const form = companyForm();
  const save = sign({ text: 'Crear empresa', ico: 'check', color: 'green', center: true });
  save.onclick = () => busy(save, async () => {
    const c = await api('/api/companies', { method: 'POST', body: form.values() });
    toast('Empresa creada', 'ok');
    go(`/empresa/${c.id}`, { replace: true });
  });
  page({ title: 'Nueva empresa', back: '/ajustes', body: [form.node], actions: [save] });
}

export async function dataView({ cid }) {
  page({ title: 'Datos de la empresa', back: `/empresa/${cid}`, body: loading() });
  let c;
  try {
    c = await api(`/api/companies/${cid}`);
  } catch (e) {
    return page({ title: 'Datos de la empresa', back: `/empresa/${cid}`, body: errorView(e, () => dataView({ cid })) });
  }
  const form = companyForm(c);
  const save = sign({ text: 'Guardar', ico: 'check', color: 'green', center: true });
  save.onclick = () => busy(save, async () => {
    await api(`/api/companies/${cid}`, { method: 'PUT', body: form.values() });
    toast('Guardado', 'ok');
  });
  const del = sign({
    text: 'Eliminar empresa', ico: 'trash', color: 'red', small: true,
    onClick: () => busy(del, async () => {
      if (!(await confirm(`¿Eliminar «${c.name}»? Solo se puede si no tiene trabajadores.`, { danger: true, ok: 'Eliminar' }))) return;
      await api(`/api/companies/${cid}`, { method: 'DELETE' });
      toast('Empresa eliminada', 'ok');
      go('/ajustes', { replace: true });
    }),
  });
  page({ title: 'Datos de la empresa', back: `/empresa/${cid}`, body: [form.node, h('div.mt', del)], actions: [save] });
}

/** Administración de DeCA emitidos: filtros por trabajador, fechas y matrícula. */
export async function decasView({ cid }, query) {
  page({ title: 'DeCA emitidos', back: `/empresa/${cid}/administracion`, body: loading() });
  let workers = [];
  try {
    workers = await api(`/api/companies/${cid}/workers`);
  } catch (e) {
    return page({ title: 'DeCA emitidos', back: `/empresa/${cid}/administracion`, body: errorView(e, () => decasView({ cid }, query)) });
  }
  const who = select([['', 'Todos los trabajadores'], ...workers.map((w) => [String(w.id), w.nombre])], query.trabajador || '');
  const from = input({ type: 'date', value: query.desde || '' });
  const to = input({ type: 'date', value: query.hasta || hoy() });
  const plate = input({ value: query.matricula || '', plate: true, placeholder: 'Cualquiera' });
  const results = h('div');
  let pageN = 1;

  async function load(reset) {
    if (reset) {
      pageN = 1;
      results.replaceChildren(loading());
    }
    const qs = new URLSearchParams({ pagina: String(pageN), por_pagina: '30' });
    if (who.value) qs.set('trabajador', who.value);
    if (from.value) qs.set('desde', from.value);
    if (to.value) qs.set('hasta', to.value);
    if (plate.value.trim()) qs.set('matricula', plate.value.trim());
    try {
      const r = await api(`/api/companies/${cid}/decas?${qs}`);
      if (reset) results.replaceChildren(h('p.hint', `${r.total} DeCA encontrados`));
      results.querySelector('.more')?.remove();
      for (const d of r.decas) results.appendChild(ticket(d, { onClick: () => go(`/deca/${d.id}`), showUser: true }));
      if (pageN * r.por_pagina < r.total) {
        results.appendChild(h('div.more', sign({ text: 'Ver más', ico: 'plus', color: 'white', small: true, onClick: () => { pageN++; load(false); } })));
      }
    } catch (e) {
      results.replaceChildren(...errorView(e, () => load(true)));
    }
  }

  const search = sign({ text: 'Buscar', ico: 'search', color: 'green', center: true, onClick: () => load(true) });
  page({
    title: 'DeCA emitidos',
    back: `/empresa/${cid}/administracion`,
    body: [
      panel(
        field('Trabajador', who),
        h('div.row.wrap', field('Desde', from), field('Hasta', to)),
        field('Matrícula del camión', plate),
      ),
      results,
    ],
    actions: [search],
  });
  load(true);
}

