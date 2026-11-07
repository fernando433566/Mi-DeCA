// Base de datos compartida (camiones, remolques, empresas, direcciones, mercancías),
// guardada también en el móvil para poder rellenar un DeCA sin conexión.

import { cachedGet, cache } from '../core/api.js';

const KINDS = {
  trucks: '/api/catalog/trucks',
  trailers: '/api/catalog/trailers',
  parties: '/api/catalog/parties',
  places: '/api/catalog/places',
  goods: '/api/catalog/goods',
};

const mem = {};

export async function load(kind, force = false) {
  if (mem[kind] && !force) return mem[kind];
  try {
    mem[kind] = await cachedGet(KINDS[kind], `cat.${kind}`);
  } catch {
    mem[kind] = cache.get(`cat.${kind}`, []);
  }
  return mem[kind] || [];
}

export async function loadAll() {
  await Promise.all(Object.keys(KINDS).map((k) => load(k, true)));
}

export function invalidate(kind) {
  delete mem[kind];
}
