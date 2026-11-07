// Inicio de sesión: manual (servidor, usuario, contraseña, código 2FA) o con el código QR
// que enseña el administrador.

import { h, icon } from '../core/dom.js';
import { api, session, normalizeServer, ApiError } from '../core/api.js';
import { page, panel, field, input, sign, busy, toast, notice, modal } from '../core/ui.js';
import { isAndroid, native } from '../core/native.js';
import { go } from '../core/router.js';
import { setUser } from '../core/state.js';

const LAST_SERVER = 'deca.lastServer';

async function checkServer(server) {
  try {
    const info = await api('/api/info', { server, token: null, timeout: 10000 });
    if (!info || info.servicio !== 'DeCA Server') throw new Error();
  } catch (e) {
    if (e.offline || !(e instanceof ApiError)) {
      throw new Error('No se encuentra el servidor en esa dirección. Revísala o comprueba la conexión.');
    }
    throw new Error('Esa dirección no es un servidor DeCA.');
  }
}

async function finish(server, resp) {
  session.set({ server, token: resp.token, user: resp.usuario });
  setUser(resp.usuario);
  try { localStorage.setItem(LAST_SERVER, server); } catch { /* nada */ }
  toast(`Hola, ${resp.usuario.nombre}`, 'ok');
  go('/', { replace: true });
}

/** Lee un QR con la cámara del navegador (BarcodeDetector, Chrome para Android). */
async function scanInBrowser() {
  if (!('BarcodeDetector' in window) || !navigator.mediaDevices) {
    throw new Error('Este navegador no puede leer códigos QR. Usa la app Android o el inicio de sesión manual.');
  }
  const detector = new window.BarcodeDetector({ formats: ['qr_code'] });
  const stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: 'environment' } });
  const video = h('video', { autoplay: true, playsinline: true, muted: true, style: { width: '100%', borderRadius: '12px' } });
  video.srcObject = stream;
  let done = false;
  const result = new Promise((resolve) => {
    const tick = async () => {
      if (done) return;
      try {
        const codes = await detector.detect(video);
        if (codes.length) return resolve(codes[0].rawValue);
      } catch { /* aún sin imagen */ }
      setTimeout(tick, 250);
    };
    video.addEventListener('playing', tick, { once: true });
  });
  const dialog = modal({ title: 'Apunta al código QR', body: [video], buttons: [{ text: 'Cancelar', value: null, color: 'white' }] });
  const text = await Promise.race([result, dialog]);
  done = true;
  stream.getTracks().forEach((t) => t.stop());
  document.querySelector('.modal-bg')?.remove();
  if (!text) throw new Error('Cancelado');
  return text;
}

async function qrLogin() {
  const text = isAndroid ? await native.scanQr() : await scanInBrowser();
  let data;
  try {
    data = JSON.parse(text);
  } catch {
    throw new Error('Ese código QR no es de acceso a DeCA.');
  }
  if (data.tipo !== 'deca-login' || !data.codigo) throw new Error('Ese código QR no es de acceso a DeCA.');
  const server = isAndroid ? normalizeServer(data.servidor) : location.origin;
  const resp = await api('/api/auth/qr', { method: 'POST', server, token: null, body: { codigo: data.codigo } });
  await finish(server, resp);
}

export async function view() {
  let last = '';
  try { last = localStorage.getItem(LAST_SERVER) || ''; } catch { /* nada */ }
  const server = input({ value: last, placeholder: 'https://deca.miempresa.es', type: 'url', autocomplete: 'url', inputmode: 'url' });
  const user = input({ placeholder: 'Usuario', autocomplete: 'username', autocapitalize: 'none' });
  const pass = input({ type: 'password', placeholder: 'Contraseña', autocomplete: 'current-password' });
  const code = input({ placeholder: '000000', inputmode: 'numeric', maxlength: 6, autocomplete: 'one-time-code' });
  const codeField = field('Código de verificación (6 dígitos)', code, { hint: 'Ábrelo en tu app de códigos (Google Authenticator, Aegis…).' });
  codeField.hidden = true;

  const enter = sign({ text: 'Entrar', ico: 'key', color: 'green', center: true, type: 'submit' });
  const form = h('form', {
    onsubmit: (e) => {
      e.preventDefault();
      busy(enter, async () => {
        const srv = isAndroid ? normalizeServer(server.value) : location.origin;
        if (isAndroid && !srv) throw new Error('Escribe la dirección del servidor');
        if (!user.value.trim() || !pass.value) throw new Error('Escribe tu usuario y tu contraseña');
        if (isAndroid) await checkServer(srv);
        try {
          const resp = await api('/api/auth/login', {
            method: 'POST', server: srv, token: null,
            body: { usuario: user.value.trim(), contrasena: pass.value, codigo_2fa: code.value.trim() || undefined },
          });
          await finish(srv, resp);
        } catch (e) {
          if (e.code === 'requiere_2fa') {
            codeField.hidden = false;
            code.focus();
            throw new Error('Escribe el código de verificación de 6 dígitos');
          }
          if (e.offline) throw new Error('No hay conexión con el servidor.');
          throw e;
        }
      });
    },
  },
    panel(
      h('div.center', h('div', { style: { color: 'var(--azul)', display: 'inline-block' } }, icon('deca', 'ico')), h('h3', 'Entrar en DeCA')),
      isAndroid ? field('Dirección del servidor', server, { hint: 'Te la da tu empresa. Normalmente empieza por https://' }) : null,
      field('Usuario', user),
      field('Contraseña', pass),
      codeField,
    ),
  );

  const qrBtn = sign({
    text: 'Entrar con código QR', sub: 'El administrador te lo enseña en su móvil', ico: 'qr', big: false,
    onClick: () => busy(qrBtn, async () => {
      try {
        await qrLogin();
      } catch (e) {
        if (e.message !== 'Cancelado') throw e;
      }
    }),
  });

  page({
    title: 'Iniciar sesión',
    body: [
      qrBtn,
      h('h2.section', 'O escribe tus datos'),
      form,
      !isAndroid ? notice('Primera vez: el administrador entra con el usuario «Admin» y la contraseña «1234», y después la cambia.', 'info', 'key') : null,
    ],
    actions: [enter],
  });
  enter.addEventListener('click', () => form.requestSubmit());
}
