// Ajustes. El menú cambia según el rol:
// - Trabajador: Preferencias, Mi cuenta, Camiones y remolques, Mapas.
// - Administrador: además, una entrada por cada empresa y "Servidor".

import { h } from '../core/dom.js';
import { api, cachedGet, session } from '../core/api.js';
import { page, sign, section, loading, confirm, toast } from '../core/ui.js';
import { go } from '../core/router.js';
import { isAdmin, state, setUser } from '../core/state.js';
import { native, isAndroid } from '../core/native.js';
import { isIOS, isInstalled } from '../core/platform.js';
import { localMapsAvailable } from '../core/localmaps.js';

export async function view() {
  page({ title: 'Ajustes', back: '/', body: loading() });
  // Los permisos pueden haber cambiado (escalado a administrador).
  try { setUser(await api('/api/auth/me')); } catch { /* sin conexión: lo guardado */ }
  const body = [];
  if (isAdmin()) {
    let companies = [];
    try {
      companies = await cachedGet('/api/companies', 'companies');
    } catch { /* sin conexión */ }
    body.push(section('Mis empresas'));
    for (const c of companies) {
      body.push(sign({ text: c.name, sub: 'Trabajadores, preferencias y DeCA emitidos', ico: 'building', chev: true, onClick: () => go(`/empresa/${c.id}`) }));
    }
    body.push(sign({ text: 'Añadir empresa', ico: 'plus', color: 'white', small: true, onClick: () => go('/empresa/nueva') }));
    body.push(section('Servidor'));
    body.push(sign({ text: 'Servidor', sub: 'Copias de seguridad, eliminación automática, dirección pública', ico: 'server', color: 'green', chev: true, onClick: () => go('/servidor') }));
  }
  body.push(section('Mis ajustes'));
  body.push(
    sign({ text: 'Preferencias', sub: 'Datos que se rellenan solos en cada DeCA', ico: 'pin', chev: true, onClick: () => go('/ajustes/preferencias') }),
    sign({ text: 'Mi cuenta', sub: 'Contraseña y código de verificación', ico: 'user', chev: true, onClick: () => go('/ajustes/cuenta') }),
    sign({ text: 'Camiones y remolques', ico: 'truck', chev: true, onClick: () => go('/ajustes/vehiculos') }),
    sign({ text: 'Empresas y ubicaciones', sub: 'Cargadoras, transportistas y sus direcciones', ico: 'building', chev: true, onClick: () => go('/ajustes/empresas') }),
    sign({ text: 'Mapas', sub: localMapsAvailable() ? 'Descargar mapas para usar sin conexión' : 'Mapas de OpenStreetMap', ico: 'map', chev: true, onClick: () => go('/ajustes/mapas') }),
    isIOS && !isInstalled() ? sign({ text: 'Instalar en el iPhone', sub: 'Añade DeCA a la pantalla de inicio', ico: 'share', color: 'yellow', chev: true, onClick: () => go('/instalar') }) : null,
    isAndroid ? sign({
      text: 'Poner el billete en la pantalla de inicio', sub: 'Widget con el último DeCA y su QR', ico: 'qr', color: 'white',
      onClick: () => { if (!native.pinWidget()) toast('Mantén pulsada la pantalla de inicio → Widgets → DeCA'); },
    }) : null,
  );
  const out = sign({
    text: 'Cerrar sesión', ico: 'logout', color: 'red', small: true,
    onClick: async () => {
      if (!(await confirm('¿Cerrar la sesión en este dispositivo?', { ok: 'Cerrar sesión', danger: true }))) return;
      try { await api('/api/auth/logout', { method: 'POST' }); } catch { /* da igual */ }
      session.clear();
      native.updateWidget([], 0);
      go('/login', { replace: true });
    },
  });
  body.push(h('div.mt', out));
  const s = session.get();
  body.push(h('p.hint.center', `${state.user?.usuario || ''} · ${s?.server || location.origin}${isAndroid ? ` · app ${native.appVersion()}` : ''}`));
  page({ title: 'Ajustes', back: '/', body });
}
