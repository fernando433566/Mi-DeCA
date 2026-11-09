// Estado compartido: usuario actual y estado de la conexión.

import { session } from './api.js';
import { isAndroid } from './native.js';
import { isInstalled } from './platform.js';

export const state = { user: null, netOk: true };

export function setUser(u) {
  state.user = u;
  const s = session.get();
  if (s) session.set({ ...s, user: u });
}

export const isAdmin = () => !!(state.user && state.user.rol === 'admin');

/** Muestra u oculta la franja amarilla de "sin conexión". */
export function setNet(ok) {
  state.netOk = ok;
  const bar = document.getElementById('netbar');
  if (!bar) return;
  bar.hidden = ok;
  if (!ok && !bar.firstChild) {
    const s = document.createElement('span');
    s.textContent = isAndroid || isInstalled() ? 'Sin conexión · Se muestran los datos guardados' : 'Sin conexión con el servidor';
    bar.appendChild(s);
  }
}
