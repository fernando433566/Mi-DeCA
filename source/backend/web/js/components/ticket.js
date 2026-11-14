// Billete horizontal del DeCA: datos a la izquierda; a la derecha, el día de emisión
// en la esquina y el QR (generado en el móvil, funciona sin conexión).

import { h, icon } from '../core/dom.js';
import { qrNode } from '../core/qr.js';
import { fecha, fechaHora, hora } from '../core/format.js';

export function ticket(d, { onClick, showUser = false } = {}) {
  const datos = d.datos || {};
  const env = (datos.envios || [])[0] || {};
  const more = (datos.envios || []).length > 1 ? ` (+${datos.envios.length - 1})` : '';
  const badges = [];
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
 * Tarjeta reducida para listas («Últimos 7 Días»): número, hora, ruta, matrícula y un
 * QR pequeño. `action` es el texto bajo el QR (lo que pasa al pulsarla).
 */
export function miniTicket(d, { onClick, action = 'Ver DeCA', showUser = false } = {}) {
  const datos = d.datos || {};
  const env = (datos.envios || [])[0] || {};
  return h('button.mini-ticket', { type: 'button', onclick: onClick, 'aria-label': `${action}: ${d.numero}` },
    h('div.mini-main',
      h('div.mini-top', h('strong', d.numero), h('span.muted', hora(d.creado))),
      h('div.mini-route', env.origen || '—', h('span.arrow', '→'), env.destino || '—'),
      h('div.mini-meta',
        datos.matricula_camion ? h('span.plate-badge', h('span', datos.matricula_camion)) : null,
        showUser ? h('span', d.emitido_por) : null,
        d.version > 1 ? h('span.badge', `v${d.version}`) : null,
        d.url_activa === false ? h('span.badge.off', 'Caducado') : null,
      ),
    ),
    h('div.mini-stub', d.url ? qrNode(d.url, 'mini-qr') : null, h('span.mini-pdf', action)),
  );
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
