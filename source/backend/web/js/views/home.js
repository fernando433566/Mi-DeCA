// Pantalla de inicio: "Crear DeCA", "Últimos 7 Días", "DeCA más reciente" (billete)
// y, en Android, los DeCA pendientes de crear por falta de conexión.

import { h } from '../core/dom.js';
import { cachedGet, api } from '../core/api.js';
import { page, sign, section, notice, topButton, loading, toast, confirm, panel } from '../core/ui.js';
import { go } from '../core/router.js';
import { isAndroid, native } from '../core/native.js';
import { state, isAdmin, setNet } from '../core/state.js';
import { pendingTicket, tripCarousel } from '../components/ticket.js';
import { currentIndex, setIndex } from '../components/latest.js';
import { fechaHora } from '../core/format.js';
import * as catalog from '../components/catalog.js';
import { pending, removePending, retry as retryQueue, queueAvailable, sendsInBackground } from '../core/queue.js';

function pendingSection() {
  if (!queueAvailable()) return [];
  const list = pending();
  const out = [];
  for (const p of list) {
    if (p.estado === 'creado') {
      // Ya creado por la cola: se avisa y se quita de pendientes.
      removePending(p.id);
      const n = (p.resultado && p.resultado.decas && p.resultado.decas[0]) || null;
      toast(n ? `DeCA ${n.numero} creado al volver la conexión` : 'DeCA pendiente creado', 'ok');
      continue;
    }
    out.push(pendingTicket(p, { onClick: () => go(`/pendiente/${p.id}`) }));
  }
  if (!out.length) return [];
  return [
    section('Pendientes de crear'),
    notice(sendsInBackground()
      ? 'Estos DeCA se crearán solos en cuanto haya conexión y te avisaremos. Hasta que tengan QR NO son válidos: no inicies el transporte.'
      : 'Estos DeCA se crearán cuando haya conexión y la aplicación esté abierta: déjala abierta al recuperar la señal. Hasta que tengan QR NO son válidos: no inicies el transporte.', 'danger', 'wifioff'),
    ...out,
  ];
}

export async function view() {
  const create = sign({ text: 'CREAR DeCA', sub: 'Nuevo documento de control', ico: 'deca', color: 'green', big: true, chev: true, onClick: () => go('/nuevo') });
  const last7 = sign({ text: 'Últimos 7 Días', sub: 'DeCA emitidos esta semana', ico: 'calendar', chev: true, onClick: () => go('/recientes') });
  const latestBox = h('div', loading());

  page({
    title: state.user ? `Hola, ${state.user.nombre}` : 'DeCA',
    top: [topButton('gear', 'Ajustes', () => go('/ajustes'))],
    body: [
      create,
      last7,
      ...pendingSection(),
      section('DeCA más reciente'),
      latestBox,
    ],
  });

  // Preparar datos para poder crear un DeCA sin conexión.
  catalog.loadAll().catch(() => {});
  cachedGet('/api/decas/plantilla', 'plantilla').catch(() => {});

  if (isAdmin()) {
    api('/api/setup/status').then((s) => {
      if (s.necesita_empresa) {
        latestBox.before(notice('Falta crear tu empresa. Pulsa aquí para hacerlo.', 'info', 'building'));
        latestBox.previousSibling.addEventListener('click', () => go('/empresa/nueva'));
      }
    }).catch(() => {});
  }

  try {
    // Todos los viajes del último DeCA, del 1 al último.
    const viajes = await cachedGet('/api/decas/ultimo', 'ultimo');
    setNet(!viajes.__offline);
    const index = currentIndex(viajes);
    latestBox.replaceChildren(
      viajes.length
        ? tripCarousel(viajes, index, {
          showUser: isAdmin(),
          onOpen: (d) => go(`/deca/${d.id}`),
          onChange: (i) => setIndex(viajes, i),
        })
        : panel(h('p.muted.center', 'Todavía no hay ningún DeCA. Pulsa «CREAR DeCA» para hacer el primero.')),
    );
    if (!viajes.__offline) native.updateWidget(viajes, index);
  } catch (e) {
    if (e.offline) setNet(false);
    latestBox.replaceChildren(notice(e.offline ? 'Sin conexión y sin datos guardados todavía.' : e.message, 'danger'));
  }
}

/** Detalle de un DeCA en la cola sin conexión (Android). */
export async function pendingView({ id }) {
  const p = pending().find((x) => x.id === id);
  if (!p) return go('/', { replace: true });
  const d = p.payload || {};
  const env = d.envios || [];
  const retry = sign({ text: 'Intentar enviar ahora', ico: 'send', color: 'green', center: true, onClick: () => { retryQueue(); toast('Reintentando…'); } });
  const discard = sign({
    text: 'Descartar', ico: 'trash', color: 'red', small: true, center: true,
    onClick: async () => {
      if (await confirm('¿Descartar este DeCA pendiente? No se creará.', { danger: true, ok: 'Descartar' })) {
        removePending(id);
        go('/', { replace: true });
      }
    },
  });
  page({
    title: 'DeCA pendiente',
    back: '/',
    body: [
      p.estado === 'error'
        ? notice(`El servidor no aceptó este DeCA: ${p.error || 'error desconocido'}. Descártalo y créalo de nuevo corrigiendo los datos.`, 'danger')
        : notice('Todavía no tiene QR. Se creará en cuanto haya conexión. No inicies el transporte sin el QR.', '', 'wifioff'),
      panel(
        h('dl.kv',
          h('dt', 'Guardado'), h('dd', fechaHora(p.creado)),
          h('dt', 'Intentos'), h('dd', String(p.intentos || 0)),
          h('dt', 'Camión'), h('dd', d.matricula_camion || ''),
          h('dt', 'Cargador'), h('dd', d.cargador?.nombre || ''),
          h('dt', 'Transportista'), h('dd', d.transportista?.nombre || ''),
          ...env.flatMap((e, i) => [h('dt', `Envío ${i + 1}`), h('dd', `${e.origen} → ${e.destino}`)]),
          h('dt', 'Viajes'), h('dd', String(d.numero_viajes || 1)),
        ),
      ),
      discard,
    ],
    actions: [p.estado !== 'error' ? retry : null],
  });
}
