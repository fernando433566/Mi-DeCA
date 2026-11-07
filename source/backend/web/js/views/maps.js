// Ajustes -> Mapas. Como en Organic Maps: el mapa se divide en regiones y cada una
// se descarga una vez para usarla sin conexión (app Android). En la web se puede
// pedir al servidor que prepare las regiones.

import { h } from '../core/dom.js';
import { api } from '../core/api.js';
import { page, sign, section, panel, loading, errorView, notice, toast, confirm } from '../core/ui.js';
import { onLeave } from '../core/router.js';
import { isAndroid, native } from '../core/native.js';
import { isAdmin } from '../core/state.js';
import { bytes, fecha } from '../core/format.js';

function regionRow(r, local, rerender) {
  const loc = local.find((l) => l.id === r.id);
  const ready = r.estado === 'lista';
  const parts = [];
  let state = '';
  if (loc && loc.estado === 'descargando') state = `Descargando en el móvil… ${Math.round((loc.progreso || 0) * 100)}%`;
  else if (loc && loc.estado === 'descargado') state = `En el móvil · ${bytes(loc.tamano)} · versión ${loc.version}`;
  else if (r.estado === 'generando') state = `Preparando en el servidor… ${Math.round(r.progreso * 100)}%`;
  else if (r.estado === 'en_cola') state = 'En cola en el servidor…';
  else if (r.estado === 'error') state = `Error: ${r.error || ''}`;
  else if (ready) state = `Disponible · ${bytes(r.tamano)}`;
  else state = 'No preparado';

  const busyState = (loc && loc.estado === 'descargando') || r.estado === 'generando' || r.estado === 'en_cola';
  const progress = loc && loc.estado === 'descargando' ? loc.progreso : r.estado === 'generando' ? r.progreso : null;

  const act = async (fn) => {
    try {
      await fn();
    } catch (e) {
      toast(e.message, 'err');
    }
    rerender();
  };
  const prepare = () => act(async () => {
    await api(`/api/maps/regiones/${r.id}/preparar`, { method: 'POST' });
    toast('El servidor está preparando el mapa');
  });

  if (isAndroid) {
    const outdated = loc && loc.estado === 'descargado' && ready && loc.version !== r.version;
    if (!busyState) {
      if (ready && (!loc || loc.estado !== 'descargado' || outdated)) {
        parts.push(sign({ text: outdated ? 'Actualizar' : 'Descargar', ico: 'download', small: true, color: 'green', onClick: () => act(() => native.mapDownload({ ...r, server_url: r.url })) }));
      } else if (!ready) {
        parts.push(sign({ text: 'Preparar', ico: 'refresh', small: true, onClick: prepare }));
      }
      if (loc && loc.estado === 'descargado') {
        parts.push(sign({
          text: 'Borrar del móvil', ico: 'trash', small: true, color: 'red',
          onClick: async () => { if (await confirm(`¿Borrar el mapa de ${r.nombre} del móvil?`, { danger: true, ok: 'Borrar' })) act(() => native.mapDelete(r.id)); },
        }));
      }
    }
  } else if (!busyState) {
    if (!ready || isAdmin()) parts.push(sign({ text: ready ? 'Actualizar en el servidor' : 'Preparar en el servidor', ico: 'refresh', small: true, onClick: prepare }));
  }

  return h('div.panel',
    h('div', { style: { display: 'flex', justifyContent: 'space-between', gap: '10px', alignItems: 'baseline' } },
      h('strong', r.nombre),
      ready ? h('span.muted', `Datos del ${fecha(`${r.version.slice(0, 4)}-${r.version.slice(4, 6)}-${r.version.slice(6, 8)}`)}`) : null,
    ),
    h('div.muted', { style: { margin: '4px 0 10px' } }, state),
    progress !== null ? h('div.progress', h('i', { style: { width: `${Math.round(progress * 100)}%` } })) : null,
    parts.length ? h('div.grid2.mt', parts) : null,
  );
}

export async function view() {
  page({ title: 'Mapas', back: '/ajustes', body: loading() });
  let timer = null;
  onLeave(() => clearTimeout(timer));
  const render = async () => {
    let cat;
    try {
      cat = await api('/api/maps/catalogo');
    } catch (e) {
      const local = isAndroid ? native.mapsLocal() : [];
      return page({
        title: 'Mapas', back: '/ajustes',
        body: [...errorView(e, render), ...(local.length ? [section('En el móvil'), ...local.map((l) => panel(h('strong', l.nombre), h('div.muted', `${bytes(l.tamano)} · versión ${l.version}`)))] : [])],
      });
    }
    const local = isAndroid ? native.mapsLocal() : [];
    const body = [
      notice(isAndroid
        ? 'Descarga los mapas de las zonas por las que trabajas para poder elegir direcciones en el mapa sin cobertura.'
        : 'Los mapas para usar sin conexión se descargan desde la app Android. Aquí puedes pedir al servidor que los prepare.', 'info', 'map'),
    ];
    for (const g of cat.grupos) {
      body.push(section(g.nombre));
      for (const r of g.regiones) body.push(regionRow(r, local, render));
    }
    body.push(h('p.hint.center', `Mapas: ${cat.atribucion}`));
    const y = window.scrollY;
    page({ title: 'Mapas', back: '/ajustes', body });
    window.scrollTo(0, y);
    const active = cat.grupos.some((g) => g.regiones.some((r) => r.estado === 'generando' || r.estado === 'en_cola')) || local.some((l) => l.estado === 'descargando');
    clearTimeout(timer);
    if (active) timer = setTimeout(render, 2500);
  };
  await render();
}
