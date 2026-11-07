// Ajustes -> Mi cuenta: cambiar contraseña y código de verificación de 6 dígitos (2FA).

import { h, trustedSvg } from '../core/dom.js';
import { api } from '../core/api.js';
import { page, sign, section, panel, field, input, busy, toast, notice, modal, loading } from '../core/ui.js';
import { password } from '../core/validate.js';
import { state, setUser } from '../core/state.js';

export async function view() {
  page({ title: 'Mi cuenta', back: '/ajustes', body: loading() });
  try {
    setUser(await api('/api/auth/me'));
  } catch { /* se usa lo guardado */ }
  const u = state.user;

  const cur = input({ type: 'password', autocomplete: 'current-password' });
  const p1 = input({ type: 'password', autocomplete: 'new-password' });
  const p2 = input({ type: 'password', autocomplete: 'new-password' });
  const savePw = sign({ text: 'Cambiar contraseña', ico: 'key', small: true, color: 'green' });
  savePw.onclick = () => busy(savePw, async () => {
    password(p1.value, p2.value);
    await api('/api/me/password', { method: 'PUT', body: { actual: cur.value, nueva: p1.value } });
    cur.value = p1.value = p2.value = '';
    toast('Contraseña cambiada', 'ok');
  });

  const tfa = u.tiene_2fa
    ? sign({ text: 'Quitar código de verificación', ico: 'shield', color: 'yellow', onClick: disable2fa })
    : sign({ text: 'Añadir código de verificación', sub: 'Código de 6 dígitos al entrar (2FA)', ico: 'shield', color: 'green', onClick: enable2fa });

  page({
    title: 'Mi cuenta',
    back: '/ajustes',
    body: [
      panel(h('dl.kv', h('dt', 'Nombre'), h('dd', u.nombre), h('dt', 'Usuario'), h('dd', u.usuario), h('dt', 'Empresa'), h('dd', u.empresa_nombre || (u.rol === 'admin' ? 'Administrador' : '—')))),
      section('Verificación en dos pasos'),
      u.tiene_2fa ? notice('Activada: al entrar se pide el código de tu app de autenticación.', 'ok', 'shield') : null,
      tfa,
      section('Contraseña'),
      panel(field('Contraseña actual', cur), field('Nueva contraseña', p1), field('Repite la nueva', p2), savePw),
    ],
  });
}

async function enable2fa() {
  const r = await api('/api/me/2fa/iniciar', { method: 'POST' });
  const code = input({ inputmode: 'numeric', maxlength: 6, placeholder: '000000' });
  const ok = await modal({
    title: 'Añadir código de verificación',
    body: [
      h('p', '1. Escanea este QR con tu app de códigos (Google Authenticator, Aegis, Microsoft Authenticator).'),
      trustedSvg(r.qr_svg, 'qr-big'),
      h('p.hint.center', `O escribe la clave: ${r.secreto}`),
      field('2. Escribe el código de 6 dígitos que aparece', code),
    ],
    buttons: [{ text: 'Cancelar', value: false, color: 'white' }, { text: 'Activar', value: true, color: 'green' }],
  });
  if (!ok) return;
  try {
    await api('/api/me/2fa/activar', { method: 'POST', body: { codigo: code.value.trim() } });
    toast('Verificación en dos pasos activada', 'ok');
  } catch (e) {
    toast(e.message, 'err');
  }
  view();
}

async function disable2fa() {
  const code = input({ inputmode: 'numeric', maxlength: 6, placeholder: '000000' });
  const ok = await modal({
    title: 'Quitar código de verificación',
    body: [field('Escribe un código actual de tu app', code)],
    buttons: [{ text: 'Cancelar', value: false, color: 'white' }, { text: 'Quitar', value: true, color: 'red' }],
  });
  if (!ok) return;
  try {
    await api('/api/me/2fa/desactivar', { method: 'POST', body: { codigo: code.value.trim() } });
    toast('Verificación en dos pasos desactivada', 'ok');
  } catch (e) {
    toast(e.message, 'err');
  }
  view();
}
