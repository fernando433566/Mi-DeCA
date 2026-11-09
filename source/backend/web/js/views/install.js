// Cómo instalar DeCA en el iPhone: icono en la pantalla de inicio desde Safari.

import { h, icon } from '../core/dom.js';
import { page, panel, notice } from '../core/ui.js';
import { session } from '../core/api.js';
import { isInstalled } from '../core/platform.js';

function step(n, ico, title, text) {
  return panel(h('div.install-step', h('div.install-n', String(n)), icon(ico),
    h('div', h('strong', title), h('div.muted', text))));
}

export function view() {
  const back = session.get() ? '/ajustes' : '/login';
  page({
    title: 'Instalar en el iPhone',
    back,
    body: [
      isInstalled() ? notice('DeCA ya está instalada en este dispositivo.', 'ok', 'check') : null,
      !window.isSecureContext ? notice('Para instalarla hay que entrar por la dirección https:// del servidor.', 'danger', 'lock') : null,
      step(1, 'search', 'Abre esta página en Safari', 'La misma dirección que estás usando ahora.'),
      step(2, 'share', 'Pulsa el botón Compartir', 'El cuadrado con una flecha hacia arriba. Si no lo ves, pulsa antes los tres puntos.'),
      step(3, 'plus', '«Añadir a pantalla de inicio»', 'Baja por la lista hasta encontrarlo. Si aparece «Abrir como app web», déjalo activado. Después pulsa «Añadir».'),
      step(4, 'deca', 'Abre DeCA desde su icono', 'Inicia sesión ahí: la aplicación instalada tiene su propia sesión, distinta de la de Safari.'),
      h('h2.section', 'Diferencias con la app Android'),
      notice('En iPhone no hay widget. Los DeCA creados sin conexión se envían cuando la aplicación está abierta y hay cobertura: ábrela al recuperar la señal y no inicies el transporte hasta tener el QR.', 'info', 'wifioff'),
    ],
  });
}
