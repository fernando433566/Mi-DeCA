// Arranque de la aplicación DeCA (web y app Android comparten este código).

import { api, session, setUnauthorizedHandler } from './core/api.js';
import { route, startRouter, go, currentPath } from './core/router.js';
import { isAndroid } from './core/native.js';
import { toast } from './core/ui.js';
import { state, setNet } from './core/state.js';
import { registerOfflineApp } from './core/platform.js';
import { startQueue } from './core/queue.js';

import * as login from './views/login.js';
import * as setup from './views/setup.js';
import * as home from './views/home.js';
import * as decaForm from './views/deca-form.js';
import * as decaView from './views/deca-view.js';
import * as settings from './views/settings.js';
import * as company from './views/company.js';
import * as workers from './views/workers.js';
import * as prefs from './views/prefs.js';
import * as account from './views/account.js';
import * as vehicles from './views/vehicles.js';
import * as parties from './views/parties.js';
import * as server from './views/server.js';
import * as maps from './views/maps.js';
import * as install from './views/install.js';


const PUBLIC_ROUTES = ['/login', '/instalar'];

// ---------------- Rutas ----------------
route('/login', login.view);
route('/instalar', install.view);
route('/setup', setup.view);
route('/', home.view);
route('/nuevo', decaForm.createView);
route('/deca/:id', decaView.view);
route('/deca/:id/modificar', decaForm.modifyView);
route('/pendiente/:id', home.pendingView);
route('/recientes', decaView.recentView);
route('/ajustes', settings.view);
route('/ajustes/preferencias', prefs.ownView);
route('/ajustes/cuenta', account.view);
route('/ajustes/vehiculos', vehicles.view);
route('/ajustes/empresas', parties.view);
route('/ajustes/mapas', maps.view);
route('/empresa/nueva', company.createView);
route('/empresa/:cid', company.menuView);
route('/empresa/:cid/datos', company.dataView);
route('/empresa/:cid/trabajadores', company.workersMenuView);
route('/empresa/:cid/trabajadores/lista', workers.listView);
route('/empresa/:cid/trabajadores/nuevo', workers.createView);
route('/empresa/:cid/preferencias', company.prefsMenuView);
route('/empresa/:cid/preferencias/empresa', prefs.companyView);
route('/empresa/:cid/preferencias/trabajador', workers.pickForPrefsView);
route('/empresa/:cid/administracion', company.adminMenuView);
route('/empresa/:cid/decas', company.decasView);
route('/trabajador/:wid', workers.profileView);
route('/trabajador/:wid/preferencias', prefs.workerView);
route('/servidor', server.view);
route('/servidor/copia', server.backupView);

// ---------------- Sesión ----------------
setUnauthorizedHandler(() => {
  session.clear();
  state.user = null;
  toast('La sesión ha caducado. Vuelve a entrar.', 'err');
  go('/login', { replace: true });
});

let refreshed = false;

/** Comprueba la sesión antes de cada pantalla. */
async function guard() {
  const path = currentPath();
  const s = session.get();
  if (!s || !s.token) {
    state.user = null;
    if (!PUBLIC_ROUTES.includes(path)) {
      go('/login', { replace: true });
      return false;
    }
    return true;
  }
  state.user = s.user;
  if (!refreshed) {
    refreshed = true;
    try {
      const me = await api('/api/auth/me');
      state.user = me;
      session.set({ ...s, user: me });
      setNet(true);
    } catch (e) {
      if (e.offline) setNet(false);
      else if (e.status === 401) return false;
    }
  }
  if (path === '/login') {
    go('/', { replace: true });
    return false;
  }
  // Asistente del primer arranque para el administrador.
  if (state.user && state.user.rol === 'admin' && state.user.contrasena_por_defecto && path !== '/setup') {
    go('/setup', { replace: true });
    return false;
  }
  return true;
}

window.addEventListener('online', () => setNet(true));
// Cada petición al servidor actualiza la franja de "sin conexión".
window.addEventListener('deca:online', () => setNet(true));
window.addEventListener('deca:offline', () => setNet(false));
window.addEventListener('offline', () => setNet(false));
// Android avisa cuando la cola sin conexión ha creado DeCA pendientes.
window.__decaQueueChanged = () => {
  if (currentPath() === '/') home.view();
};

registerOfflineApp();
startRouter(guard);
startQueue();
