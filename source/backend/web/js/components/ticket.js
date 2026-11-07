// Billete horizontal del DeCA: datos a la izquierda; a la derecha, el día de emisión
// en la esquina y el QR (generado en el móvil, funciona sin conexión).

import { h, icon } from '../core/dom.js';
import { qrNode } from '../core/qr.js';
import { fecha, fechaHora } from '../core/format.js';

export function ticket(d, { onClick, showUser = false } = {}) {
  const datos = d.datos || {};
  const env = (datos.envios || [])[0] || {};
  const more = (datos.envios || []).length > 1 ? ` (+${datos.envios.length - 1})` : '';
  const badges = [];
  if (d.viaje_total > 1) badges.push(h('span.badge', `Viaje ${d.viaje_num}/${d.viaje_total}`));
  if (d.version > 1) badges.push(h('span.badge', `Modificado v${d.version}`));
  if (d.url_activa === false) badges.push(h('span.badge.off', 'Enlace caducado'));
  return h('button.ticket', { type: 'button', onclick: onClick, 'aria-label': `DeCA ${d.numero}` },
    h('div.body',
      h('div.kind', 'DeCA · Documento de control'),
      h('div.num', d.numero),
      h('div.route', env.origen || '—', h('span.arrow', '→'), env.destino || '—', more),
      h('div.meta',
        datos.matricula_camion ? h('span.plate-badge', h('span', datos.matricula_camion)) : null,
        h('span', `Transporte: ${fecha(datos.fecha_transporte)}`),
        showUser ? h('span', `· ${d.emitido_por}`) : null,
        ...badges,
      ),
    ),
    h('div.stub',
      h('div.date', { title: fechaHora(d.creado) }, fecha(d.creado)),
      d.url ? qrNode(d.url, 'qr') : null,
    ),
  );
}

/**
 * Billetes de un DeCA con varios viajes (ida y vuelta): flecha a la izquierda para el
 * viaje anterior y a la derecha para el siguiente.
 * @param {Array} viajes  DeCA del grupo, del viaje 1 al último
 * @param {number} index  viaje mostrado (0 = el primero)
 * @param {(i:number) => void} onChange
 */
export function tripCarousel(viajes, index, { onChange, onOpen, showUser = false } = {}) {
  const wrap = h('div');
  const draw = () => {
    const d = viajes[index];
    const prev = h('button.trip-arrow', { type: 'button', 'aria-label': 'Viaje anterior', disabled: index === 0, onclick: () => move(-1) }, icon('back'));
    const next = h('button.trip-arrow', { type: 'button', 'aria-label': 'Viaje siguiente', disabled: index === viajes.length - 1, onclick: () => move(1) }, icon('chev'));
    wrap.replaceChildren(
      h('div.trip-nav', prev, ticket(d, { onClick: () => onOpen && onOpen(d), showUser }), next),
      h('p.trip-label', `Viaje ${index + 1} de ${viajes.length}`),
    );
  };
  const move = (step) => {
    index = Math.max(0, Math.min(viajes.length - 1, index + step));
    draw();
    onChange && onChange(index);
  };
  if (viajes.length === 1) return ticket(viajes[0], { onClick: () => onOpen && onOpen(viajes[0]), showUser });
  draw();
  return wrap;
}

/** Billete provisional de un DeCA que espera conexión (Android). */
export function pendingTicket(p, { onClick } = {}) {
  const datos = p.payload || {};
  const env = (datos.envios || [])[0] || {};
  const state = { pendiente: 'Esperando conexión', enviando: 'Enviando…', error: 'Error al crear', creado: 'Creado' }[p.estado] || p.estado;
  return h('button.ticket', { type: 'button', onclick: onClick },
    h('div.body', { style: { background: 'linear-gradient(90deg, var(--amarillo) 0 10px, transparent 10px), #fffdf6' } },
      h('div.kind', 'Pendiente de crear · sin QR todavía'),
      h('div.num', state),
      h('div.route', env.origen || '—', h('span.arrow', '→'), env.destino || '—'),
      h('div.meta',
        datos.matricula_camion ? h('span.plate-badge', h('span', datos.matricula_camion)) : null,
        h('span.badge.pend', p.estado === 'error' ? 'Revisar' : 'No iniciar viaje'),
      ),
    ),
    h('div.stub', h('div.date', fecha(p.creado)), h('div', { style: { color: 'var(--amarillo-osc)', width: '84px', marginTop: '8px' } }, icon('clock', 'qr'))),
  );
}
