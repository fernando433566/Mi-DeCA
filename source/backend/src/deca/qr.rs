//! Códigos QR: matriz de módulos (para dibujarla en el PDF), SVG y PNG.

use qrcode::{Color, EcLevel, QrCode};

/// Margen blanco alrededor del QR, en módulos (el estándar recomienda 4).
pub const QUIET_ZONE: usize = 4;

/// Matriz cuadrada del QR: `width` módulos de lado, `true` = módulo negro.
pub struct QrMatrix {
    pub width: usize,
    pub modules: Vec<bool>,
}

impl QrMatrix {
    pub fn new(data: &str) -> anyhow::Result<Self> {
        let code = QrCode::with_error_correction_level(data.as_bytes(), EcLevel::M)
            .map_err(|e| anyhow::anyhow!("no se pudo generar el QR: {e}"))?;
        Ok(Self {
            width: code.width(),
            modules: code.to_colors().into_iter().map(|c| c == Color::Dark).collect(),
        })
    }

    pub fn is_dark(&self, x: usize, y: usize) -> bool {
        self.modules[y * self.width + x]
    }

    /// Tramos horizontales de módulos negros `(x, y, longitud)` para dibujar con pocos rectángulos.
    pub fn runs(&self) -> Vec<(usize, usize, usize)> {
        let mut runs = Vec::new();
        for y in 0..self.width {
            let mut x = 0;
            while x < self.width {
                if self.is_dark(x, y) {
                    let start = x;
                    while x < self.width && self.is_dark(x, y) {
                        x += 1;
                    }
                    runs.push((start, y, x - start));
                } else {
                    x += 1;
                }
            }
        }
        runs
    }
}

pub fn svg(data: &str) -> anyhow::Result<String> {
    let m = QrMatrix::new(data)?;
    let size = m.width + QUIET_ZONE * 2;
    let mut path = String::new();
    for (x, y, len) in m.runs() {
        path.push_str(&format!("M{} {}h{}v1h-{}z", x + QUIET_ZONE, y + QUIET_ZONE, len, len));
    }
    Ok(format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {size} {size}" shape-rendering="crispEdges"><rect width="100%" height="100%" fill="#fff"/><path d="{path}" fill="#000"/></svg>"##
    ))
}

/// PNG en escala de grises; `scale` = píxeles por módulo.
pub fn png(data: &str, scale: usize) -> anyhow::Result<Vec<u8>> {
    let m = QrMatrix::new(data)?;
    let scale = scale.clamp(1, 40);
    let side = (m.width + QUIET_ZONE * 2) * scale;
    let mut pixels = vec![255u8; side * side];
    for y in 0..m.width {
        for x in 0..m.width {
            if m.is_dark(x, y) {
                for dy in 0..scale {
                    let row = ((y + QUIET_ZONE) * scale + dy) * side;
                    let col = (x + QUIET_ZONE) * scale;
                    pixels[row + col..row + col + scale].fill(0);
                }
            }
        }
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, side as u32, side as u32);
        enc.set_color(png::ColorType::Grayscale);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header()?;
        writer.write_image_data(&pixels)?;
    }
    Ok(out)
}
