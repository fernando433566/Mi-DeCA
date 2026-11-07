// Ajustes -> Servidor (administrador).

import { h } from '../core/dom.js';
import { api, downloadFile, session } from '../core/api.js';
import { page, sign, section, panel, field, input, select, toggle, busy, toast, notice, loading, errorView, confirm, pickFile } from '../core/ui.js';
import { go } from '../core/router.js';
import { fechaHora } from '../core/format.js';

export async function view() {
  page({ title: 'Servidor', back: '/ajustes', body: loading() });
  let r;
  let status;
  try {
    [r, status] = await Promise.all([api('/api/server/settings'), api('/api/server/status')]);
  } catch (e) {
    return page({ title: 'Servidor', back: '/ajustes', body: errorView(e, view) });
  }
  const s = r.ajustes;
  const dp = r.direccion_publica || {};
  const put = (patch) => api('/api/server/settings', { method: 'PUT', body: patch });

  // Dirección pública
  const urlInput = input({ value: s.public_base_url || '', type: 'url', placeholder: 'Vacío = se detecta sola por Cloudflare Tunnel' });
  const saveUrl = sign({ text: 'Guardar dirección', ico: 'check', small: true });
  saveUrl.onclick = () => busy(saveUrl, async () => {
    await put({ public_base_url: urlInput.value.trim() });
    toast('Dirección guardada', 'ok');
    view();
  });
  const check = sign({ text: 'Comprobar enlace ahora', ico: 'refresh', small: true, color: 'white' });
  check.onclick = () => busy(check, async () => {
    const c = await api('/api/server/public-url/check', { method: 'POST' });
    toast(c.comprobacion.ok ? 'El enlace funciona por HTTPS' : `No funciona: ${c.comprobacion.detalle}`, c.comprobacion.ok ? 'ok' : 'err');
    view();
  });

  // Mapas
  const zoom = select([['12', 'Bajo (12)'], ['13', 'Medio (13)'], ['14', 'Alto (14)'], ['15', 'Máximo (15, calles y números)']], String(s.maps_max_zoom));
  zoom.onchange = () => put({ maps_max_zoom: Number(zoom.value) }).then(() => toast('Guardado', 'ok')).catch((e) => toast(e.message, 'err'));
  const days = select([['0', 'Nunca'], ['7', 'Cada semana'], ['30', 'Cada mes'], ['90', 'Cada 3 meses']], String(s.maps_auto_update_days));
  days.onchange = () => put({ maps_auto_update_days: Number(days.value) }).then(() => toast('Guardado', 'ok')).catch((e) => toast(e.message, 'err'));

  const restore = sign({ text: 'Restaurar de una copia de seguridad', sub: 'Fichero .json o .zip', ico: 'upload', color: 'yellow' });
  restore.onclick = () => busy(restore, async () => {
    const file = await pickFile('.json,.zip,application/json,application/zip');
    if (!file) return;
    if (!(await confirm(`¿Restaurar «${file.name}»? Se sustituirán TODOS los datos actuales y habrá que volver a entrar.`, { danger: true, ok: 'Restaurar' }))) return;
    const fd = new FormData();
    fd.append('archivo', file);
    toast('Subiendo copia… no cierres la app');
    const res = await api('/api/server/restore', { method: 'POST', body: fd, timeout: 3600000 });
    toast(res.mensaje, 'ok');
    session.clear();
    go('/login', { replace: true });
  });

  const cleanup = sign({ text: 'Ejecutar limpieza ahora', ico: 'trash', small: true, color: 'white' });
  cleanup.onclick = () => busy(cleanup, async () => {
    if (!(await confirm('¿Borrar ahora los PDF con más de 366 días?'))) return;
    const res = await api('/api/server/cleanup/run', { method: 'POST' });
    toast(`${res.eliminados} PDF eliminados`, 'ok');
  });

  page({
    title: 'Servidor',
    back: '/ajustes',
    body: [
      panel(h('dl.kv',
        h('dt', 'DeCA emitidos'), h('dd', String(status.decas_total)),
        h('dt', 'Con enlace activo'), h('dd', String(status.decas_con_enlace_activo)),
        h('dt', 'Versión'), h('dd', status.version),
      )),
      section('Eliminación automática'),
      toggle('Permitir eliminación automática', s.auto_delete_enabled, (v) => put({ auto_delete_enabled: v }).then(() => toast(v ? 'Activada' : 'Desactivada', 'ok')).catch((e) => toast(e.message, 'err')),
        'Cada día a las 12:00 se borran los PDF con más de 366 días (la ley obliga a guardarlos 1 año).'),
      cleanup,
      section('Copias de seguridad'),
      sign({ text: 'Crear copia de seguridad', ico: 'download', color: 'green', chev: true, onClick: () => go('/servidor/copia') }),
      restore,
      section('Dirección pública (QR)'),
      dp.url
        ? notice(dp.verificada ? `Los QR usan ${dp.url}. Comprobado por HTTPS.` : `Los QR usan ${dp.url}. ${dp.aviso || 'Sin comprobar.'}`, dp.verificada ? 'ok' : '', dp.verificada ? 'check' : 'warning')
        : notice(dp.aviso || 'Dirección pública desconocida.', 'danger', 'server'),
      dp.ultima_comprobacion ? h('p.hint', `Última comprobación: ${fechaHora(dp.ultima_comprobacion.fecha)}`) : null,
      panel(field('Dirección manual (opcional)', urlInput, { hint: 'Déjala vacía para usar la detectada automáticamente.' }), h('div.grid2', saveUrl, check)),
      section('Mapas sin conexión'),
      panel(field('Nivel de detalle de los mapas', zoom), field('Actualizar mapas', days)),
    ],
  });
}

export function backupView() {
  const cfg = sign({ text: 'Copia de seguridad de configuración', sub: 'Fichero .json: empresas, usuarios, camiones, preferencias… (sin PDF)', ico: 'download' });
  cfg.onclick = () => busy(cfg, () => downloadFile('/api/server/backup/config', `DeCA-configuracion-${new Date().toISOString().slice(0, 10)}.json`, 'application/json'));
  const full = sign({ text: 'Copia de seguridad completa', sub: 'Fichero .zip: configuración + todos los PDF', ico: 'download', color: 'green' });
  full.onclick = () => busy(full, async () => {
    toast('Preparando la copia completa… puede tardar');
    await downloadFile('/api/server/backup/full', `DeCA-copia-completa-${new Date().toISOString().slice(0, 10)}.zip`, 'application/zip');
  });
  page({
    title: 'Crear copia de seguridad',
    back: '/servidor',
    body: [
      notice('Guarda la copia fuera de este servidor (otro disco, la nube…). Con ella puedes llevar todo a otra máquina desde «Restaurar de una copia de seguridad».', 'info', 'server'),
      cfg,
      full,
    ],
  });
}
