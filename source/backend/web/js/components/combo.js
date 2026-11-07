// Campo de texto con sugerencias (base de datos compartida, OpenStreetMap...).
// El usuario puede elegir una sugerencia o escribir libremente.

import { h, clear } from '../core/dom.js';

/**
 * @param {object} o
 * @param {HTMLInputElement} o.input  campo de texto ya creado
 * @param {(q: string) => Promise<Array<{label, sub?, src?, value}>>} o.suggest
 * @param {(item) => void} o.onPick
 * @param {number} [o.minChars=0]
 * @param {number} [o.delay=150]
 */
export function combo({ input, suggest, onPick, minChars = 0, delay = 150 }) {
  const box = h('div.sugg', { hidden: true });
  const wrap = h('div.combo', input, box);
  let timer = null;
  let seq = 0;

  const close = () => { box.hidden = true; };
  const refresh = () => {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      const q = input.value.trim();
      if (q.length < minChars) return close();
      const my = ++seq;
      let items = [];
      try {
        items = await suggest(q);
      } catch { items = []; }
      if (my !== seq) return;
      clear(box);
      if (!items.length) return close();
      for (const it of items.slice(0, 12)) {
        box.appendChild(h('button', {
          type: 'button',
          onmousedown: (e) => e.preventDefault(), // evita perder el foco antes del clic
          onclick: () => { close(); onPick(it); },
        }, it.src ? h('span.src', it.src) : null, h('div', it.label), it.sub ? h('div.s', it.sub) : null));
      }
      box.hidden = false;
    }, delay);
  };
  input.addEventListener('input', refresh);
  input.addEventListener('focus', refresh);
  input.addEventListener('blur', () => setTimeout(close, 150));
  return wrap;
}

/** Filtro local sin tildes ni mayúsculas. */
export function matches(q, ...texts) {
  const n = (s) => String(s || '').normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();
  const nq = n(q);
  return texts.some((t) => n(t).includes(nq));
}
