//! Mapas de OpenStreetMap.
//!
//! Funcionan como en Organic Maps: el mundo se divide en regiones y cada región es
//! un único fichero de mapa vectorial que la app descarga una vez (con reanudación
//! por HTTP Range y comprobación de hash) y usa después sin conexión. Hay un catálogo
//! versionado para saber cuándo hay una versión más nueva.
//!
//! - [`regions`]: catálogo, cola de generación y actualización automática.
//! - [`extract`]: recorte de una región desde la versión diaria de Protomaps.
//! - [`pmtiles`]: formato del fichero (PMTiles v3, compatible con MapLibre).
//! - [`tiles`]: teselas raster en caché para ver el mapa con conexión.

pub mod extract;
pub mod pmtiles;
pub mod regions;
pub mod tiles;

pub use tiles::BBox;

pub const USER_AGENT: &str = concat!("DeCA-Server/", env!("CARGO_PKG_VERSION"));
