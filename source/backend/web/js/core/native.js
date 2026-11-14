// Puente con la app Android (objeto `window.DecaNative` inyectado por el WebView).
// En el navegador web no existe y estas funciones devuelven valores por defecto.

const N = window.DecaNative;
export const isAndroid = !!N;

let seq = 0;
const waiting = new Map();

// Android llama a esto para resolver las operaciones asíncronas (escáner QR, etc.).
window.__decaNativeResult = (id, ok, value) => {
  const p = waiting.get(id);
  if (!p) return;
  waiting.delete(id);
  ok ? p.resolve(value) : p.reject(new Error(value || 'Cancelado'));
};

function call(method, ...args) {
  if (!N || typeof N[method] !== 'function') return undefined;
  return N[method](...args);
}

function callAsync(method, ...args) {
  if (!N) return Promise.reject(new Error('Solo disponible en la app Android'));
  const id = `n${++seq}`;
  return new Promise((resolve, reject) => {
    waiting.set(id, { resolve, reject });
    call(method, id, ...args);
  });
}

const json = (s, d) => { try { return s ? JSON.parse(s) : d; } catch { return d; } };

export const native = {
  /** Guarda servidor + token para que la cola, las notificaciones y el widget funcionen sin la app abierta. */
  setSession: (session) => call('setSession', JSON.stringify(session || {})),
  /** Escanea un QR con la cámara. Devuelve el texto. */
  scanQr: () => callAsync('scanQr'),
  /** Mete una creación de DeCA en la cola sin conexión. */
  enqueueDeca: (payload) => json(call('enqueueDeca', JSON.stringify(payload)), null),
  pendingDecas: () => json(call('pendingDecas'), []),
  removePending: (id) => call('removePending', id),
  retryPending: () => call('retryPending'),
  /** Descarga un fichero con el gestor de descargas de Android (admite ficheros grandes). */
  download: (url, fileName, mime) => call('download', url, fileName, mime || ''),
  /** Busca una versión nueva de la app y, si la hay, la instala. Resuelve con "al_dia" o "instalando". */
  checkUpdate: () => callAsync('checkUpdate'),
  /** Abre un PDF de DeCA guardado en la app; si no está, lo descarga antes. `info`: { id, version, creado, archivo }. */
  openPdf: (info) => callAsync('openPdf', JSON.stringify(info)),
  /** ¿Está ya descargado este PDF en el móvil? */
  pdfSaved: (info) => call('pdfSaved', JSON.stringify(info)) === true,
  /** Enseña en el widget el último DeCA (o lo vacía con `null`). */
  updateWidget: (deca) => call('updateWidget', JSON.stringify(deca || null)),
  /** Brillo al máximo mientras se enseña el QR en un control. */
  setBrightness: (max) => call('setBrightness', !!max),
  /** Añade el widget del último DeCA a la pantalla de inicio. */
  pinWidget: () => call('pinWidget') === true,
  isOnline: () => (N ? call('isOnline') !== false : navigator.onLine),
  // ---- Mapas sin conexión ----
  mapsLocal: () => json(call('mapsLocal'), []),
  mapDownload: (region) => call('mapDownload', JSON.stringify(region)),
  mapDelete: (id) => call('mapDelete', id),
  /** Lee bytes de un mapa descargado (para PMTiles). Devuelve Base64. */
  mapRead: (id, offset, length) => call('mapRead', id, offset, length),
  appVersion: () => call('appVersion') || '',
};
