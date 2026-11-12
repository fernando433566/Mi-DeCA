//! Recorte de una región a partir de un PMTiles remoto (p. ej. la versión diaria
//! mundial de Protomaps, ~140 GB) usando solo peticiones HTTP Range: se descargan
//! únicamente los directorios y las teselas de la zona pedida.
//!
//! Fichero resultante: cabecera | raíz (reservados 16 KiB) | teselas | metadatos | hojas

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use anyhow::{ensure, Context};
use futures::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use super::pmtiles::{self, Entry, Header, COMPRESSION_GZIP, HEADER_LEN};
use super::tiles::{tile_range, BBox};

/// Peticiones simultáneas al servidor de origen.
const PARALLEL: usize = 4;
/// Se unen en una sola petición los trozos separados por menos de esto…
const MAX_GAP: u64 = 256 * 1024;
/// …mientras la petición no supere este tamaño.
const MAX_BATCH: u64 = 16 * 1024 * 1024;

/// Presupuesto independiente del contenido del proveedor para terminar el recorrido.
#[derive(Default)]
struct DirectoryTraversal {
    depth: usize,
    visited: HashSet<(u64, u64)>,
}
impl DirectoryTraversal {
    fn next_level(&mut self) -> anyhow::Result<()> {
        self.depth += 1;
        ensure!(self.depth <= 16, "demasiados niveles de directorios PMTiles");
        Ok(())
    }
    fn visit(&mut self, offset: u64, length: u64) -> anyhow::Result<()> {
        ensure!(self.visited.len() < 100_000, "demasiados directorios PMTiles");
        ensure!(self.visited.insert((offset, length)), "directorio PMTiles repetido o cíclico");
        Ok(())
    }
}

pub struct Remote {
    pub http: reqwest::Client,
    pub url: String,
}

impl Remote {
    /// Lee `len` bytes desde `offset` (con reintentos).
    pub async fn range(&self, offset: u64, len: u64) -> anyhow::Result<Vec<u8>> {
        if len == 0 {
            return Ok(Vec::new());
        }
        ensure!(len <= MAX_BATCH, "rango PMTiles demasiado grande");
        let end = offset.checked_add(len - 1).context("rango PMTiles desbordado")?;
        let mut wait = Duration::from_secs(2);
        let mut last_err = None;
        for _ in 0..5 {
            let res = self
                .http
                .get(&self.url)
                .header(reqwest::header::RANGE, format!("bytes={}-{}", offset, end))
                .timeout(Duration::from_secs(300))
                .send()
                .await;
            match res {
                Ok(mut r) if r.status() == reqwest::StatusCode::PARTIAL_CONTENT => {
                    let mut bytes = Vec::new();
                    let mut error = None;
                    loop {
                        match r.chunk().await {
                            Ok(Some(chunk)) => {
                                ensure!(bytes.len() as u64 + chunk.len() as u64 <= len, "respuesta PMTiles mayor que el rango solicitado");
                                bytes.extend_from_slice(&chunk);
                            }
                            Ok(None) => break,
                            Err(e) => { error = Some(e); break; }
                        }
                    }
                    if error.is_none() && bytes.len() as u64 == len { return Ok(bytes); }
                    last_err = Some(anyhow::anyhow!("respuesta PMTiles incompleta"));
                },
                Ok(r) => last_err = Some(anyhow::anyhow!("el servidor de mapas respondió {}", r.status())),
                Err(e) => last_err = Some(e.into()),
            }
            tokio::time::sleep(wait).await;
            wait *= 2;
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("error de descarga")))
    }
}

/// Une rangos `(offset, len)` ordenados en peticiones más grandes.
fn batches(ranges: &[(u64, u64)]) -> Vec<(u64, u64)> {
    let mut out: Vec<(u64, u64)> = Vec::new();
    for &(off, len) in ranges {
        if let Some(last) = out.last_mut() {
            let end = last.0 + last.1;
            if off >= last.0 && off <= end + MAX_GAP && off + len - last.0 <= MAX_BATCH {
                last.1 = last.1.max(off + len - last.0);
                continue;
            }
        }
        out.push((off, len));
    }
    out
}

/// ¿Hay algún id de `needed` (ordenado) en `[start, end)`?
fn any_in(needed: &[u64], start: u64, end: u64) -> bool {
    let i = needed.partition_point(|&id| id < start);
    i < needed.len() && needed[i] < end
}

pub struct ExtractResult {
    pub size: u64,
    pub sha256: String,
    pub tiles: u64,
}

/// Recorta la región `bbox` (zoom 0..=`max_zoom`) y la escribe en `dest`.
/// `progress` recibe el avance en tanto por mil.
pub async fn extract(remote: &Remote, bbox: &BBox, max_zoom: u8, dest: &Path, progress: &AtomicU32) -> anyhow::Result<ExtractResult> {
    // 1. Cabecera y directorio raíz (están en los primeros 16 KiB).
    let head = remote.range(0, 16_384).await.context("no se pudo leer la cabecera del mapa")?;
    let h = Header::parse(&head)?;
    let max_zoom = max_zoom.min(h.max_zoom);
    ensure!(h.min_zoom <= max_zoom && max_zoom <= 15, "zoom PMTiles fuera del límite");
    let root_end = h.root_offset.checked_add(h.root_length).context("raíz PMTiles desbordada")?;
    let root_raw = if root_end <= head.len() as u64 {
        head[h.root_offset as usize..root_end as usize].to_vec()
    } else {
        remote.range(h.root_offset, h.root_length).await?
    };
    let root = pmtiles::decode_directory(&pmtiles::decompress(&root_raw, h.internal_compression)?)?;

    // 2. Teselas necesarias, en orden de id.
    let mut needed: Vec<u64> = Vec::new();
    for z in h.min_zoom..=max_zoom {
        let (x0, x1, y0, y1) = tile_range(bbox, z);
        for x in x0..=x1 {
            for y in y0..=y1 {
                needed.push(pmtiles::zxy_to_id(z, x, y));
            }
        }
    }
    needed.sort_unstable();
    needed.dedup();
    progress.store(10, Ordering::Relaxed);

    // 3. Recorrer directorios (raíz -> hojas) recogiendo las teselas de la zona.
    let mut found: Vec<(u64, u64, u32)> = Vec::new(); // (id, desplazamiento origen, longitud)
    let mut level: Vec<(Vec<Entry>, u64)> = vec![(root, u64::MAX)];
    let mut traversal = DirectoryTraversal::default();
    while !level.is_empty() {
        traversal.next_level()?;
        let mut leaves: Vec<(u64, u64, u64)> = Vec::new(); // (offset, len, límite superior)
        for (entries, upper) in &level {
            for (i, e) in entries.iter().enumerate() {
                if e.run_length > 0 {
                    let end = e.tile_id.checked_add(e.run_length as u64).context("rango de teselas desbordado")?;
                    let start_idx = needed.partition_point(|&id| id < e.tile_id);
                    for &id in needed[start_idx..].iter().take_while(|&&id| id < end) {
                        let offset = h.data_offset.checked_add(e.offset).context("desplazamiento de tesela desbordado")?;
                        offset.checked_add(e.length as u64).context("longitud de tesela desbordada")?;
                        ensure!(e.length as u64 <= MAX_BATCH, "tesela demasiado grande");
                        found.push((id, offset, e.length));
                    }
                } else {
                    let end = entries.get(i + 1).map(|n| n.tile_id).unwrap_or(*upper);
                    if any_in(&needed, e.tile_id, end) {
                        let offset = h.leaf_offset.checked_add(e.offset).context("desplazamiento de hoja desbordado")?;
                        offset.checked_add(e.length as u64).context("longitud de hoja desbordada")?;
                        traversal.visit(offset, e.length as u64)?;
                        leaves.push((offset, e.length as u64, end));
                    }
                }
            }
        }
        leaves.sort_unstable();
        let ranges: Vec<(u64, u64)> = leaves.iter().map(|&(o, l, _)| (o, l)).collect();
        let fetched: Vec<anyhow::Result<(u64, Vec<u8>)>> = futures::stream::iter(batches(&ranges))
            .map(|(off, len)| async move { remote.range(off, len).await.map(|b| (off, b)) })
            .buffered(PARALLEL)
            .collect()
            .await;
        let mut blobs = Vec::new();
        for f in fetched {
            blobs.push(f?);
        }
        let mut next = Vec::new();
        for (off, len, upper) in leaves {
            let (boff, blob) = blobs
                .iter()
                .find(|(b, data)| *b <= off && off + len <= *b + data.len() as u64)
                .context("directorio hoja no descargado")?;
            let start = (off - boff) as usize;
            let raw = pmtiles::decompress(&blob[start..start + len as usize], h.internal_compression)?;
            next.push((pmtiles::decode_directory(&raw)?, upper));
        }
        level = next;
    }
    ensure!(!found.is_empty(), "no hay datos de mapa para esta zona");
    found.sort_unstable_by_key(|f| f.0);
    progress.store(50, Ordering::Relaxed);

    // 4. Descargar las teselas (cada contenido una sola vez) y escribirlas tras los 16 KiB reservados.
    let mut unique: Vec<(u64, u64)> = found.iter().map(|&(_, off, len)| (off, len as u64)).collect();
    unique.sort_unstable();
    unique.dedup();
    let total_bytes: u64 = unique.iter().map(|u| u.1).sum::<u64>().max(1);

    let tmp = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&tmp).await?;
    file.write_all(&vec![0u8; 16_384]).await?;
    let data_offset = 16_384u64;
    let mut written = 0u64;
    let mut new_offsets: HashMap<u64, u64> = HashMap::with_capacity(unique.len());

    let mut stream = futures::stream::iter(batches(&unique))
        .map(|(off, len)| async move { remote.range(off, len).await.map(|b| (off, b)) })
        .buffered(PARALLEL);
    let mut ui = 0;
    while let Some(res) = stream.next().await {
        let (boff, blob) = res?;
        let bend = boff + blob.len() as u64;
        while ui < unique.len() && unique[ui].0 + unique[ui].1 <= bend {
            let (off, len) = unique[ui];
            let start = (off - boff) as usize;
            file.write_all(&blob[start..start + len as usize]).await?;
            new_offsets.insert(off, written);
            written += len;
            ui += 1;
        }
        progress.store(50 + (written * 940 / total_bytes) as u32, Ordering::Relaxed);
    }
    ensure!(ui == unique.len(), "faltan teselas por descargar");

    // 5. Directorio nuevo (agrupando teselas consecutivas idénticas).
    let mut entries: Vec<Entry> = Vec::new();
    for &(id, off, len) in &found {
        let new_off = new_offsets[&off];
        if let Some(last) = entries.last_mut() {
            if last.tile_id + last.run_length as u64 == id && last.offset == new_off && last.length == len {
                last.run_length += 1;
                continue;
            }
        }
        entries.push(Entry { tile_id: id, offset: new_off, length: len, run_length: 1 });
    }
    let (root_dir, leaf_dirs) = pmtiles::build_directories(&entries);

    // 6. Metadatos (atribución, capas…) del original, en gzip.
    let meta_raw = remote.range(h.metadata_offset, h.metadata_length).await?;
    let metadata = match h.internal_compression {
        COMPRESSION_GZIP => meta_raw,
        c => pmtiles::gzip(&pmtiles::decompress(&meta_raw, c)?),
    };

    let metadata_offset = data_offset + written;
    file.write_all(&metadata).await?;
    let leaf_offset = metadata_offset + metadata.len() as u64;
    file.write_all(&leaf_dirs).await?;

    let e7 = |v: f64| (v * 1e7).round() as i32;
    let header = Header {
        root_offset: HEADER_LEN as u64,
        root_length: root_dir.len() as u64,
        metadata_offset,
        metadata_length: metadata.len() as u64,
        leaf_offset,
        leaf_length: leaf_dirs.len() as u64,
        data_offset,
        data_length: written,
        addressed_tiles: found.len() as u64,
        tile_entries: entries.len() as u64,
        tile_contents: unique.len() as u64,
        clustered: h.clustered,
        internal_compression: COMPRESSION_GZIP,
        tile_compression: h.tile_compression,
        tile_type: h.tile_type,
        min_zoom: h.min_zoom,
        max_zoom,
        min_lon_e7: e7(bbox.min_lon),
        min_lat_e7: e7(bbox.min_lat),
        max_lon_e7: e7(bbox.max_lon),
        max_lat_e7: e7(bbox.max_lat),
        center_zoom: max_zoom.min(9),
        center_lon_e7: e7((bbox.min_lon + bbox.max_lon) / 2.0),
        center_lat_e7: e7((bbox.min_lat + bbox.max_lat) / 2.0),
    };
    file.seek(std::io::SeekFrom::Start(0)).await?;
    file.write_all(&header.to_bytes()).await?;
    file.write_all(&root_dir).await?;
    file.flush().await?;
    file.sync_all().await?;
    drop(file);

    // 7. Hash SHA-256 para que la app compruebe la descarga.
    let mut f = tokio::fs::File::open(&tmp).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut size = 0u64;
    loop {
        let n = f.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    tokio::fs::rename(&tmp, dest).await?;
    progress.store(1000, Ordering::Relaxed);
    Ok(ExtractResult { size, sha256: data_encoding::HEXLOWER.encode(&hasher.finalize()), tiles: found.len() as u64 })
}

/// Comprueba un PMTiles local: cabecera, directorios y que cada tesela sea legible.
#[cfg(test)]
pub fn verify_local(bytes: &[u8]) -> anyhow::Result<u64> {
    let h = Header::parse(bytes)?;
    let mut stack = vec![bytes[h.root_offset as usize..(h.root_offset + h.root_length) as usize].to_vec()];
    let mut tiles = 0;
    while let Some(raw) = stack.pop() {
        for e in pmtiles::decode_directory(&pmtiles::decompress(&raw, h.internal_compression)?)? {
            if e.run_length == 0 {
                let o = (h.leaf_offset + e.offset) as usize;
                stack.push(bytes[o..o + e.length as usize].to_vec());
            } else {
                let o = (h.data_offset + e.offset) as usize;
                if o + e.length as usize > bytes.len() {
                    anyhow::bail!("tesela fuera del fichero");
                }
                tiles += e.run_length as u64;
            }
        }
    }
    Ok(tiles)
}

#[cfg(test)]
mod security_tests {
    use super::*;
    #[test]
    fn repeated_leaf_and_excess_depth_are_rejected() {
        let mut traversal = DirectoryTraversal::default();
        traversal.visit(16384, 8).unwrap();
        assert!(traversal.visit(16384, 8).is_err());
        for _ in 0..16 { traversal.next_level().unwrap(); }
        assert!(traversal.next_level().is_err());
    }
}
