//! Escritor PDF 1.7 mínimo, sin dependencias externas.
//!
//! El DeCA se genera de forma nativa digital (texto real, no imágenes escaneadas):
//! fuentes estándar Helvetica con codificación WinAnsi (admite á, é, ñ, ü, €...),
//! QR dibujado como vectores y metadatos `CreationDate` / `ModDate` en el diccionario Info.

use chrono::{DateTime, Utc};
use chrono_tz::Europe::Madrid;

pub const PAGE_W: f32 = 595.28; // A4
pub const PAGE_H: f32 = 841.89;

/// Contenido gráfico de una página.
#[derive(Default)]
pub struct Canvas {
    ops: Vec<u8>,
}

#[derive(Clone, Copy)]
pub enum Font {
    Regular,
    Bold,
}

impl Font {
    fn name(self) -> &'static str {
        match self {
            Font::Regular => "F1",
            Font::Bold => "F2",
        }
    }
}

impl Canvas {
    fn push(&mut self, s: &str) {
        self.ops.extend_from_slice(s.as_bytes());
    }

    pub fn fill_color(&mut self, r: f32, g: f32, b: f32) {
        self.push(&format!("{r:.3} {g:.3} {b:.3} rg\n"));
    }

    pub fn stroke_color(&mut self, r: f32, g: f32, b: f32) {
        self.push(&format!("{r:.3} {g:.3} {b:.3} RG\n"));
    }

    pub fn rect_fill(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.push(&format!("{x:.2} {y:.2} {w:.2} {h:.2} re f\n"));
    }

    pub fn rect_stroke(&mut self, x: f32, y: f32, w: f32, h: f32, width: f32) {
        self.push(&format!("{width:.2} w {x:.2} {y:.2} {w:.2} {h:.2} re S\n"));
    }

    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32) {
        self.push(&format!("{width:.2} w {x1:.2} {y1:.2} m {x2:.2} {y2:.2} l S\n"));
    }

    /// Escribe texto en (x, y) = línea base. El color es el de relleno actual.
    pub fn text(&mut self, x: f32, y: f32, size: f32, font: Font, text: &str) {
        self.push(&format!("BT /{} {size:.1} Tf {x:.2} {y:.2} Td (", font.name()));
        self.ops.extend(encode_winansi_escaped(text));
        self.push(") Tj ET\n");
    }
}

/// Convierte texto Unicode a WinAnsi (CP1252) escapando `\`, `(` y `)`.
fn encode_winansi_escaped(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        let b = match c {
            '\\' | '(' | ')' => {
                out.push(b'\\');
                c as u8
            }
            '\n' | '\r' | '\t' => b' ',
            ' '..='~' => c as u8,
            '\u{A0}'..='\u{FF}' => c as u32 as u8,
            '€' => 0x80,
            '…' => 0x85,
            '‘' => 0x91,
            '’' => 0x92,
            '“' => 0x93,
            '”' => 0x94,
            '•' => 0x95,
            '–' => 0x96,
            '—' => 0x97,
            _ => b'?',
        };
        out.push(b);
    }
    out
}

/// Anchura aproximada del texto en puntos (métricas AFM de Helvetica).
pub fn text_width(text: &str, size: f32, font: Font) -> f32 {
    const W: [u16; 95] = [
        278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
        556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
        611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
        667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
        222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
    ];
    let units: u32 = text
        .chars()
        .map(|c| match c {
            ' '..='~' => W[(c as u32 - 32) as usize] as u32,
            _ => 600,
        })
        .sum();
    let factor = match font {
        Font::Regular => 1.0,
        Font::Bold => 1.07,
    };
    units as f32 * size / 1000.0 * factor
}

/// Parte el texto en líneas que caben en `max_w` puntos.
pub fn wrap(text: &str, size: f32, font: Font, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if text_width(&candidate, size, font) <= max_w {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // Palabra más larga que la línea (p. ej. una URL): se corta por caracteres.
            let mut chunk = String::new();
            for ch in word.chars() {
                chunk.push(ch);
                if text_width(&chunk, size, font) > max_w {
                    chunk.pop();
                    lines.push(std::mem::take(&mut chunk));
                    chunk.push(ch);
                }
            }
            line = chunk;
        }
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Metadatos del documento (diccionario Info).
pub struct Metadata {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
}

/// Fecha PDF: `D:AAAAMMDDHHmmSS+01'00'` en hora peninsular española.
fn pdf_date(dt: DateTime<Utc>) -> String {
    let local = dt.with_timezone(&Madrid);
    let offset = local.format("%z").to_string(); // +0100
    format!("D:{}{}'{}'", local.format("%Y%m%d%H%M%S"), &offset[..3], &offset[3..])
}

/// Cadena de texto PDF en UTF-16BE (hexadecimal), válida para cualquier carácter.
fn pdf_utf16(s: &str) -> String {
    let mut hex = String::from("<FEFF");
    for unit in s.encode_utf16() {
        hex.push_str(&format!("{unit:04X}"));
    }
    hex.push('>');
    hex
}

/// Ensambla el fichero PDF completo a partir de las páginas.
pub fn build(pages: Vec<Canvas>, meta: &Metadata) -> Vec<u8> {
    // Numeración de objetos:
    // 1 Catalog, 2 Pages, 3 Info, 4 Helvetica, 5 Helvetica-Bold, luego (Page, Contents) por página.
    let n = pages.len();
    let page_ids: Vec<usize> = (0..n).map(|i| 6 + i * 2).collect();
    let mut objects: Vec<Vec<u8>> = Vec::new();

    objects.push(b"<< /Type /Catalog /Pages 2 0 R /Lang (es-ES) >>".to_vec());
    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
    objects.push(format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")).into_bytes());
    objects.push(
        format!(
            "<< /Title {} /Author {} /Subject {} /Keywords {} /Creator {} /Producer {} /CreationDate ({}) /ModDate ({}) >>",
            pdf_utf16(&meta.title),
            pdf_utf16(&meta.author),
            pdf_utf16(&meta.subject),
            pdf_utf16(&meta.keywords),
            pdf_utf16("DeCA Server"),
            pdf_utf16(concat!("DeCA Server ", env!("CARGO_PKG_VERSION"))),
            pdf_date(meta.created),
            pdf_date(meta.modified),
        )
        .into_bytes(),
    );
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec());
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".to_vec());

    for (i, page) in pages.into_iter().enumerate() {
        let contents_id = page_ids[i] + 1;
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] \
                 /Resources << /Font << /F1 4 0 R /F2 5 0 R >> >> /Contents {contents_id} 0 R >>"
            )
            .into_bytes(),
        );
        let mut stream = format!("<< /Length {} >>\nstream\n", page.ops.len()).into_bytes();
        stream.extend_from_slice(&page.ops);
        stream.extend_from_slice(b"\nendstream");
        objects.push(stream);
    }

    let mut out = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_pos = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for off in offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winansi_accents_and_escapes() {
        assert_eq!(encode_winansi_escaped("Ñá(€)"), vec![0xD1, 0xE1, b'\\', b'(', 0x80, b'\\', b')']);
    }

    #[test]
    fn wrap_long_words() {
        let lines = wrap("https://ejemplo.es/una/ruta/muy/larga/que/no/cabe", 10.0, Font::Regular, 60.0);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| text_width(l, 10.0, Font::Regular) <= 60.0));
    }
}
