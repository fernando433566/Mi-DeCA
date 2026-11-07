// Enrutador por hash (#/ruta). Cada vista es una función async (params, query) => void.

const routes = [];
let current = null;
let leaveHook = null;

export function route(pattern, view) {
  const keys = [];
  const re = new RegExp('^' + pattern.replace(/:(\w+)/g, (_, k) => (keys.push(k), '([^/]+)')) + '/?$');
  routes.push({ re, keys, view });
}

export function currentPath() {
  return (location.hash || '#/').slice(1).split('?')[0] || '/';
}

export function go(path, { replace = false } = {}) {
  const target = '#' + path;
  if (location.hash === target) return render();
  if (replace) location.replace(target);
  else location.hash = target;
}

export function back(fallback = '/') {
  if (history.length > 1 && sessionStorage.getItem('deca.nav') === '1') history.back();
  else go(fallback, { replace: true });
}

/** Una vista puede registrar una función a ejecutar al salir (cerrar mapas, temporizadores...). */
export function onLeave(fn) {
  leaveHook = fn;
}

export async function render() {
  if (leaveHook) {
    try { leaveHook(); } catch { /* nada */ }
    leaveHook = null;
  }
  const raw = (location.hash || '#/').slice(1);
  const [path, qs] = raw.split('?');
  const query = Object.fromEntries(new URLSearchParams(qs || ''));
  for (const r of routes) {
    const m = (path || '/').match(r.re);
    if (m) {
      const params = Object.fromEntries(r.keys.map((k, i) => [k, decodeURIComponent(m[i + 1])]));
      current = path;
      window.scrollTo(0, 0);
      await r.view(params, query);
      return;
    }
  }
  go('/', { replace: true });
}

export function startRouter(guard) {
  let first = true;
  window.addEventListener('hashchange', () => {
    if (!first) sessionStorage.setItem('deca.nav', '1');
    guard().then((ok) => ok && render());
  });
  guard().then((ok) => {
    first = false;
    if (ok) render();
  });
}

export function isCurrent(path) {
  return current === path;
}
