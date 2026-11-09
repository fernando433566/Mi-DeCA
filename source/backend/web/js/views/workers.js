// Ajustes -> <Empresa> -> Trabajadores -> Trabajadores en mi empresa (administrador).

import { h } from '../core/dom.js';
import { api } from '../core/api.js';
import { page, sign, section, loading, errorView, panel, field, input, busy, toast, confirm, listItem, modal, notice, toggle } from '../core/ui.js';
import { go, onLeave } from '../core/router.js';
import { trustedSvg } from '../core/dom.js';
import { password } from '../core/validate.js';
import { state, isAdmin } from '../core/state.js';
import { fechaHora } from '../core/format.js';

export async function listView({ cid }) {
  page({ title: 'Trabajadores en mi empresa', back: `/empresa/${cid}/trabajadores`, body: loading() });
  let list;
  try {
    list = await api(`/api/companies/${cid}/workers`);
  } catch (e) {
    return page({ title: 'Trabajadores en mi empresa', back: `/empresa/${cid}/trabajadores`, body: errorView(e, () => listView({ cid })) });
  }
  page({
    title: 'Trabajadores en mi empresa',
    back: `/empresa/${cid}/trabajadores`,
    body: [
      sign({ text: 'Crear cuenta trabajador', ico: 'plus', color: 'green', chev: true, onClick: () => go(`/empresa/${cid}/trabajadores/nuevo`) }),
      section(`${list.length} trabajadores`),
      ...list.map((w) => listItem({
        title: w.nombre,
        sub: `Usuario: ${w.usuario} · ${w.decas_7_dias} DeCA esta semana${w.administrador ? ' · Administrador' : ''}${w.tiene_2fa ? ' · con código de verificación' : ''}`,
        ico: 'user',
        onClick: () => go(`/trabajador/${w.id}`),
      })),
    ],
  });
}

export async function createView({ cid }) {
  const user = input({ autocapitalize: 'none', placeholder: 'juan.perez' });
  const name = input({ placeholder: 'Juan Pérez' });
  const p1 = input({ type: 'password', autocomplete: 'new-password' });
  const p2 = input({ type: 'password', autocomplete: 'new-password' });
  const save = sign({ text: 'Crear cuenta', ico: 'check', color: 'green', center: true });
  save.onclick = () => busy(save, async () => {
    password(p1.value, p2.value);
    const w = await api(`/api/companies/${cid}/workers`, { method: 'POST', body: { usuario: user.value.trim(), nombre: name.value.trim(), contrasena: p1.value } });
    toast('Cuenta creada. Enséñale el código QR para que entre.', 'ok');
    go(`/trabajador/${w.id}`, { replace: true });
  });
  page({
    title: 'Crear cuenta trabajador',
    back: `/empresa/${cid}/trabajadores/lista`,
    body: [panel(
      field('Usuario (para entrar)', user, { hint: 'Sin espacios. Ej.: juan.perez' }),
      field('Nombre completo', name, { hint: 'Aparece en los DeCA y en el nombre del fichero.' }),
      field('Contraseña', p1),
      field('Repite la contraseña', p2),
    )],
    actions: [save],
  });
}

/** Muestra el QR de acceso automático con cuenta atrás. */
async function showLoginQr(w) {
  const r = await api(`/api/workers/${w.id}/qr-login`, { method: 'POST' });
  const left = h('div.center.muted');
  const tick = () => {
    const s = Math.max(0, Math.round((new Date(r.caduca) - Date.now()) / 1000));
    left.textContent = s > 0 ? `Caduca en ${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}` : 'Caducado. Genera otro.';
  };
  tick();
  const timer = setInterval(tick, 1000);
  await modal({
    title: `Acceso de ${w.nombre}`,
    body: [
      trustedSvg(r.qr_svg, 'qr-big'),
      h('p.center', 'En su móvil: abrir la app DeCA → «Entrar con código QR» → escanear.'),
      left,
      notice(`Servidor: ${r.contenido.servidor}`, 'info', 'server'),
    ],
    buttons: [{ text: 'Cerrar', value: true, color: 'green' }],
  });
  clearInterval(timer);
}

export async function profileView({ wid }) {
  page({ title: 'Trabajador', back: true, body: loading() });
  let w;
  try {
    w = await api(`/api/workers/${wid}`);
  } catch (e) {
    return page({ title: 'Trabajador', back: true, body: errorView(e, () => profileView({ wid })) });
  }
  const back = w.empresa_id ? `/empresa/${w.empresa_id}/trabajadores/lista` : '/ajustes';
  const user = input({ value: w.usuario, autocapitalize: 'none' });
  const name = input({ value: w.nombre });
  const p1 = input({ type: 'password', autocomplete: 'new-password', placeholder: 'Déjalo vacío para no cambiarla' });
  const save = sign({ text: 'Guardar cambios', ico: 'check', small: true, color: 'green' });
  save.onclick = () => busy(save, async () => {
    if (p1.value) password(p1.value);
    await api(`/api/workers/${wid}`, { method: 'PUT', body: { usuario: user.value.trim(), nombre: name.value.trim(), contrasena: p1.value || undefined } });
    p1.value = '';
    toast('Datos guardados', 'ok');
  });
  // Casilla solo para administradores; nadie puede cambiarse su propio permiso.
  const adminToggle = isAdmin() && state.user.id !== w.id
    ? toggle('Administrador', w.administrador, async (value, cb) => {
      const text = value
        ? `¿Convertir a ${w.nombre} en administrador? Podrá ver los DeCA de todos, gestionar trabajadores y el servidor.`
        : `¿Quitar a ${w.nombre} los permisos de administrador?`;
      if (!(await confirm(text, { danger: value, ok: value ? 'Hacer administrador' : 'Quitar' }))) {
        cb.checked = !value;
        return;
      }
      try {
        await api(`/api/workers/${wid}`, { method: 'PUT', body: { administrador: value } });
        toast(value ? `${w.nombre} ya es administrador` : `${w.nombre} ya no es administrador`, 'ok');
      } catch (e) {
        cb.checked = !value;
        toast(e.message, 'err');
      }
    }, 'Puede ver todos los DeCA y gestionar trabajadores, empresas y servidor.')
    : null;
  const qrBtn = sign({ text: 'Enseñar código QR', sub: 'Para que entre sin escribir nada', ico: 'qr', color: 'green', center: true });
  qrBtn.onclick = () => busy(qrBtn, () => showLoginQr(w));

  page({
    title: w.nombre,
    back,
    body: [
      panel(h('dl.kv',
        h('dt', 'Usuario'), h('dd', w.usuario),
        h('dt', 'Cuenta creada'), h('dd', fechaHora(w.creado)),
        h('dt', 'DeCA (7 días)'), h('dd', String(w.decas_7_dias)),
        h('dt', 'Verificación 2 pasos'), h('dd', w.tiene_2fa ? 'Activada' : 'No'),
      )),
      adminToggle,
      sign({ text: 'Gestionar preferencias', sub: 'Datos fijados para este trabajador', ico: 'pin', chev: true, onClick: () => go(`/trabajador/${wid}/preferencias`) }),
      w.tiene_2fa ? sign({
        text: 'Quitar código de verificación', sub: 'Si ha perdido el móvil', ico: 'shield', color: 'yellow',
        onClick: async () => {
          if (!(await confirm(`¿Quitar la verificación en dos pasos de ${w.nombre}?`))) return;
          await api(`/api/workers/${wid}/2fa`, { method: 'DELETE' }).catch((e) => toast(e.message, 'err'));
          toast('Código de verificación eliminado', 'ok');
          profileView({ wid });
        },
      }) : null,
      section('Cambiar datos'),
      panel(field('Usuario', user), field('Nombre', name), field('Nueva contraseña', p1), save),
      section('Zona peligrosa'),
      sign({
        text: 'Eliminar cuenta', sub: 'Sus DeCA se conservan', ico: 'trash', color: 'red', small: true,
        onClick: async () => {
          if (!(await confirm(`¿Eliminar la cuenta de ${w.nombre}? No podrá volver a entrar.`, { danger: true, ok: 'Eliminar' }))) return;
          await api(`/api/workers/${wid}`, { method: 'DELETE' });
          toast('Cuenta eliminada', 'ok');
          go(back, { replace: true });
        },
      }),
    ],
    actions: [qrBtn],
  });
}

/** Administración de Preferencias -> Preferencias de un trabajador -> elegir trabajador. */
export async function pickForPrefsView({ cid }) {
  page({ title: 'Preferencias de un trabajador', back: `/empresa/${cid}/preferencias`, body: loading() });
  try {
    const list = await api(`/api/companies/${cid}/workers`);
    page({
      title: 'Preferencias de un trabajador',
      back: `/empresa/${cid}/preferencias`,
      body: list.length
        ? list.map((w) => listItem({ title: w.nombre, sub: w.usuario, ico: 'user', onClick: () => go(`/trabajador/${w.id}/preferencias`) }))
        : panel(h('p.muted.center', 'Esta empresa todavía no tiene trabajadores.')),
    });
  } catch (e) {
    page({ title: 'Preferencias de un trabajador', back: `/empresa/${cid}/preferencias`, body: errorView(e, () => pickForPrefsView({ cid })) });
  }
}

