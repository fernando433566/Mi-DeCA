// Componentes de interfaz con estética de señales de tráfico.

import { h, icon, clear, append } from './dom.js';
import { back as navBack, go } from './router.js';

const $ = (id) => document.getElementById(id);

/**
 * Pinta una pantalla.
 * - title: texto de la cabecera
 * - back: ruta a la que vuelve la flecha (o true = atrás)
 * - body: contenido
 * - actions: botones de la barra inferior fija (siempre visibles)
 * - top: botones extra en la cabecera
 */
export function page({ title, back, body = [], actions = [], top = [] }) {
  $('top-title').textContent = title || 'DeCA';
  document.title = title ? `${title} · DeCA` : 'DeCA';
  const backBtn = $('top-back');
  clear(backBtn);
  if (back) {
    backBtn.hidden = false;
    backBtn.appendChild(icon('back'));
    backBtn.onclick = () => (back === true ? navBack() : go(back));
  } else {
    backBtn.hidden = true;
    backBtn.onclick = null;
  }
  append(clear($('top-actions')), top);
  const app = clear($('app'));
  append(app, Array.isArray(body) ? body : [body]);
  const bar = clear($('actionbar'));
  const acts = actions.filter(Boolean);
  bar.hidden = acts.length === 0;
  app.classList.toggle('no-bar', acts.length === 0);
  append(bar, acts);
  return app;
}

export function topButton(name, label, onClick) {
  return h('button.top-btn', { 'aria-label': label, title: label, onclick: onClick }, icon(name));
}

/** Botón con forma de señal. color: blue (por defecto), green, red, yellow, white */
export function sign({ text, sub, ico, color, big, small, center, onClick, disabled, chev, type = 'button' }) {
  const cls = ['sign', color, big && 'big', small && 'small', center && 'center'].filter(Boolean).join(' ');
  const b = h('button', { class: cls, type, disabled, onclick: onClick },
    ico ? icon(ico) : null,
    h('span.txt', h('span', text), sub ? h('span.sub', sub) : null),
    chev ? icon('chev', 'chev') : null,
  );
  b.className = cls;
  return b;
}

/** Ejecuta `fn` deshabilitando el botón mientras dura (evita dobles pulsaciones). */
export async function busy(btn, fn) {
  if (btn.disabled) return;
  btn.disabled = true;
  try {
    return await fn();
  } catch (e) {
    toast(e.message || String(e), 'err');
  } finally {
    btn.disabled = false;
  }
}

export function listItem({ title, sub, ico, onClick, cls = '' }) {
  return h('button.list-item', { class: cls, type: 'button', onclick: onClick },
    ico ? icon(ico, 'chev') : null,
    h('div.main', h('div.t', title), sub ? h('div.s', sub) : null),
    onClick ? icon('chev', 'chev') : null,
  );
}

export function section(text) {
  return h('h2.section', text);
}

export function panel(...children) {
  return h('div.panel.bolts', ...children);
}

export function notice(text, type = '', ico = 'warning') {
  return h('div.notice', { class: type }, icon(ico), h('div', text));
}

export function loading() {
  return h('div.loading', h('div.wheel'));
}

/** Campo con etiqueta. opts.pin: 'fijado' | 'admin' */
export function field(label, control, { pin, hint } = {}) {
  return h('label.field',
    h('span.lbl', label,
      pin === 'admin' ? h('span.pin.lock', 'Fijado por la empresa') : pin ? h('span.pin', 'Fijado') : null),
    control,
    hint ? h('p.hint', hint) : null,
  );
}

export function input({ value = '', type = 'text', placeholder = '', plate = false, ...rest } = {}) {
  const el = h('input.input', { type, placeholder, ...rest });
  if (plate) el.classList.add('plate');
  el.value = value ?? '';
  return el;
}

export function textarea({ value = '', placeholder = '' } = {}) {
  const el = h('textarea.input', { placeholder });
  el.value = value ?? '';
  return el;
}

export function select(options, value) {
  const el = h('select.input', options.map(([v, t]) => h('option', { value: v }, t)));
  el.value = value ?? '';
  return el;
}

/** Interruptor de palanca. */
export function toggle(label, checked, onChange, sub) {
  const cb = h('input', { type: 'checkbox', checked });
  cb.addEventListener('change', () => onChange && onChange(cb.checked, cb));
  return h('label.switch.panel', h('span', h('strong', label), sub ? h('div.muted', sub) : null), cb, h('span.lever'));
}

export function toast(msg, type = '') {
  const t = h('div.toast', { class: type }, msg);
  $('toasts').appendChild(t);
  setTimeout(() => t.remove(), type === 'err' ? 6000 : 3500);
}

/** Diálogo. Devuelve el `value` del botón pulsado (o null si se cierra). */
export function modal({ title, body = [], buttons = [{ text: 'Aceptar', value: true }] }) {
  return new Promise((resolve) => {
    const bg = h('div.modal-bg');
    const close = (v) => {
      bg.remove();
      resolve(v);
    };
    bg.addEventListener('click', (e) => e.target === bg && close(null));
    const btns = buttons.map((b) => sign({ text: b.text, color: b.color, small: true, center: true, onClick: () => close(b.value) }));
    bg.appendChild(h('div.modal', h('div.panel.bolts', title ? h('h3', title) : null, ...(Array.isArray(body) ? body : [body]), h('div.btns', btns))));
    document.body.appendChild(bg);
  });
}

export function confirm(text, { ok = 'Sí', cancel = 'No', danger = false } = {}) {
  return modal({
    title: text,
    buttons: [
      { text: cancel, value: false, color: 'white' },
      { text: ok, value: true, color: danger ? 'red' : 'green' },
    ],
  }).then((v) => v === true);
}

/** Error al cargar una pantalla, con botón de reintentar. */
export function errorView(e, retry) {
  return [
    notice(e.offline ? 'No hay conexión con el servidor. Comprueba la cobertura e inténtalo de nuevo.' : e.message, 'danger', e.offline ? 'wifioff' : 'warning'),
    retry ? sign({ text: 'Reintentar', ico: 'refresh', onClick: retry }) : null,
  ];
}

/** Lee un fichero elegido por el usuario. */
export function pickFile(accept) {
  return new Promise((resolve) => {
    const inp = h('input', { type: 'file', accept, style: { display: 'none' } });
    inp.addEventListener('change', () => {
      resolve(inp.files[0] || null);
      inp.remove();
    });
    document.body.appendChild(inp);
    inp.click();
  });
}
