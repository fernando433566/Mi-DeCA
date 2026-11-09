// Elegir un punto en el mapa (OpenStreetMap) y obtener su dirección.
//
// - Con conexión: teselas de OpenStreetMap a través del servidor (en caché) y
//   dirección exacta con Nominatim.
// - Sin conexión (app Android con mapas descargados): mapa vectorial PMTiles leído
//   del propio móvil; la dirección se compone con la calle y la población del mapa.

import { h, icon } from '../core/dom.js';
import { api, serverBase, session } from '../core/api.js';
import { native } from '../core/native.js';
import { sign, toast, select } from '../core/ui.js';
import * as localMaps from '../core/localmaps.js';

const VENDOR = new URL('../../vendor/', import.meta.url).href;
let libs = null;

async function loadLibs() {
  if (libs) return libs;
  if (!document.querySelector('link[data-maplibre]')) {
    document.head.appendChild(h('link', { rel: 'stylesheet', href: VENDOR + 'maplibre/maplibre-gl.css', 'data-maplibre': '1' }));
  }
  const loadScript = (src) => new Promise((ok, ko) => {
    if (document.querySelector(`script[src="${src}"]`)) return ok();
    const s = h('script', { src });
    s.onload = ok;
    s.onerror = ko;
    document.head.appendChild(s);
  });
  const [maplibregl] = await Promise.all([
    import(VENDOR + 'maplibre/maplibre-gl.mjs'),
    loadScript(VENDOR + 'pmtiles/pmtiles.js'),
    loadScript(VENDOR + 'basemaps/basemaps.js'),
  ]);
  libs = { maplibregl, pmtiles: window.pmtiles, basemaps: window.basemaps };
  const protocol = new libs.pmtiles.Protocol();
  maplibregl.addProtocol('pmtiles', protocol.tile);
  libs.protocol = protocol;
  return libs;
}

function onlineStyle() {
  return {
    version: 8,
    sources: {
      osm: {
        type: 'raster',
        tiles: [serverBase() + '/api/maps/tiles/{z}/{x}/{y}.png'],
        tileSize: 256,
        maxzoom: 19,
        attribution: '© OpenStreetMap contributors',
      },
    },
    layers: [{ id: 'osm', type: 'raster', source: 'osm' }],
  };
}

async function offlineStyle(L, region) {
  const src = await localMaps.openSource(region.id, L.pmtiles);
  L.protocol.add(new L.pmtiles.PMTiles(src));
  return {
    version: 8,
    glyphs: VENDOR + 'map-assets/fonts/{fontstack}/{range}.pbf',
    sprite: VENDOR + 'map-assets/sprites/light',
    sources: {
      protomaps: { type: 'vector', url: `pmtiles://${src.getKey()}`, attribution: '© OpenStreetMap contributors' },
    },
    layers: L.basemaps.layers('protomaps', L.basemaps.namedFlavor('light'), { lang: 'es' }),
  };
}

/** Dirección aproximada sin conexión: calle + población leídas del mapa. */
function offlineAddress(map) {
  const c = map.project(map.getCenter());
  const box = [[c.x - 40, c.y - 40], [c.x + 40, c.y + 40]];
  const feats = map.queryRenderedFeatures(box);
  const road = feats.find((f) => f.sourceLayer === 'roads' && f.properties.name);
  const place = map.queryRenderedFeatures().find((f) => f.sourceLayer === 'places' && f.properties.name);
  const ll = map.getCenter();
  const parts = [road && road.properties.name, place && place.properties.name].filter(Boolean);
  return `${parts.join(', ') || 'Punto del mapa'} (${ll.lat.toFixed(5)}, ${ll.lng.toFixed(5)})`;
}

/**
 * Abre el mapa a pantalla completa. Devuelve la dirección elegida o null.
 */
export function pickOnMap({ initial = '' } = {}) {
  return new Promise(async (resolve) => {
    let L;
    try {
      L = await loadLibs();
    } catch (e) {
      toast('No se pudo cargar el mapa', 'err');
      return resolve(null);
    }
    // "Con conexión" = el servidor responde (no basta con tener wifi).
    let online = native.isOnline() !== false && navigator.onLine;
    if (online) {
      try { await api('/api/info', { timeout: 4000 }); } catch { online = false; }
    }
    const regions = localMaps.localMapsAvailable() ? localMaps.list().filter((r) => r.estado === 'descargado') : [];
    const useOffline = !online && regions.length > 0;
    if (!online && !useOffline) {
      toast(localMaps.localMapsAvailable() ? 'Sin conexión y sin mapas descargados (Ajustes → Mapas)' : 'Sin conexión', 'err');
      return resolve(null);
    }

    const addr = h('div.addr', 'Mueve el mapa hasta el punto…');
    const mapEl = h('div.map');
    const pin = h('div.map-pin', { style: { color: 'var(--rojo)' } }, icon('pin', 'ico'));
    let regionSel = null;
    let current = useOffline ? regions[0] : null;
    if (useOffline && regions.length > 1) {
      regionSel = select(regions.map((r) => [r.id, r.nombre]), current.id);
      regionSel.style.minHeight = '44px';
      regionSel.addEventListener('change', async () => {
        current = regions.find((r) => r.id === regionSel.value);
        map.setStyle(await offlineStyle(L, current));
        const b = current.limites;
        map.fitBounds([[b.min_lon, b.min_lat], [b.max_lon, b.max_lat]]);
      });
    }

    let picked = null;
    const ok = sign({ text: 'Usar esta dirección', ico: 'check', color: 'green', center: true, onClick: () => done(picked) });
    ok.disabled = true;
    const gps = h('button.top-btn', { 'aria-label': 'Mi ubicación', onclick: locate }, icon('pin'));
    const closeBtn = h('button.top-btn', { 'aria-label': 'Cerrar', onclick: () => done(null) }, icon('back'));
    const wrap = h('div.map-wrap',
      h('div.map-top', closeBtn, h('div.t', useOffline ? 'Mapa sin conexión' : 'Elige el punto en el mapa'), regionSel, gps),
      h('div', { style: { position: 'relative', flex: '1', display: 'flex' } }, mapEl, pin),
      h('div.map-bottom', addr, ok),
    );
    mapEl.style.flex = '1';
    document.body.appendChild(wrap);

    const token = session.get()?.token;
    const b = current && current.limites;
    const map = new L.maplibregl.Map({
      container: mapEl,
      style: useOffline ? await offlineStyle(L, current) : onlineStyle(),
      center: b ? [(b.min_lon + b.max_lon) / 2, (b.min_lat + b.max_lat) / 2] : [-3.70, 40.42],
      zoom: b ? 9 : 5.2,
      attributionControl: { compact: true },
      transformRequest: (url, type) => {
        // Las carpetas de fuentes no llevan espacios (el WebView de Android no decodifica %20).
        if (type === 'Glyphs') return { url: url.replace(/%20| /g, '_') };
        if (url.startsWith(serverBase() + '/api/') && token) return { url, headers: { Authorization: `Bearer ${token}` } };
        return { url };
      },
    });

    let seq = 0;
    const update = async () => {
      const my = ++seq;
      ok.disabled = true;
      const ll = map.getCenter();
      if (useOffline) {
        picked = offlineAddress(map);
      } else {
        addr.textContent = 'Buscando dirección…';
        try {
          const r = await api(`/api/maps/reverse?lat=${ll.lat}&lon=${ll.lng}`, { timeout: 8000 });
          picked = r.direccion || offlineAddress(map);
        } catch {
          picked = `Punto del mapa (${ll.lat.toFixed(5)}, ${ll.lng.toFixed(5)})`;
        }
      }
      if (my !== seq) return;
      addr.textContent = picked;
      ok.disabled = false;
    };
    let t = null;
    map.on('moveend', () => { clearTimeout(t); t = setTimeout(update, 500); });
    map.once('idle', update);

    // Si ya había una dirección escrita, centramos el mapa en ella.
    if (initial && online) {
      api(`/api/maps/search?q=${encodeURIComponent(initial)}`, { timeout: 8000 })
        .then((r) => r[0] && map.jumpTo({ center: [r[0].lon, r[0].lat], zoom: 16 }))
        .catch(() => {});
    }

    function locate() {
      if (!navigator.geolocation) return toast('Ubicación no disponible', 'err');
      navigator.geolocation.getCurrentPosition(
        (p) => map.flyTo({ center: [p.coords.longitude, p.coords.latitude], zoom: 16 }),
        () => toast('No se pudo obtener tu ubicación', 'err'),
        { enableHighAccuracy: true, timeout: 15000 },
      );
    }

    function done(v) {
      map.remove();
      wrap.remove();
      resolve(v);
    }
  });
}
