// Campo de dirección completa: sugerencias de direcciones guardadas y de
// OpenStreetMap (Nominatim), y botón para elegir el punto en el mapa.

import { h, icon } from '../core/dom.js';
import { api } from '../core/api.js';
import { input as mkInput } from '../core/ui.js';
import { combo, matches } from './combo.js';
import * as catalog from './catalog.js';

/** `companyPlaces`: función opcional que devuelve las ubicaciones de las empresas del DeCA. */
export function addressInput({ value = '', placeholder = 'Calle, número, población', onChange, companyPlaces } = {}) {
  const inp = mkInput({ value, placeholder });
  inp.addEventListener('input', () => onChange && onChange(inp.value));
  const set = (v) => {
    inp.value = v;
    onChange && onChange(v);
  };
  const c = combo({
    input: inp,
    minChars: 0,
    delay: 400,
    suggest: async (q) => {
      const own = companyPlaces ? await companyPlaces() : [];
      const ownItems = own
        .filter((u) => !q || matches(q, u.nombre, u.direccion))
        .map((u) => ({ label: u.nombre, sub: u.nombre !== u.direccion ? u.direccion : '', src: 'EMPRESA', value: u.direccion }));
      const places = await catalog.load('places');
      const saved = places
        .filter((p) => !q || matches(q, p.nombre, p.direccion))
        .slice(0, 6)
        .map((p) => ({ label: p.nombre, sub: p.nombre !== p.direccion ? p.direccion : '', src: 'GUARDADA', value: p.direccion }));
      let osm = [];
      if (q.length >= 4 && navigator.onLine) {
        try {
          const r = await api(`/api/maps/search?q=${encodeURIComponent(q)}`, { timeout: 8000 });
          osm = r.map((x) => ({ label: x.direccion, src: 'MAPA', value: x.direccion }));
        } catch { /* sin conexión: solo las guardadas */ }
      }
      const seen = new Set(ownItems.map((i) => i.value));
      return [...ownItems, ...saved.filter((i) => !seen.has(i.value)), ...osm];
    },
    onPick: (it) => set(it.value),
  });
  const mapBtn = h('button.sign.small.white', {
    type: 'button',
    style: { width: 'auto', marginTop: '8px' },
    onclick: async () => {
      const { pickOnMap } = await import('./map-picker.js');
      const addr = await pickOnMap({ initial: inp.value });
      if (addr) set(addr);
    },
  }, icon('map'), h('span.txt', 'Elegir en el mapa'));
  const el = h('div', c, mapBtn);
  el.getValue = () => inp.value.trim();
  el.setValue = set;
  return el;
}
