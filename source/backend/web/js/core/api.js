// Cliente de la API del servidor DeCA: sesión, peticiones y caché para usar sin conexión.

import { isAndroid, native } from './native.js';

const SESSION_KEY = 'deca.session';

export class ApiError extends Error {
  constructor(message, status, code) {
    super(message);
    this.status = status;
    this.code = code;
  }
}
/** No se pudo contactar con el servidor (sin cobertura, servidor apagado...). */
export class OfflineError extends Error {
  constructor() {
    super('Sin conexión con el servidor');
    this.offline = true;
  }
}

function load(key, def) {
  try {
    const v = localStorage.getItem(key);
    return v ? JSON.parse(v) : def;
  } catch {
    return def;
  }
}
function save(key, value) {
  try {
    if (value === undefined || value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, JSON.stringify(value));
  } catch { /* almacenamiento lleno o bloqueado */ }
}

/** Sesión: { server, token, user } */
export const session = {
  get: () => load(SESSION_KEY, null),
  set(s) {
    save(SESSION_KEY, s);
    native.setSession(s ? { server: s.server, token: s.token, user: s.user } : null);
  },
  clear() {
    this.set(null);
  },
};

/** Dirección del servidor: en la web es la propia página; en Android, la elegida al iniciar sesión. */
export function serverBase() {
  const s = session.get();
  if (s && s.server) return s.server;
  return isAndroid ? '' : location.origin;
}

export function normalizeServer(url) {
  let u = (url || '').trim();
  if (!u) return '';
  if (!/^https?:\/\//i.test(u)) u = 'https://' + u;
  return u.replace(/\/+$/, '');
}

let onUnauthorized = () => {};
export function setUnauthorizedHandler(fn) {
  onUnauthorized = fn;
}

/**
 * Petición a la API. `body` se envía como JSON (o FormData tal cual).
 * Lanza ApiError (respuesta de error) u OfflineError (sin conexión).
 */
export async function api(path, { method = 'GET', body, server, token, raw = false, timeout = 25000 } = {}) {
  const s = session.get();
  const base = server || serverBase();
  const tk = token !== undefined ? token : s && s.token;
  const headers = {};
  if (tk) headers.Authorization = `Bearer ${tk}`;
  let payload = body;
  if (body !== undefined && !(body instanceof FormData)) {
    headers['Content-Type'] = 'application/json';
    payload = JSON.stringify(body);
  }
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), timeout);
  let res;
  try {
    res = await fetch(base + path, { method, headers, body: payload, signal: ctrl.signal });
  } catch {
    window.dispatchEvent(new Event('deca:offline'));
    throw new OfflineError();
  } finally {
    clearTimeout(timer);
  }
  window.dispatchEvent(new Event('deca:online'));
  if (raw && res.ok) return res;
  let data = null;
  const text = await res.text();
  try {
    data = text ? JSON.parse(text) : null;
  } catch {
    data = null;
  }
  if (!res.ok) {
    const msg = (data && data.error) || `Error ${res.status}`;
    const err = new ApiError(msg, res.status, data && data.codigo);
    if (res.status === 401 && err.code === 'sesion') onUnauthorized();
    throw err;
  }
  return data;
}

// ---------------- Caché local (para enseñar los DeCA sin cobertura) ----------------

function cacheKey(name) {
  const s = session.get();
  return `deca.cache.${s ? s.server + '|' + (s.user && s.user.id) : 'anon'}.${name}`;
}
export const cache = {
  get: (name, def = null) => load(cacheKey(name), def),
  set: (name, value) => save(cacheKey(name), value),
};

/**
 * GET con respaldo en caché: si no hay conexión devuelve lo último guardado
 * marcado con `__offline = true`.
 */
export async function cachedGet(path, name) {
  try {
    const data = await api(path);
    cache.set(name, data);
    return data;
  } catch (e) {
    if (e.offline) {
      const c = cache.get(name);
      if (c !== null) {
        if (typeof c === 'object') Object.defineProperty(c, '__offline', { value: true });
        return c;
      }
    }
    throw e;
  }
}

/** Descarga un fichero protegido (PDF, copias de seguridad). */
export async function downloadFile(path, fileName, mime) {
  const s = session.get();
  if (isAndroid) {
    // El gestor de descargas de Android añade el token a la petición.
    native.download(serverBase() + path, fileName, mime);
    return;
  }
  const res = await api(path, { raw: true, timeout: 600000 });
  const blob = await res.blob();
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = fileName;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60000);
  return s;
}
