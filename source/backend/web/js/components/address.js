// Campo de dirección completa: sugerencias de direcciones guardadas y de
// OpenStreetMap (Nominatim), y botón para elegir el punto en el mapa.

import { h, icon } from '../core/dom.js';
import { api } from '../core/api.js';
import { input as mkInput } from '../core/ui.js';
import { combo, matches } from './combo.js';
import * as catalog from './catalog.js';

export function addressInput({ value = '', placeholder = 'Calle, número, población', onChange } = {}) {
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
      return [...saved, ...osm];
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
