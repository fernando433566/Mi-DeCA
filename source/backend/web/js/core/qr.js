// QR generado en el propio dispositivo (funciona sin conexión).
// Usa la librería qrcode-generator (MIT) cargada en index.html como `window.qrcode`.

import { trustedSvg } from './dom.js';

export function qrSvgText(text) {
  const qr = window.qrcode(0, 'M');
  qr.addData(text, 'Byte');
  qr.make();
  const n = qr.getModuleCount();
  const q = 4; // margen blanco
  let d = '';
  for (let y = 0; y < n; y++) {
    let x = 0;
    while (x < n) {
      if (qr.isDark(y, x)) {
        const s = x;
        while (x < n && qr.isDark(y, x)) x++;
        d += `M${s + q} ${y + q}h${x - s}v1h-${x - s}z`;
      } else x++;
    }
  }
  const size = n + q * 2;
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${size} ${size}" shape-rendering="crispEdges"><rect width="100%" height="100%" fill="#fff"/><path d="${d}" fill="#000"/></svg>`;
}

export function qrNode(text, cls = 'qr') {
  return trustedSvg(qrSvgText(text), cls);
}
