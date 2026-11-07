// "DeCA más reciente": los viajes del último DeCA emitido y cuál se está mostrando.
// Con varios viajes (ida y vuelta) se empieza por el viaje 1; las flechas cambian de
// viaje y la elección se recuerda (y se comparte con el widget de Android).

import { api, cache } from '../core/api.js';
import { isAndroid, native } from '../core/native.js';

const KEY = 'viajeActual';

/** Viaje que hay que mostrar del grupo (0 = viaje 1). */
export function currentIndex(viajes) {
  if (!viajes || !viajes.length) return 0;
  const grupo = viajes[0].grupo_id;
  const w = isAndroid ? native.widgetState() : null;
  const saved = w && w.grupo_id === grupo ? w : cache.get(KEY);
  const i = saved && saved.grupo_id === grupo ? saved.indice : 0;
  return Math.max(0, Math.min(viajes.length - 1, i || 0));
}

/** Guarda el viaje elegido y actualiza el widget. */
export function setIndex(viajes, indice) {
  if (!viajes || !viajes.length) return;
  cache.set(KEY, { grupo_id: viajes[0].grupo_id, indice });
  native.updateWidget(viajes, indice);
}

/** Vuelve a pedir el último DeCA al servidor y actualiza el widget (tras crear o modificar). */
export async function refreshLatest() {
  try {
    const viajes = await api('/api/decas/ultimo');
    cache.set('ultimo', viajes);
    native.updateWidget(viajes, currentIndex(viajes));
    return viajes;
  } catch {
    return null;
  }
}
