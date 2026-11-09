// Dónde se está ejecutando la aplicación.

import { isAndroid } from './native.js';

/** iPhone o iPad (los iPad modernos se presentan como Mac con pantalla táctil). */
export const isIOS = /iPad|iPhone|iPod/.test(navigator.userAgent)
  || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);

/** Abierta desde el icono de la pantalla de inicio, no desde una pestaña del navegador. */
export function isInstalled() {
  return navigator.standalone === true || window.matchMedia('(display-mode: standalone)').matches;
}

/** Guarda la aplicación en el dispositivo para que abra sin conexión (no en la app Android). */
export function registerOfflineApp() {
  if (isAndroid || !('serviceWorker' in navigator)) return;
  navigator.serviceWorker.register('sw.js').catch(() => { /* http sin cifrar: sigue funcionando con conexión */ });
  if (isInstalled()) navigator.storage?.persist?.().catch(() => {});
}
