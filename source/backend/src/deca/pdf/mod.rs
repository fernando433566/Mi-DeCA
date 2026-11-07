//! Maquetación del PDF del DeCA.
//!
//! Contenido: cabecera, QR con la URL única, cargador contractual y transportista
//! efectivo diferenciados (apartado Octavo), datos del servicio, envíos (apartado Sexto)
//! y, si las hay, las modificaciones con los valores antiguos tachados y marcados como
//! NO VÁLIDO junto con el motivo (apartado Quinto, método 1).

mod writer;

use chrono::{DateTime, Utc};
use chrono_tz::Europe::Madrid;

use super::model::{fmt_date, fmt_kg, fmt_m3, DecaDatos, Modificacion};
use super::qr::{QrMatrix, QUIET_ZONE};
use writer::{text_width, wrap, Canvas, Font, Metadata, PAGE_H, PAGE_W};

/// Tamaño máximo permitido por la normativa.
pub const MAX_PDF_BYTES: usize = 5 * 1024 * 1024;

pub struct RenderInput<'a> {
    pub numero: &'a str,
    pub url: &'a str,
    pub emitido_por: &'a str,
    pub viaje_num: i64,
    pub viaje_total: i64,
    pub version: i64,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    pub datos: &'a DecaDatos,
    pub modificaciones: &'a [Modificacion],
}

// Colores (estilo señal de tráfico informativa)
const BLUE: (f32, f32, f32) = (0.0, 0.29, 0.6);
const RED: (f32, f32, f32) = (0.75, 0.05, 0.05);
const DARK: (f32, f32, f32) = (0.1, 0.1, 0.1);
const GREY: (f32, f32, f32) = (0.38, 0.38, 0.38);
const LIGHT: (f32, f32, f32) = (0.92, 0.94, 0.97);

const MARGIN: f32 = 40.0;
const BOTTOM: f32 = 55.0;
const CONTENT_W: f32 = PAGE_W - 2.0 * MARGIN;
const LABEL_W: f32 = 120.0;
const SIZE: f32 = 9.5;
const LINE: f32 = 12.5;

fn local(dt: DateTime<Utc>) -> String {
    dt.with_timezone(&Madrid).format("%d/%m/%Y %H:%M:%S").to_string()
}

/// Página actual + cursor vertical; abre páginas nuevas cuando no cabe el contenido.
struct Layout {
    pages: Vec<Canvas>,
    y: f32,
}

impl Layout {
    fn new() -> Self {
        Self { pages: vec![Canvas::default()], y: PAGE_H - MARGIN }
    }

    fn c(&mut self) -> &mut Canvas {
        self.pages.last_mut().expect("siempre hay al menos una página")
    }

    fn ensure(&mut self, height: f32) {
        if self.y - height < BOTTOM {
            self.pages.push(Canvas::default());
            self.y = PAGE_H - MARGIN;
        }
    }

    fn color(&mut self, (r, g, b): (f32, f32, f32)) {
        self.c().fill_color(r, g, b);
    }

    /// Barra de título de sección.
    fn section(&mut self, title: &str) {
        self.ensure(22.0 + LINE * 2.0);
        self.y -= 6.0;
        let y = self.y;
        self.color(BLUE);
        self.c().rect_fill(MARGIN, y - 16.0, CONTENT_W, 16.0);
        self.color((1.0, 1.0, 1.0));
        self.c().text(MARGIN + 6.0, y - 11.8, 9.5, Font::Bold, title);
        self.y -= 22.0;
    }

    /// Fila «Etiqueta: valor» con el valor en varias líneas si hace falta.
    fn row(&mut self, x: f32, width: f32, label: &str, value: &str) {
        let lines = wrap(if value.is_empty() { "—" } else { value }, SIZE, Font::Regular, width - LABEL_W);
        self.ensure(lines.len() as f32 * LINE);
        let y = self.y;
        self.color(GREY);
        self.c().text(x, y - 9.0, 8.0, Font::Bold, label);
        self.color(DARK);
        for (i, l) in lines.iter().enumerate() {
            self.c().text(x + LABEL_W, y - 9.0 - i as f32 * LINE, SIZE, Font::Regular, l);
        }
        self.y -= lines.len() as f32 * LINE + 1.5;
    }

    fn paragraph(&mut self, x: f32, width: f32, text: &str, size: f32, font: Font, color: (f32, f32, f32)) {
        for l in wrap(text, size, font, width) {
            self.ensure(size + 3.0);
            self.color(color);
            let y = self.y;
            self.c().text(x, y - size, size, font, &l);
            self.y -= size + 3.0;
        }
    }
}

fn draw_qr(c: &mut Canvas, url: &str, x: f32, top: f32, size: f32) -> anyhow::Result<()> {
    let m = QrMatrix::new(url)?;
    let modules = (m.width + QUIET_ZONE * 2) as f32;
    let unit = size / modules;
    c.fill_color(1.0, 1.0, 1.0);
    c.rect_fill(x, top - size, size, size);
    c.fill_color(0.0, 0.0, 0.0);
    for (mx, my, len) in m.runs() {
        let px = x + (mx + QUIET_ZONE) as f32 * unit;
        let py = top - (my + QUIET_ZONE + 1) as f32 * unit;
        // Pequeño solape para evitar líneas blancas al rasterizar.
        c.rect_fill(px, py, len as f32 * unit + 0.05, unit + 0.05);
    }
    c.stroke_color(BLUE.0, BLUE.1, BLUE.2);
    c.rect_stroke(x, top - size, size, size, 1.2);
    Ok(())
}

/// Cabecera de la primera página: banda azul, identificación del DeCA y QR.
fn header(l: &mut Layout, inp: &RenderInput) -> anyhow::Result<()> {
    let band_h = 66.0;
    l.color(BLUE);
    l.c().rect_fill(0.0, PAGE_H - band_h, PAGE_W, band_h);
    l.color((1.0, 1.0, 1.0));
    l.c().text(MARGIN, PAGE_H - 40.0, 28.0, Font::Bold, "DeCA");
    l.c().text(MARGIN + 82.0, PAGE_H - 28.0, 12.5, Font::Bold, "Documento electrónico de Control Administrativo");
    l.c().text(
        MARGIN + 82.0,
        PAGE_H - 43.0,
        8.0,
        Font::Regular,
        "Orden FOM/2861/2012 (art. 6) · Resolución de la DGTCF de 5 de junio de 2026",
    );
    l.c().text(MARGIN + 82.0, PAGE_H - 55.0, 8.0, Font::Regular, "Transporte público de mercancías por carretera");

    // QR a la derecha
    let qr_size = 150.0;
    let qr_x = PAGE_W - MARGIN - qr_size;
    let qr_top = PAGE_H - band_h - 12.0;
    draw_qr(l.c(), inp.url, qr_x, qr_top, qr_size)?;
    let caption = "Escanee para descargar este DeCA";
    l.color(DARK);
    let cw = text_width(caption, 7.5, Font::Bold);
    l.c().text(qr_x + (qr_size - cw) / 2.0, qr_top - qr_size - 10.0, 7.5, Font::Bold, caption);
    let mut uy = qr_top - qr_size - 19.0;
    l.color(GREY);
    for line in wrap(inp.url, 6.0, Font::Regular, qr_size) {
        l.c().text(qr_x, uy, 6.0, Font::Regular, &line);
        uy -= 7.5;
    }

    // Identificación a la izquierda
    let left_w = CONTENT_W - qr_size - 16.0;
    l.y = qr_top;
    l.color(DARK);
    let y = l.y;
    l.c().text(MARGIN, y - 16.0, 16.0, Font::Bold, &format!("Nº {}", inp.numero));
    l.y -= 26.0;
    if inp.viaje_total > 1 {
        l.color(RED);
        let y = l.y;
        l.c().text(MARGIN, y - 12.0, 12.0, Font::Bold, &format!("Viaje {} de {}", inp.viaje_num, inp.viaje_total));
        l.y -= 20.0;
    }
    l.row(MARGIN, left_w, "Fecha de realización", &fmt_date(inp.datos.fecha_transporte));
    if let Some(fin) = inp.datos.fecha_fin {
        l.row(MARGIN, left_w, "Fin previsto", &fmt_date(fin));
    }
    l.row(MARGIN, left_w, "Creado", &format!("{} (hora peninsular)", local(inp.created)));
    if inp.version > 1 {
        l.row(MARGIN, left_w, "Última modificación", &format!("{} (versión {})", local(inp.modified), inp.version));
    }
    l.row(MARGIN, left_w, "Emitido por", inp.emitido_por);
    l.row(MARGIN, left_w, "Matrícula camión", &inp.datos.matricula_camion);
    l.row(
        MARGIN,
        left_w,
        "Matrícula remolque",
        inp.datos.matricula_remolque.as_deref().unwrap_or(""),
    );
    l.y = l.y.min(uy - 4.0);
    Ok(())
}

/// Dos recuadros en paralelo: cargador contractual y transportista efectivo.
fn parties(l: &mut Layout, d: &DecaDatos) {
    let gap = 12.0;
    let box_w = (CONTENT_W - gap) / 2.0;
    let inner = box_w - 16.0;
    let w = |text: &str, font| wrap(text, SIZE, font, inner);
    let left = vec![
        ("Empresa cargadora", w(&d.cargador.nombre, Font::Bold)),
        ("NIF / CIF", w(&d.cargador.nif, Font::Regular)),
        ("Domicilio", w(&d.cargador.domicilio, Font::Regular)),
    ];
    let t = &d.transportista;
    let mut right = vec![
        ("Empresa transportista", w(&t.nombre, Font::Bold)),
        ("NIF transportista", w(&t.nif, Font::Regular)),
        ("Domicilio", w(&t.domicilio, Font::Regular)),
    ];
    if let Some(titular) = &t.titular {
        right.push(("Nombre transportista (titular de los vehículos)", w(titular, Font::Regular)));
    }
    let height_of = |e: &Vec<(&str, Vec<String>)>| -> f32 {
        26.0 + e.iter().map(|(_, lines)| 11.0 + lines.len() as f32 * LINE + 3.0).sum::<f32>()
    };
    let h = height_of(&left).max(height_of(&right)) + 6.0;
    l.ensure(h + 8.0);
    l.y -= 6.0;
    let top = l.y;
    for (i, (title, e)) in [("CARGADOR CONTRACTUAL", &left), ("TRANSPORTISTA EFECTIVO", &right)]
        .into_iter()
        .enumerate()
    {
        let x = MARGIN + i as f32 * (box_w + gap);
        let c = l.c();
        c.fill_color(LIGHT.0, LIGHT.1, LIGHT.2);
        c.rect_fill(x, top - h, box_w, h);
        c.fill_color(BLUE.0, BLUE.1, BLUE.2);
        c.rect_fill(x, top - 18.0, box_w, 18.0);
        c.stroke_color(BLUE.0, BLUE.1, BLUE.2);
        c.rect_stroke(x, top - h, box_w, h, 1.0);
        c.fill_color(1.0, 1.0, 1.0);
        c.text(x + 8.0, top - 12.8, 9.5, Font::Bold, title);
        let mut y = top - 26.0;
        for (j, (label, lines)) in e.iter().enumerate() {
            c.fill_color(GREY.0, GREY.1, GREY.2);
            c.text(x + 8.0, y - 8.0, 7.5, Font::Bold, label);
            y -= 11.0;
            c.fill_color(DARK.0, DARK.1, DARK.2);
            for line in lines {
                let font = if j == 0 { Font::Bold } else { Font::Regular };
                c.text(x + 8.0, y - 9.5, SIZE, font, line);
                y -= LINE;
            }
            y -= 3.0;
        }
    }
    l.y = top - h - 4.0;
}

fn envios(l: &mut Layout, d: &DecaDatos) {
    let total = d.envios.len();
    l.section(if total > 1 { "ENVÍOS AGRUPADOS" } else { "ENVÍO" });
    for (i, e) in d.envios.iter().enumerate() {
        l.ensure(LINE * 6.0);
        if total > 1 {
            l.color(BLUE);
            let y = l.y;
            l.c().text(MARGIN, y - 10.0, 10.0, Font::Bold, &format!("Envío {} de {}", i + 1, total));
            l.y -= 15.0;
        }
        l.row(MARGIN, CONTENT_W, "Origen", &e.origen);
        l.row(MARGIN, CONTENT_W, "Destino", &e.destino);
        l.row(MARGIN, CONTENT_W, "Naturaleza mercancía", &e.mercancia);
        l.row(MARGIN, CONTENT_W, "Peso", &fmt_kg(e.peso_kg));
        if let Some(v) = e.volumen_m3 {
            l.row(MARGIN, CONTENT_W, "Volumen", &fmt_m3(v));
        }
        if i + 1 < total {
            l.c().stroke_color(0.75, 0.75, 0.75);
            let y = l.y - 3.0;
            l.c().line(MARGIN, y, MARGIN + CONTENT_W, y, 0.5);
            l.y -= 7.0;
        }
    }
}

fn service(l: &mut Layout, d: &DecaDatos) {
    l.section("DATOS DEL SERVICIO");
    l.row(MARGIN, CONTENT_W, "Conductor", d.conductor.as_deref().unwrap_or(""));
    l.row(MARGIN, CONTENT_W, "Fecha de realización", &fmt_date(d.fecha_transporte));
    l.row(MARGIN, CONTENT_W, "Matrícula camión", &d.matricula_camion);
    l.row(MARGIN, CONTENT_W, "Matrícula remolque", d.matricula_remolque.as_deref().unwrap_or(""));
    l.row(
        MARGIN,
        CONTENT_W,
        "Autorización especial",
        d.autorizacion_especial.as_deref().unwrap_or("No procede"),
    );
    if let Some(o) = &d.observaciones {
        l.row(MARGIN, CONTENT_W, "Observaciones", o);
    }
}

fn modifications(l: &mut Layout, mods: &[Modificacion]) {
    if mods.is_empty() {
        return;
    }
    l.section("MODIFICACIONES DURANTE EL SERVICIO");
    l.paragraph(
        MARGIN,
        CONTENT_W,
        "Los datos tachados han dejado de ser válidos. Los datos vigentes son los que figuran en los apartados anteriores.",
        8.0,
        Font::Regular,
        GREY,
    );
    for m in mods {
        l.ensure(LINE * 4.0);
        l.y -= 4.0;
        l.paragraph(
            MARGIN,
            CONTENT_W,
            &format!("Modificación (versión {}) · {} · por {}", m.version, local(m.fecha), m.usuario),
            9.5,
            Font::Bold,
            BLUE,
        );
        l.row(MARGIN, CONTENT_W, "Motivo", &m.motivo);
        for c in &m.cambios {
            // Valor anterior: en rojo, tachado y marcado NO VÁLIDO
            let old = if c.antes.is_empty() { "(vacío)".to_string() } else { c.antes.clone() };
            let width = CONTENT_W - LABEL_W - 70.0;
            let old_lines = wrap(&old, SIZE, Font::Regular, width);
            let new_lines = wrap(
                if c.despues.is_empty() { "(vacío)" } else { &c.despues },
                SIZE,
                Font::Regular,
                width,
            );
            l.ensure((old_lines.len() + new_lines.len()) as f32 * LINE + 4.0);
            let y = l.y;
            l.color(GREY);
            for (i, line) in wrap(&c.campo, 8.0, Font::Bold, LABEL_W - 6.0).iter().enumerate() {
                l.c().text(MARGIN, y - 9.0 - i as f32 * 10.0, 8.0, Font::Bold, line);
            }
            let x = MARGIN + LABEL_W;
            for (i, line) in old_lines.iter().enumerate() {
                let ly = y - 9.0 - i as f32 * LINE;
                l.color(RED);
                l.c().text(x, ly, SIZE, Font::Regular, line);
                let w = text_width(line, SIZE, Font::Regular);
                l.c().stroke_color(RED.0, RED.1, RED.2);
                l.c().line(x, ly + 3.2, x + w, ly + 3.2, 0.9);
            }
            l.color(RED);
            l.c().text(MARGIN + CONTENT_W - 62.0, y - 9.0, 8.0, Font::Bold, "NO VÁLIDO");
            let ny = y - old_lines.len() as f32 * LINE;
            l.color(DARK);
            for (i, line) in new_lines.iter().enumerate() {
                let prefix = if i == 0 { "» " } else { "   " };
                l.c().text(x, ny - 9.0 - i as f32 * LINE, SIZE, Font::Bold, &format!("{prefix}{line}"));
            }
            l.y -= (old_lines.len() + new_lines.len()) as f32 * LINE + 4.0;
        }
    }
}

fn footers(pages: &mut [Canvas], inp: &RenderInput) {
    let total = pages.len();
    for (i, c) in pages.iter_mut().enumerate() {
        c.stroke_color(BLUE.0, BLUE.1, BLUE.2);
        c.line(MARGIN, 40.0, PAGE_W - MARGIN, 40.0, 0.8);
        c.fill_color(GREY.0, GREY.1, GREY.2);
        c.text(
            MARGIN,
            28.0,
            7.0,
            Font::Regular,
            &format!(
                "DeCA {} · Documento generado de forma nativa digital · Creado {} · Modificado {}",
                inp.numero,
                local(inp.created),
                local(inp.modified)
            ),
        );
        let p = format!("Página {} de {}", i + 1, total);
        let w = text_width(&p, 7.5, Font::Bold);
        c.text(PAGE_W - MARGIN - w, 28.0, 7.5, Font::Bold, &p);
    }
}

/// Genera el PDF completo del DeCA.
pub fn render(inp: &RenderInput) -> anyhow::Result<Vec<u8>> {
    let mut l = Layout::new();
    header(&mut l, inp)?;
    parties(&mut l, inp.datos);
    service(&mut l, inp.datos);
    envios(&mut l, inp.datos);
    modifications(&mut l, inp.modificaciones);
    footers(&mut l.pages, inp);

    let d = inp.datos;
    let meta = Metadata {
        title: format!("DeCA {}", inp.numero),
        author: format!("{} · {}", d.transportista.nombre, inp.emitido_por),
        subject: format!(
            "Documento electrónico de Control Administrativo · Cargador: {} ({}) · Transportista: {} ({})",
            d.cargador.nombre, d.cargador.nif, d.transportista.nombre, d.transportista.nif
        ),
        keywords: format!("DeCA, {}, {}", inp.numero, d.matricula_camion),
        created: inp.created,
        modified: inp.modified,
    };
    let bytes = writer::build(l.pages, &meta);
    if bytes.len() > MAX_PDF_BYTES {
        anyhow::bail!("el PDF supera los 5 MB permitidos");
    }
    Ok(bytes)
}
