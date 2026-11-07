// Asistente del primer arranque (administrador): cambiar la contraseña "1234",
// crear la empresa y comprobar la dirección pública del servidor.

import { h } from '../core/dom.js';
import { api } from '../core/api.js';
import { page, panel, field, input, sign, busy, toast, notice, loading, section } from '../core/ui.js';
import { go } from '../core/router.js';
import { setUser } from '../core/state.js';
import { password } from '../core/validate.js';

export async function view() {
  page({ title: 'Configuración inicial', body: loading() });
  const [me, status] = await Promise.all([api('/api/auth/me'), api('/api/setup/status').catch(() => null)]);

  if (me.contrasena_por_defecto) return passwordStep(me);
  if (status && status.necesita_empresa) return companyStep();
  return urlStep(status);
}

function steps(n) {
  return h('div.steps', [1, 2, 3].map((i) => h('i', { class: i < n ? 'done' : i === n ? 'now' : '' })));
}

function passwordStep() {
  const p1 = input({ type: 'password', autocomplete: 'new-password' });
  const p2 = input({ type: 'password', autocomplete: 'new-password' });
  const next = sign({ text: 'Guardar y seguir', ico: 'check', color: 'green', center: true });
  next.onclick = () => busy(next, async () => {
    password(p1.value, p2.value);
    await api('/api/me/password', { method: 'PUT', body: { actual: '1234', nueva: p1.value } });
    setUser(await api('/api/auth/me'));
    toast('Contraseña cambiada', 'ok');
    view();
  });
  page({
    title: 'Configuración inicial',
    body: [
      steps(1),
      notice('Estás usando la contraseña inicial «1234». Por seguridad, cámbiala ahora.', '', 'lock'),
      panel(field('Nueva contraseña', p1), field('Repite la contraseña', p2)),
    ],
    actions: [next],
  });
}

function companyStep() {
  const nombre = input({ placeholder: 'Transportes García S.L.' });
  const cif = input({ placeholder: 'B12345678', plate: true });
  const dom = input({ placeholder: 'Calle, número, código postal, ciudad' });
  const titular = input({ placeholder: 'Persona o sociedad dueña de los camiones' });
  const next = sign({ text: 'Crear empresa', ico: 'building', color: 'green', center: true });
  next.onclick = () => busy(next, async () => {
    await api('/api/companies', { method: 'POST', body: { nombre: nombre.value, cif: cif.value, domicilio: dom.value, titular: titular.value } });
    toast('Empresa creada', 'ok');
    view();
  });
  page({
    title: 'Configuración inicial',
    body: [
      steps(2),
      section('Tu empresa transportista'),
      panel(
        field('Nombre de la empresa', nombre),
        field('CIF / NIF', cif),
        field('Domicilio', dom),
        field('Nombre transportista (dueño de los vehículos)', titular, { hint: 'Opcional. Aparecerá en los DeCA.' }),
      ),
    ],
    actions: [next],
  });
}

function urlStep(status) {
  const dp = (status && status.direccion_publica) || {};
  const done = sign({ text: 'Empezar', ico: 'check', color: 'green', center: true, onClick: () => go('/', { replace: true }) });
  page({
    title: 'Configuración inicial',
    body: [
      steps(3),
      section('Dirección pública (QR)'),
      dp.url
        ? notice(dp.verificada ? `Los QR usarán ${dp.url} (comprobado por HTTPS).` : `Los QR usarán ${dp.url}. ${dp.aviso || ''}`, dp.verificada ? 'ok' : '', dp.verificada ? 'check' : 'warning')
        : notice('Aún no se conoce la dirección pública. Abre esta página una vez desde la dirección https:// de Cloudflare Tunnel y se detectará sola. También puedes escribirla en Ajustes → Servidor.', 'info', 'server'),
      h('p.hint', 'Ya puedes crear trabajadores en Ajustes → tu empresa → Trabajadores.'),
    ],
    actions: [done],
  });
}
