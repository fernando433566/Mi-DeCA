// Formatos en español (fechas en hora peninsular, números con punto de miles).

const TZ = 'Europe/Madrid';

export function fecha(v) {
  if (!v) return '';
  // Fechas sin hora (AAAA-MM-DD) se muestran tal cual, sin cambio de zona.
  if (/^\d{4}-\d{2}-\d{2}$/.test(v)) {
    const [y, m, d] = v.split('-');
    return `${d}/${m}/${y}`;
  }
  return new Date(v).toLocaleDateString('es-ES', { timeZone: TZ, day: '2-digit', month: '2-digit', year: 'numeric' });
}

export function fechaHora(v) {
  if (!v) return '';
  return new Date(v).toLocaleString('es-ES', {
    timeZone: TZ, day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit',
  });
}

export function diaCorto(v) {
  return new Date(v).toLocaleDateString('es-ES', { timeZone: TZ, weekday: 'short', day: 'numeric', month: 'short' });
}

export function num(v, dec = 2) {
  if (v === null || v === undefined || v === '') return '';
  return Number(v).toLocaleString('es-ES', { maximumFractionDigits: dec });
}

export function kg(v) {
  return v ? `${num(v)} kg` : '';
}

export function hoy() {
  const d = new Date();
  const p = new Intl.DateTimeFormat('en-CA', { timeZone: TZ, year: 'numeric', month: '2-digit', day: '2-digit' }).format(d);
  return p; // AAAA-MM-DD
}

export function bytes(n) {
  if (!n) return '';
  if (n > 1e9) return `${num(n / 1e9, 1)} GB`;
  if (n > 1e6) return `${num(n / 1e6, 0)} MB`;
  return `${num(n / 1e3, 0)} KB`;
}

/** Número escrito por el usuario ("24.500,5" o "24500.5") -> 24500.5 */
export function parseNum(s) {
  if (s === null || s === undefined) return null;
  let t = String(s).trim().replace(/\s/g, '');
  if (!t) return null;
  if (t.includes(',')) t = t.replace(/\./g, '').replace(',', '.');
  const n = Number(t);
  return Number.isFinite(n) ? n : NaN;
}

export function uuid() {
  if (crypto.randomUUID) return crypto.randomUUID();
  const b = crypto.getRandomValues(new Uint8Array(16));
  b[6] = (b[6] & 0x0f) | 0x40;
  b[8] = (b[8] & 0x3f) | 0x80;
  const x = [...b].map((v) => v.toString(16).padStart(2, '0')).join('');
  return `${x.slice(0, 8)}-${x.slice(8, 12)}-${x.slice(12, 16)}-${x.slice(16, 20)}-${x.slice(20)}`;
}
