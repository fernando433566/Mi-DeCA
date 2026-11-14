// PDF de un DeCA: verlo en la app.
//
// - Android: el PDF se descarga en los datos privados de la app (no en «Descargas») y
//   se abre con el visor de PDF del móvil. Si ya está descargado se abre sin conexión.
//   La app lo borra sola cuando el DeCA sale de «Últimos 7 Días» (ver PdfStore.java).
// - Navegador / iPhone: se pide con la sesión y se abre en una pestaña nueva.

import { api } from './api.js';
import { isAndroid, native } from './native.js';
import { toast } from './ui.js';
import { go } from './router.js';
import { state } from './state.js';

/** Nombre del PDF (el mismo que tiene en el servidor). Las versiones anteriores llevan `-vN`. */
export function pdfName(d, version = d.version) {
  const name = d.archivo || `${d.numero}.pdf`;
  return version === d.version ? name : name.replace(/\.pdf$/i, `-v${version}.pdf`);
}

/** Lo que necesita la app Android para guardar y encontrar el PDF. */
function nativeInfo(d, version) {
  return { id: d.id, version, creado: d.creado, archivo: pdfName(d, version) };
}

/** ¿Está ya descargado en el móvil? (Solo Android; en el navegador siempre `false`.) */
export function isPdfSaved(d, version = d.version) {
  return isAndroid && native.pdfSaved(nativeInfo(d, version));
}

/** Texto del botón según si el PDF ya está en el móvil. */
export function pdfButtonText(d, version = d.version) {
  if (!isAndroid) return 'Ver PDF';
  return isPdfSaved(d, version) ? 'Abrir PDF' : 'Descargar PDF';
}

/**
 * Abre el PDF de un DeCA (versión vigente o una anterior), descargándolo si hace falta.
 * Debe llamarse directamente desde una pulsación: el navegador solo deja abrir la
 * pestaña nueva en ese momento.
 */
export async function openPdf(d, version = d.version) {
  if (!d.pdf_disponible) throw new Error('El PDF se eliminó al superar el año de conservación.');
  if (isAndroid) return native.openPdf(nativeInfo(d, version));
  const path = version === d.version ? d.pdf : `${d.pdf}?version=${version}`;
  const tab = window.open('', '_blank');
  try {
    const res = await api(path, { raw: true, timeout: 120000 });
    const url = URL.createObjectURL(new Blob([await res.blob()], { type: 'application/pdf' }));
    setTimeout(() => URL.revokeObjectURL(url), 10 * 60 * 1000);
    if (tab) {
      tab.location.href = url;
    } else {
      // Pestañas bloqueadas: se guarda como descarga normal.
      const a = Object.assign(document.createElement('a'), { href: url, download: pdfName(d, version) });
      document.body.appendChild(a);
      a.click();
      a.remove();
    }
  } catch (e) {
    tab?.close();
    throw e;
  }
}

/** Abre el PDF avisando mientras se descarga y mostrando los errores. */
export function showPdf(d, version = d.version) {
  const downloading = !isPdfSaved(d, version);
  const opening = openPdf(d, version);
  if (downloading) toast('Descargando el PDF…');
  return opening.catch((e) => toast(e.message, 'danger'));
}

/** «Mostrar PDF directamente al seleccionar un DeCA» (Ajustes → Preferencias → Preferencias visuales). */
export function pdfDirect() {
  return !!state.user?.preferencias_visuales?.pdf_directo;
}

/**
 * Al pulsar un billete («DeCA más reciente» y «Últimos 7 Días»): según la preferencia,
 * su PDF o la página del DeCA.
 */
export function selectDeca(d) {
  return pdfDirect() ? showPdf(d) : go(`/deca/${d.id}`);
}
