// Mapas por regiones guardados en el dispositivo.
// - App Android: ficheros nativos (MapStore).
// - Aplicación instalada (iPhone): almacenamiento del navegador (Cache API).

import { cache as store, serverBase, session } from './api.js';
import { isAndroid, native } from './native.js';
import { isInstalled } from './platform.js';

const CACHE = 'deca-mapas';
const KEY = 'mapas';
/** Por encima de este tamaño no se calcula el SHA-256: habría que cargar el mapa entero en memoria. */
const MAX_HASH_BYTES = 150 * 1024 * 1024;

const active = new Map();
const keyOf = (id) => new URL(`/__mapas/${id}`, location.origin).href;
const saved = () => store.get(KEY, []);

export const localMapsAvailable = () => isAndroid || (isInstalled() && 'caches' in window);

/** Regiones del dispositivo: {id, nombre, version, tamano, limites, estado, progreso}. */
export function list() {
  if (isAndroid) return native.mapsLocal();
  const rest = saved().filter((m) => !active.has(m.id));
  return [...rest, ...active.values()];
}

async function sha256(blob) {
  const digest = await crypto.subtle.digest('SHA-256', await blob.arrayBuffer());
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

async function fetchRegion(region, entry) {
  const res = await fetch(serverBase() + region.url, { headers: { Authorization: `Bearer ${session.get().token}` } });
  if (!res.ok || !res.body) throw new Error(`Error ${res.status} al descargar el mapa`);
  let done = 0;
  const counter = new TransformStream({
    transform(chunk, ctrl) {
      done += chunk.byteLength;
      entry.progreso = Math.min(1, done / Math.max(1, region.tamano));
      ctrl.enqueue(chunk);
    },
  });
  const maps = await caches.open(CACHE);
  await maps.put(keyOf(region.id), new Response(res.body.pipeThrough(counter), {
    headers: { 'Content-Type': 'application/vnd.pmtiles' },
  }));
  const blob = await (await maps.match(keyOf(region.id))).blob();
  const valid = blob.size === region.tamano && (blob.size > MAX_HASH_BYTES || (await sha256(blob)) === region.sha256);
  if (!valid) {
    await maps.delete(keyOf(region.id));
    throw new Error('El mapa descargado está incompleto');
  }
}

/** Empieza a descargar una región del catálogo. La aplicación debe seguir abierta. */
export function download(region) {
  if (isAndroid) return native.mapDownload(region);
  const entry = {
    id: region.id, nombre: region.nombre, version: region.version, tamano: region.tamano,
    limites: region.limites, estado: 'descargando', progreso: 0,
  };
  active.set(region.id, entry);
  fetchRegion(region, entry).then(() => {
    active.delete(region.id);
    store.set(KEY, [...saved().filter((m) => m.id !== region.id), { ...entry, estado: 'descargado', progreso: 1 }]);
  }).catch((e) => {
    entry.estado = 'error';
    entry.error = e.message;
  });
}

export async function remove(id) {
  if (isAndroid) return native.mapDelete(id);
  active.delete(id);
  await (await caches.open(CACHE)).delete(keyOf(id));
  store.set(KEY, saved().filter((m) => m.id !== id));
}

/** Fuente PMTiles de una región descargada. */
export async function openSource(id, pmtiles) {
  if (isAndroid) {
    return {
      getKey: () => `local-${id}`,
      async getBytes(offset, length) {
        const b64 = native.mapRead(id, offset, length);
        if (!b64) throw new Error('No se pudo leer el mapa');
        const bin = atob(b64);
        const buf = new Uint8Array(bin.length);
        for (let i = 0; i < bin.length; i++) buf[i] = bin.charCodeAt(i);
        return { data: buf.buffer };
      },
    };
  }
  const res = await (await caches.open(CACHE)).match(keyOf(id));
  if (!res) throw new Error('El mapa ya no está en el dispositivo');
  return new pmtiles.FileSource(new File([await res.blob()], `local-${id}`));
}
