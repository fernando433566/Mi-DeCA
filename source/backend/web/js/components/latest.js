// "DeCA más reciente": el último DeCA emitido, compartido con el widget de Android.

import { api, cache } from '../core/api.js';
import { native } from '../core/native.js';

/** Enseña este DeCA (o ninguno) en el widget. */
export function showOnWidget(deca) {
  native.updateWidget(deca || null);
}

/** Vuelve a pedir el último DeCA al servidor y actualiza el widget (tras crear o modificar). */
export async function refreshLatest() {
  try {
    // El servidor devuelve una lista con 0 o 1 DeCA.
    const list = await api('/api/decas/ultimo');
    cache.set('ultimo', list);
    showOnWidget(list[0]);
    return list[0] || null;
  } catch {
    return null;
  }
}
