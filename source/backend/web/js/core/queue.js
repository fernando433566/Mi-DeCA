// DeCA creados sin conexión.
// - App Android: la cola es nativa y se envía aunque la app esté cerrada.
// - Aplicación instalada (iPhone): se guarda en el dispositivo y se envía mientras la
//   aplicación está abierta, porque iOS no permite trabajar en segundo plano a una web.
// Cada petición lleva `client_ref`: si se repite, el servidor no duplica el DeCA.

import { api, cache, session, sessionIdentity } from './api.js';
import { isAndroid, native } from './native.js';
import { isInstalled } from './platform.js';
import { uuid } from './format.js';
import { toast } from './ui.js';

const KEY = 'cola';
const load = () => cache.get(KEY, []);
const save = (items) => cache.set(KEY, items);

export const queueAvailable = () => isAndroid || isInstalled();

/** Android envía la cola por su cuenta; la aplicación instalada solo con ella abierta. */
export const sendsInBackground = () => isAndroid;

export function pending() {
  return isAndroid ? native.pendingDecas() : load();
}

export function enqueue(payload) {
  if (isAndroid) return native.enqueueDeca(payload);
  const item = { id: uuid(), creado: new Date().toISOString(), estado: 'pendiente', intentos: 0, payload };
  save([...load(), item]);
  return item;
}

export function removePending(id) {
  if (isAndroid) return native.removePending(id);
  save(load().filter((p) => p.id !== id));
}

const changed = () => window.__decaQueueChanged && window.__decaQueueChanged();

let running = false;

/** Intenta enviar los DeCA pendientes. Se detiene al primer fallo de conexión. */
export async function flush() {
  if (isAndroid || running || !session.get()) return;
  const owner = sessionIdentity();
  const sameOwner = () => sessionIdentity() === owner;
  const todo = load().filter((p) => p.estado === 'pendiente');
  if (!todo.length) return;
  running = true;
  let touched = false;
  try {
    for (const item of todo) {
      if (!sameOwner()) break;
      const update = (patch) => save(load().map((p) => (p.id === item.id ? { ...p, ...patch } : p)));
      try {
        update({ intentos: item.intentos + 1 });
        const res = await api('/api/decas', { method: 'POST', body: item.payload, timeout: 30000 });
        removePending(item.id);
        touched = true;
        const first = res.decas[0];
        toast(res.decas.length > 1 ? `${res.decas.length} DeCA creados al volver la conexión` : `DeCA ${first.numero} creado al volver la conexión`, 'ok');
      } catch (e) {
        if (!sameOwner() || e.code === 'session_changed') break;
        // Sin conexión, servidor caído o sesión caducada: se reintenta más tarde.
        if (e.offline || e.status >= 500 || [401, 408, 429].includes(e.status)) break;
        update({ estado: 'error', error: e.message });
        touched = true;
      }
    }
  } finally {
    running = false;
  }
  if (touched) changed();
}

export function retry() {
  return isAndroid ? native.retryPending() : flush();
}

/** Envía la cola al arrancar, al volver a la aplicación o al recuperar la conexión. */
export function startQueue() {
  if (isAndroid) return;
  flush();
  window.addEventListener('online', flush);
  window.addEventListener('deca:online', flush);
  document.addEventListener('visibilitychange', () => { if (!document.hidden) flush(); });
  setInterval(flush, 30000);
}
