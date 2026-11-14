// Detalle de un DeCA (datos introducidos + QR) y lista "Últimos 7 Días".

import { h, icon } from '../core/dom.js';
import { api, cache, cachedGet } from '../core/api.js';
import { page, sign, section, notice, loading, errorView, panel, listItem, modal } from '../core/ui.js';
import { go, onLeave } from '../core/router.js';
import { native } from '../core/native.js';
import { isAdmin, setNet, state } from '../core/state.js';
import { qrNode } from '../core/qr.js';
import { ticket, miniTicket } from '../components/ticket.js';
import { fecha, fechaHora, kg, num, diaCorto } from '../core/format.js';
import { showPdf, pdfButtonText, pdfDirect, selectDeca } from '../core/pdf.js';

/** QR a pantalla completa con brillo máximo (para el agente en un control). */
export function showQrFullscreen(d) {
  native.setBrightness(true);
  // En iPhone no se puede subir el brillo, pero sí evitar que la pantalla se apague.
  let lock = null;
  navigator.wakeLock?.request('screen').then((l) => { lock = l; }).catch(() => {});
  const close = () => {
    native.setBrightness(false);
    lock?.release();
    el.remove();
  };
  const el = h('div.qr-full', { onclick: close },
    h('div.cap', `DeCA ${d.numero}`),
    qrNode(d.url, 'qr-big'),
    h('div.muted', 'Toca la pantalla para cerrar'),
  );
  document.body.appendChild(el);
}

function kv(pairs) {
  return h('dl.kv', pairs.filter(([, v]) => v !== undefined && v !== null && v !== '').flatMap(([k, v]) => [h('dt', k), h('dd', String(v))]));
}

function findCached(id) {
  const c = cache.get(`deca.${id}`);
  if (c) return c;
  const lists = [cache.get('recientes', []), cache.get('ultimo', [])].flat().filter(Boolean);
  const d = lists.find((x) => String(x.id) === String(id));
  return d ? { deca: d, versiones_anteriores: [] } : null;
}

export async function view({ id }) {
  page({ title: 'DeCA', back: '/', body: loading() });
  let res;
  try {
    res = await api(`/api/decas/${id}`);
    cache.set(`deca.${id}`, res);
    setNet(true);
  } catch (e) {
    res = e.offline ? findCached(id) : null;
    if (e.offline) setNet(false);
    if (!res) return page({ title: 'DeCA', back: '/', body: errorView(e, () => view({ id })) });
  }
  const d = res.deca;
  const datos = d.datos;
  const canEdit = isAdmin() || (state.user && state.user.id === d.emitido_por_id);

  const qrBtn = sign({ text: 'Enseñar QR', sub: 'Pantalla completa para el control', ico: 'qr', color: 'green', center: true, onClick: () => showQrFullscreen(d) });

  const head = ticket(d, { onClick: () => showQrFullscreen(d), showUser: true });

  const body = [
    head,
    d.url_activa
      ? notice(`Enlace del QR activo hasta el ${fechaHora(d.url_caduca)}.`, 'ok', 'check')
      : notice(d.pdf_disponible ? 'El enlace público del QR ha caducado. El PDF sigue guardado en el servidor.' : 'El PDF se eliminó al superar el año de conservación.', '', 'clock'),
    h('div.grid2.mt',
      d.pdf_disponible ? sign({ text: pdfButtonText(d), ico: 'download', small: true, onClick: () => showPdf(d) }) : null,
      canEdit && d.url_activa ? sign({ text: 'Modificar', sub: 'Mismo QR', ico: 'edit', small: true, color: 'white', onClick: () => go(`/deca/${d.id}/modificar`) }) : null,
    ),
    section('Cargador contractual'),
    panel(kv([['Empresa cargadora', datos.cargador.nombre], ['NIF / CIF', datos.cargador.nif], ['Domicilio', datos.cargador.domicilio]])),
    section('Transportista efectivo'),
    panel(kv([
      ['Empresa transportista', datos.transportista.nombre],
      ['NIF transportista', datos.transportista.nif],
      ['Domicilio', datos.transportista.domicilio],
      ['Nombre transportista', datos.transportista.titular],
    ])),
    section('Servicio'),
    panel(kv([
      ['Fecha de realización', fecha(datos.fecha_transporte)],
      ['Fin previsto', datos.fecha_fin ? fecha(datos.fecha_fin) : ''],
      ['Matrícula camión', datos.matricula_camion],
      ['Matrícula remolque', datos.matricula_remolque],
      ['Conductor', datos.conductor],
      ['Autorización especial', datos.autorizacion_especial],
      ['Observaciones', datos.observaciones],
      ['Emitido por', d.emitido_por],
      ['Creado', fechaHora(d.creado)],
      ['Modificado', d.version > 1 ? fechaHora(d.modificado) : ''],
    ])),
    ...datos.envios.flatMap((e, i) => [
      section(datos.envios.length > 1 ? `Envío ${i + 1}` : 'Envío'),
      panel(kv([['Origen', e.origen], ['Destino', e.destino], ['Mercancía', e.mercancia], ['Peso', kg(e.peso_kg)], ['Volumen', e.volumen_m3 ? `${num(e.volumen_m3)} m³` : '']])),
    ]),
  ];

  if (d.modificaciones && d.modificaciones.length) {
    body.push(section('Modificaciones'));
    for (const m of d.modificaciones) {
      body.push(panel(
        h('h3', `Versión ${m.version} · ${fechaHora(m.fecha)}`),
        h('p', h('strong', 'Motivo: '), m.motivo),
        ...m.cambios.map((c) => h('div', { style: { margin: '6px 0' } },
          h('div.muted', c.campo),
          h('div', h('s', { style: { color: 'var(--rojo)' } }, c.antes || '(vacío)'), ' → ', h('strong', c.despues || '(vacío)')),
        )),
      ));
    }
  }
  if (res.versiones_anteriores && res.versiones_anteriores.length && d.pdf_disponible) {
    body.push(section('Versiones anteriores'));
    for (const v of res.versiones_anteriores) {
      body.push(listItem({ title: `Versión ${v.version}`, sub: fechaHora(v.created_at), ico: 'deca', onClick: () => showPdf(d, v.version) }));
    }
  }

  page({ title: d.numero, back: '/', body, actions: [qrBtn] });
}

export async function recentView() {
  page({ title: 'Últimos 7 Días', back: '/', body: loading() });
  try {
    const list = await cachedGet('/api/decas/recientes', 'recientes');
    setNet(!list.__offline);
    if (!list.length) {
      return page({ title: 'Últimos 7 Días', back: '/', body: panel(h('p.muted.center', 'No hay DeCA de los últimos 7 días.')) });
    }
    const body = [];
    let day = '';
    for (const d of list) {
      const dd = diaCorto(d.creado);
      if (dd !== day) {
        day = dd;
        body.push(section(dd));
      }
      body.push(miniTicket(d, { onClick: () => selectDeca(d), action: pdfDirect() ? 'Abrir PDF' : 'Ver DeCA', showUser: isAdmin() }));
    }
    page({
      title: 'Últimos 7 Días',
      back: '/',
      body: [isAdmin() ? notice('Ves los DeCA de todos los trabajadores.', 'info', 'users') : null, ...body],
    });
  } catch (e) {
    page({ title: 'Últimos 7 Días', back: '/', body: errorView(e, recentView) });
  }
}

