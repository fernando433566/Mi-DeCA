// DeCA · service worker (generado por el servidor: ver src/routes/pwa.rs).
// Guarda la aplicación web en el dispositivo para que abra sin conexión.
// Nunca toca la API ni los enlaces públicos de los QR: esos siempre van a la red.

const VERSION = '__VERSION__';
const FILES = __FILES__;
const CACHE = `deca-shell-${VERSION}`;
const SOLO_RED = ['/api/', '/deca/', '/verificacion/', '/health', '/sw.js'];

self.addEventListener('install', (event) => {
  event.waitUntil((async () => {
    const cache = await caches.open(CACHE);
    await cache.addAll(FILES.map((url) => new Request(url, { cache: 'reload' })));
    await self.skipWaiting();
  })());
});

self.addEventListener('activate', (event) => {
  event.waitUntil((async () => {
    // Solo versiones antiguas de la aplicación: los mapas descargados no se tocan.
    for (const name of await caches.keys()) {
      if (name.startsWith('deca-shell-') && name !== CACHE) await caches.delete(name);
    }
    await self.clients.claim();
  })());
});

self.addEventListener('fetch', (event) => {
  const req = event.request;
  const url = new URL(req.url);
  if (req.method !== 'GET' || url.origin !== self.location.origin) return;
  if (SOLO_RED.some((p) => url.pathname.startsWith(p))) return;
  event.respondWith((async () => {
    const cache = await caches.open(CACHE);
    // La aplicación usa rutas con #, así que cualquier navegación es la página principal.
    const hit = await cache.match(req.mode === 'navigate' ? '/' : req, { ignoreSearch: true });
    return hit || fetch(req);
  })());
});
