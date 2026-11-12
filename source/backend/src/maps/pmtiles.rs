//! Formato PMTiles v3 (https://github.com/protomaps/PMTiles/blob/main/spec/v3/spec.md):
//! un único fichero con todas las teselas vectoriales de una zona, legible por
//! MapLibre (Android / iOS / Flutter) sin conexión.
//!
//! Estructura: cabecera (127 bytes) | directorio raíz | metadatos | directorios hoja | teselas

use std::io::{Read, Write};

use anyhow::{bail, ensure, Context};

pub const HEADER_LEN: usize = 127;
/// El directorio raíz debe caber, junto con la cabecera, en los primeros 16 KiB.
pub const ROOT_MAX: usize = 16_384 - HEADER_LEN;

pub const COMPRESSION_NONE: u8 = 1;
pub const COMPRESSION_GZIP: u8 = 2;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Header {
    pub root_offset: u64,
    pub root_length: u64,
    pub metadata_offset: u64,
    pub metadata_length: u64,
    pub leaf_offset: u64,
    pub leaf_length: u64,
    pub data_offset: u64,
    pub data_length: u64,
    pub addressed_tiles: u64,
    pub tile_entries: u64,
    pub tile_contents: u64,
    pub clustered: bool,
    pub internal_compression: u8,
    pub tile_compression: u8,
    pub tile_type: u8,
    pub min_zoom: u8,
    pub max_zoom: u8,
    pub min_lon_e7: i32,
    pub min_lat_e7: i32,
    pub max_lon_e7: i32,
    pub max_lat_e7: i32,
    pub center_zoom: u8,
    pub center_lon_e7: i32,
    pub center_lat_e7: i32,
}

impl Header {
    pub fn parse(b: &[u8]) -> anyhow::Result<Self> {
        ensure!(b.len() >= HEADER_LEN, "cabecera PMTiles incompleta");
        ensure!(&b[0..7] == b"PMTiles", "el fichero no es PMTiles");
        ensure!(b[7] == 3, "versión de PMTiles no soportada ({})", b[7]);
        let u = |o: usize| u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        let i = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        Ok(Self {
            root_offset: u(8),
            root_length: u(16),
            metadata_offset: u(24),
            metadata_length: u(32),
            leaf_offset: u(40),
            leaf_length: u(48),
            data_offset: u(56),
            data_length: u(64),
            addressed_tiles: u(72),
            tile_entries: u(80),
            tile_contents: u(88),
            clustered: b[96] == 1,
            internal_compression: b[97],
            tile_compression: b[98],
            tile_type: b[99],
            min_zoom: b[100],
            max_zoom: b[101],
            min_lon_e7: i(102),
            min_lat_e7: i(106),
            max_lon_e7: i(110),
            max_lat_e7: i(114),
            center_zoom: b[118],
            center_lon_e7: i(119),
            center_lat_e7: i(123),
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(HEADER_LEN);
        b.extend_from_slice(b"PMTiles");
        b.push(3);
        for v in [
            self.root_offset,
            self.root_length,
            self.metadata_offset,
            self.metadata_length,
            self.leaf_offset,
            self.leaf_length,
            self.data_offset,
            self.data_length,
            self.addressed_tiles,
            self.tile_entries,
            self.tile_contents,
        ] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&[
            self.clustered as u8,
            self.internal_compression,
            self.tile_compression,
            self.tile_type,
            self.min_zoom,
            self.max_zoom,
        ]);
        for v in [self.min_lon_e7, self.min_lat_e7, self.max_lon_e7, self.max_lat_e7] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(self.center_zoom);
        b.extend_from_slice(&self.center_lon_e7.to_le_bytes());
        b.extend_from_slice(&self.center_lat_e7.to_le_bytes());
        debug_assert_eq!(b.len(), HEADER_LEN);
        b
    }
}

/// Entrada de directorio. `run_length == 0` significa que apunta a un directorio hoja.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub tile_id: u64,
    pub offset: u64,
    pub length: u32,
    pub run_length: u32,
}

fn write_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn read_varint(b: &[u8], pos: &mut usize) -> anyhow::Result<u64> {
    let mut v = 0u64;
    let mut shift = 0;
    loop {
        let byte = *b.get(*pos).context("directorio PMTiles truncado")?;
        *pos += 1;
        ensure!(shift < 63 || byte & 0xfe == 0, "varint desbordado");
        v |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok(v);
        }
        shift += 7;
        ensure!(shift < 64, "varint demasiado largo");
    }
}

pub fn decompress(data: &[u8], compression: u8) -> anyhow::Result<Vec<u8>> {
    const MAX_EXPANDED: usize = 32 * 1024 * 1024;
    ensure!(data.len() <= MAX_EXPANDED, "bloque PMTiles demasiado grande");
    match compression {
        COMPRESSION_NONE => Ok(data.to_vec()),
        COMPRESSION_GZIP => {
            let mut out = Vec::new();
            flate2::read::GzDecoder::new(data).take(MAX_EXPANDED as u64 + 1).read_to_end(&mut out)?;
            ensure!(out.len() <= MAX_EXPANDED, "bloque PMTiles descomprimido demasiado grande");
            Ok(out)
        }
        c => bail!("compresión interna no soportada ({c})"),
    }
}

pub fn gzip(data: &[u8]) -> Vec<u8> {
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(data).expect("escribir en memoria no falla");
    enc.finish().expect("escribir en memoria no falla")
}

/// Directorio sin comprimir -> entradas.
pub fn decode_directory(b: &[u8]) -> anyhow::Result<Vec<Entry>> {
    let mut pos = 0;
    let n = read_varint(b, &mut pos)? as usize;
    ensure!(n <= 1_000_000 && n <= b.len() / 4, "directorio demasiado grande o truncado");
    let mut entries = vec![Entry { tile_id: 0, offset: 0, length: 0, run_length: 0 }; n];
    let mut last = 0u64;
    for e in entries.iter_mut() {
        last = last.checked_add(read_varint(b, &mut pos)?).context("id PMTiles desbordado")?;
        e.tile_id = last;
    }
    for e in entries.iter_mut() {
        e.run_length = u32::try_from(read_varint(b, &mut pos)?).context("run PMTiles fuera de rango")?;
    }
    for e in entries.iter_mut() {
        e.length = u32::try_from(read_varint(b, &mut pos)?).context("longitud PMTiles fuera de rango")?;
    }
    for i in 0..n {
        let v = read_varint(b, &mut pos)?;
        entries[i].offset = if v == 0 && i > 0 {
            entries[i - 1].offset.checked_add(entries[i - 1].length as u64).context("offset PMTiles desbordado")?
        } else {
            v.checked_sub(1).context("desplazamiento no válido en directorio")?
        };
    }
    Ok(entries)
}

/// Entradas -> directorio sin comprimir.
pub fn encode_directory(entries: &[Entry]) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, entries.len() as u64);
    let mut last = 0;
    for e in entries {
        write_varint(&mut out, e.tile_id - last);
        last = e.tile_id;
    }
    for e in entries {
        write_varint(&mut out, e.run_length as u64);
    }
    for e in entries {
        write_varint(&mut out, e.length as u64);
    }
    for (i, e) in entries.iter().enumerate() {
        if i > 0 && Some(e.offset) == entries[i - 1].offset.checked_add(entries[i - 1].length as u64) {
            write_varint(&mut out, 0);
        } else {
            write_varint(&mut out, e.offset + 1);
        }
    }
    out
}

/// Identificador de tesela: posición en la curva de Hilbert, acumulando los niveles anteriores.
pub fn zxy_to_id(z: u8, x: u32, y: u32) -> u64 {
    let acc = ((1u64 << (2 * z as u64)) - 1) / 3;
    let (mut x, mut y) = (x as u64, y as u64);
    let mut d = 0u64;
    let mut s = (1u64 << z) >> 1;
    while s > 0 {
        let rx = u64::from(x & s > 0);
        let ry = u64::from(y & s > 0);
        d += s * s * ((3 * rx) ^ ry);
        // Solo importan los bits inferiores a `s` en las siguientes iteraciones.
        x &= s - 1;
        y &= s - 1;
        if ry == 0 {
            if rx == 1 {
                x = s - 1 - x;
                y = s - 1 - y;
            }
            std::mem::swap(&mut x, &mut y);
        }
        s >>= 1;
    }
    acc + d
}

/// Construye el directorio raíz (y hojas si no cabe en 16 KiB).
/// Devuelve (raíz comprimida, hojas comprimidas concatenadas).
pub fn build_directories(entries: &[Entry]) -> (Vec<u8>, Vec<u8>) {
    let root = gzip(&encode_directory(entries));
    if root.len() <= ROOT_MAX {
        return (root, Vec::new());
    }
    let mut leaf_size = 4096usize;
    loop {
        let mut leaves = Vec::new();
        let mut root_entries = Vec::new();
        for chunk in entries.chunks(leaf_size) {
            let leaf = gzip(&encode_directory(chunk));
            root_entries.push(Entry {
                tile_id: chunk[0].tile_id,
                offset: leaves.len() as u64,
                length: leaf.len() as u32,
                run_length: 0,
            });
            leaves.extend_from_slice(&leaf);
        }
        let root = gzip(&encode_directory(&root_entries));
        if root.len() <= ROOT_MAX {
            return (root, leaves);
        }
        leaf_size *= 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_ids_from_spec() {
        assert_eq!(zxy_to_id(0, 0, 0), 0);
        assert_eq!(zxy_to_id(1, 0, 0), 1);
        assert_eq!(zxy_to_id(1, 0, 1), 2);
        assert_eq!(zxy_to_id(1, 1, 1), 3);
        assert_eq!(zxy_to_id(1, 1, 0), 4);
        assert_eq!(zxy_to_id(2, 0, 0), 5);
        assert_eq!(zxy_to_id(20, 0, 0), 366503875925);
    }

    #[test]
    fn hilbert_is_a_bijection_per_zoom() {
        for z in 0..6u8 {
            let n = 1u32 << z;
            let mut ids: Vec<u64> = (0..n).flat_map(|x| (0..n).map(move |y| zxy_to_id(z, x, y))).collect();
            ids.sort();
            let base = ((1u64 << (2 * z)) - 1) / 3;
            assert_eq!(ids, (base..base + (n as u64 * n as u64)).collect::<Vec<_>>());
        }
    }

    #[test]
    fn directory_roundtrip() {
        let entries = vec![
            Entry { tile_id: 0, offset: 0, length: 10, run_length: 1 },
            Entry { tile_id: 5, offset: 10, length: 20, run_length: 3 },
            Entry { tile_id: 9, offset: 0, length: 10, run_length: 1 },
            Entry { tile_id: 1000, offset: 30, length: 7, run_length: 0 },
        ];
        assert_eq!(decode_directory(&encode_directory(&entries)).unwrap(), entries);
    }

    #[test]
    fn header_roundtrip() {
        let h = Header { root_offset: 127, max_zoom: 15, min_lon_e7: -75_000_000, clustered: true, ..Default::default() };
        assert_eq!(Header::parse(&h.to_bytes()).unwrap(), h);
    }

    #[test]
    fn big_directories_split_into_leaves() {
        let entries: Vec<Entry> = (0..200_000u64)
            .map(|i| Entry { tile_id: i * 3, offset: i * 1000 + (i % 7) * 13, length: 900 + (i % 97) as u32, run_length: 1 })
            .collect();
        let (root, leaves) = build_directories(&entries);
        assert!(root.len() <= ROOT_MAX && !leaves.is_empty());
    }
}
