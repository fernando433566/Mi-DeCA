// Utilidades DOM. Todo el texto se inserta con textContent (nunca innerHTML con datos
// del usuario) para evitar inyección de HTML.

/** Crea un elemento: h('div.clase#id', {atributos}, ...hijos) */
export function h(spec, attrs, ...children) {
  if (attrs == null || typeof attrs !== 'object' || attrs instanceof Node || Array.isArray(attrs)) {
    if (attrs !== undefined && attrs !== null) children.unshift(attrs);
    attrs = {};
  }
  const m = spec.match(/^([a-z0-9-]+)?((?:[.#][\w-]+)*)$/i);
  const el = document.createElement((m && m[1]) || 'div');
  if (m && m[2]) {
    for (const part of m[2].match(/[.#][\w-]+/g)) {
      if (part[0] === '.') el.classList.add(part.slice(1));
      else el.id = part.slice(1);
    }
  }
  for (const [k, v] of Object.entries(attrs)) {
    if (v === undefined || v === null || v === false) continue;
    if (k.startsWith('on') && typeof v === 'function') el.addEventListener(k.slice(2).toLowerCase(), v);
    else if (k === 'class') el.className += ' ' + v;
    else if (k === 'style' && typeof v === 'object') Object.assign(el.style, v);
    else if (k === 'dataset') Object.assign(el.dataset, v);
    else if (k in el && k !== 'list' && typeof v !== 'string') el[k] = v;
    else el.setAttribute(k, v === true ? '' : v);
  }
  append(el, children);
  return el;
}

export function append(el, children) {
  for (const c of children.flat(Infinity)) {
    if (c === null || c === undefined || c === false) continue;
    el.appendChild(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return el;
}

export function clear(el) {
  while (el.firstChild) el.removeChild(el.firstChild);
  return el;
}

/** SVG de confianza generado por el propio servidor o por qr.js. */
export function trustedSvg(svgText, cls = '') {
  const tpl = document.createElement('template');
  tpl.innerHTML = svgText.trim();
  const svg = tpl.content.querySelector('svg');
  const wrap = h('div', { class: cls });
  if (svg) wrap.appendChild(svg);
  return wrap;
}

// ---------------- Pictogramas (estilo señal, trazo blanco) ----------------
const P = {
  back: '<path d="M30 10 16 24l14 14"/>',
  chev: '<path d="M18 10l14 14-14 14"/>',
  truck: '<path d="M4 12h24v20H4z"/><path d="M28 18h9l7 8v6h-16z"/><circle cx="12" cy="35" r="4"/><circle cx="36" cy="35" r="4"/>',
  deca: '<path d="M12 4h17l9 9v31H12z"/><path d="M29 4v9h9"/><path d="M17 22h14M17 29h14M17 36h8"/>',
  calendar: '<rect x="6" y="9" width="36" height="33" rx="3"/><path d="M6 18h36M15 4v9M33 4v9"/><path d="M14 26h5M22 26h5M30 26h5M14 33h5M22 33h5"/>',
  gear: '<circle cx="24" cy="24" r="7"/><path d="M24 4v7M24 37v7M4 24h7M37 24h7M9.9 9.9l5 5M33.1 33.1l5 5M9.9 38.1l5-5M33.1 14.9l5-5"/>',
  map: '<path d="M5 10l12-5 14 5 12-5v33l-12 5-14-5-12 5z"/><path d="M17 5v33M31 10v33"/>',
  user: '<circle cx="24" cy="15" r="9"/><path d="M7 44c2-10 9-15 17-15s15 5 17 15"/>',
  users: '<circle cx="17" cy="16" r="7"/><path d="M3 41c2-8 7-12 14-12s12 4 14 12"/><circle cx="33" cy="14" r="6"/><path d="M31 27c7 0 12 4 14 11"/>',
  qr: '<rect x="5" y="5" width="14" height="14"/><rect x="29" y="5" width="14" height="14"/><rect x="5" y="29" width="14" height="14"/><path d="M29 29h5v5h-5zM38 29h5M29 38h5v5M38 38h5v5"/>',
  key: '<circle cx="15" cy="30" r="9"/><path d="M21 24 40 5M33 12l5 5M28 17l4 4"/>',
  shield: '<path d="M24 4 7 10v12c0 11 7 18 17 22 10-4 17-11 17-22V10z"/><path d="M16 24l6 6 10-11"/>',
  download: '<path d="M24 5v26M13 21l11 11 11-11"/><path d="M6 37v6h36v-6"/>',
  upload: '<path d="M24 32V6M13 16 24 5l11 11"/><path d="M6 37v6h36v-6"/>',
  server: '<rect x="6" y="6" width="36" height="14" rx="2"/><rect x="6" y="28" width="36" height="14" rx="2"/><path d="M13 13h2M13 35h2"/>',
  trash: '<path d="M7 11h34M18 11V5h12v6M11 11l2 32h22l2-32"/><path d="M20 19v17M28 19v17"/>',
  pin: '<path d="M24 44S9 29 9 19a15 15 0 0 1 30 0c0 10-15 25-15 25z"/><circle cx="24" cy="19" r="5"/>',
  plus: '<path d="M24 7v34M7 24h34"/>',
  edit: '<path d="M30 7l11 11-23 23H7V30z"/><path d="M25 12l11 11"/>',
  check: '<path d="M8 25l11 11 21-23"/>',
  warning: '<path d="M24 4 2 42h44z"/><path d="M24 17v12M24 34v3"/>',
  box: '<path d="M24 4 42 13v22L24 44 6 35V13z"/><path d="M6 13l18 9 18-9M24 22v22"/>',
  building: '<path d="M8 44V8h20v36M28 18h12v26"/><path d="M14 15h3M20 15h3M14 23h3M20 23h3M14 31h3M20 31h3M33 26h3M33 34h3M4 44h40"/>',
  route: '<circle cx="11" cy="37" r="5"/><circle cx="37" cy="11" r="5"/><path d="M16 37h14a7 7 0 0 0 0-14H18a7 7 0 0 1 0-14h14"/>',
  clock: '<circle cx="24" cy="24" r="19"/><path d="M24 12v13l9 6"/>',
  logout: '<path d="M19 6H7v36h12"/><path d="M31 14l10 10-10 10M41 24H18"/>',
  refresh: '<path d="M40 22A16 16 0 1 0 36 35"/><path d="M41 9v13H28"/>',
  wifioff: '<path d="M4 16a30 30 0 0 1 40 0M10 23a21 21 0 0 1 28 0M16 30a12 12 0 0 1 16 0"/><circle cx="24" cy="38" r="2.5"/><path d="M6 6l36 36"/>',
  camera: '<path d="M4 14h10l4-6h12l4 6h10v26H4z"/><circle cx="24" cy="26" r="8"/>',
  lock: '<rect x="9" y="21" width="30" height="22" rx="3"/><path d="M15 21v-6a9 9 0 0 1 18 0v6"/>',
  search: '<circle cx="20" cy="20" r="13"/><path d="M30 30l13 13"/>',
  weight: '<path d="M13 16h22l6 28H7z"/><circle cx="24" cy="10" r="5"/>',
  send: '<path d="M4 22 44 5 30 44l-8-15z"/><path d="M22 29l22-24"/>',
  share: '<path d="M24 30V5M14 14 24 4l10 10"/><path d="M15 20H8v24h32V20h-7"/>',
  eye: '<path d="M2 24s8-14 22-14 22 14 22 14-8 14-22 14S2 24 2 24z"/><circle cx="24" cy="24" r="6"/>',
};

export function icon(name, cls = 'ico') {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 48 48');
  svg.setAttribute('class', cls);
  svg.setAttribute('fill', 'none');
  svg.setAttribute('stroke', 'currentColor');
  svg.setAttribute('stroke-width', '3.5');
  svg.setAttribute('stroke-linecap', 'round');
  svg.setAttribute('stroke-linejoin', 'round');
  svg.setAttribute('aria-hidden', 'true');
  svg.innerHTML = P[name] || P.deca; // contenido constante, no datos del usuario
  return svg;
}
