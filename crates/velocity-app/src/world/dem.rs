//! Real elevation data for the virtual world.
//!
//! SRTM/Copernicus-style elevation is streamed as Mapzen "Terrarium" PNG tiles
//! from the public AWS Open Data bucket (CC-BY 4.0; attribution belongs in the
//! About dialog). Tiles are decoded into a small height grid, cached on disk and
//! sampled bilinearly across tile borders, so virtual mountains follow the real
//! landscape instead of invented noise.
//!
//! Everything degrades gracefully: when the network is unavailable the world
//! still renders from the route's own elevation profile, and real DEM data is
//! folded in as soon as the background loader delivers it.

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use velocity_core::route::Route;

pub const DEM_ATTRIBUTION: &str =
    "Elevation: Mapzen Terrain Tiles (SRTM, Copernicus, GMTED), CC-BY 4.0, hosted by AWS Open Data";

const TILE_SIZE: u32 = 256;
const MAX_TILES: usize = 36;
const MIN_ZOOM: u8 = 9;
const MAX_ZOOM: u8 = 14;
const TILE_URL: &str = "https://s3.amazonaws.com/elevation-tiles-prod/terrarium";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DemStatus {
    /// No tiles requested yet.
    #[default]
    Idle,
    /// Background loader is fetching tiles.
    Loading,
    /// Real elevation is available for the route area.
    Ready,
    /// Network unavailable and nothing cached; route profile is used.
    Offline,
    /// Bundled demo route: its elevation profile is illustrative, so the world
    /// is built from the route itself rather than from real DEM tiles.
    Demo,
}

impl DemStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "no elevation tiles",
            Self::Loading => "loading real elevation",
            Self::Ready => "real elevation",
            Self::Offline => "route elevation (offline)",
            Self::Demo => "demo terrain (route-built)",
        }
    }
}

/// Local tangent-plane projection around the first route point.
#[derive(Clone, Copy, Debug)]
pub struct GeoOrigin {
    pub lat0: f64,
    pub lon0: f64,
    metres_per_deg_lat: f64,
    metres_per_deg_lon: f64,
}

impl GeoOrigin {
    pub fn new(lat0: f64, lon0: f64) -> Self {
        Self {
            lat0,
            lon0,
            metres_per_deg_lat: 111_132.95,
            metres_per_deg_lon: 111_319.49 * lat0.to_radians().cos().abs().max(0.05),
        }
    }

    /// World metres: `x` east, `z` south, `y` up.
    pub fn to_world(&self, lat: f64, lon: f64) -> (f32, f32) {
        (
            ((lon - self.lon0) * self.metres_per_deg_lon) as f32,
            (-(lat - self.lat0) * self.metres_per_deg_lat) as f32,
        )
    }

    pub fn to_latlon(&self, x: f32, z: f32) -> (f64, f64) {
        (
            self.lat0 - (z as f64) / self.metres_per_deg_lat,
            self.lon0 + (x as f64) / self.metres_per_deg_lon,
        )
    }
}

/// Height grids for every tile of the route area that has been loaded.
#[derive(Default)]
pub struct DemTiles {
    pub tiles: HashMap<(u32, u32), Arc<Vec<f32>>>,
    pub version: u64,
    pub status: DemStatus,
}

impl DemTiles {
    pub fn insert(&mut self, z: u8, x: u32, y: u32, heights: Vec<f32>) {
        let _ = z;
        if heights.len() != (TILE_SIZE * TILE_SIZE) as usize {
            return;
        }
        self.tiles.insert((x, y), Arc::new(heights));
        self.version = self.version.wrapping_add(1);
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    fn pixel(&self, ix: i32, iy: i32) -> Option<f32> {
        let tx = ix.div_euclid(TILE_SIZE as i32) as u32;
        let ty = iy.div_euclid(TILE_SIZE as i32) as u32;
        let lx = ix.rem_euclid(TILE_SIZE as i32) as usize;
        let ly = iy.rem_euclid(TILE_SIZE as i32) as usize;
        let tile = self.tiles.get(&(tx, ty))?;
        Some(tile[ly * TILE_SIZE as usize + lx])
    }

    /// Bilinear height in metres at a WGS84 coordinate, if covered by loaded tiles.
    pub fn height(&self, zoom: u8, lat: f64, lon: f64) -> Option<f32> {
        if self.tiles.is_empty() {
            return None;
        }
        let n = (1u32 << zoom) as f64 * TILE_SIZE as f64;
        let gx = (lon + 180.0) / 360.0 * n - 0.5;
        let lat_rad = lat.to_radians();
        let gy = (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) / 2.0
            * n
            - 0.5;
        if !gx.is_finite() || !gy.is_finite() {
            return None;
        }
        let x0 = gx.floor();
        let y0 = gy.floor();
        let fx = (gx - x0) as f32;
        let fy = (gy - y0) as f32;
        let ix = x0 as i32;
        let iy = y0 as i32;
        let h00 = self.pixel(ix, iy)?;
        let h10 = self.pixel(ix + 1, iy)?;
        let h01 = self.pixel(ix, iy + 1)?;
        let h11 = self.pixel(ix + 1, iy + 1)?;
        let h0 = h00 + (h10 - h00) * fx;
        let h1 = h01 + (h11 - h01) * fx;
        Some(h0 + (h1 - h0) * fy)
    }
}

pub enum DemMessage {
    Tile {
        zoom: u8,
        x: u32,
        y: u32,
        heights: Vec<f32>,
    },
    Done {
        downloaded: usize,
        cached: usize,
        failed: usize,
    },
}

/// Pick a zoom level that covers the route (plus a margin) with a bounded number
/// of tiles. Coarse tiles are cheap; the route mesh supplies road detail.
pub fn choose_zoom(route: &Route) -> u8 {
    let (min_lat, max_lat, min_lon, max_lon) = route_bounds(route, 2_500.0);
    for zoom in (MIN_ZOOM..=MAX_ZOOM).rev() {
        if tile_bounds(zoom, min_lat, max_lat, min_lon, max_lon).2 <= MAX_TILES {
            return zoom;
        }
    }
    MIN_ZOOM
}

fn route_bounds(route: &Route, margin_m: f64) -> (f64, f64, f64, f64) {
    let mut min_lat = f64::INFINITY;
    let mut max_lat = f64::NEG_INFINITY;
    let mut min_lon = f64::INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    for p in &route.points {
        min_lat = min_lat.min(p.lat);
        max_lat = max_lat.max(p.lat);
        min_lon = min_lon.min(p.lon);
        max_lon = max_lon.max(p.lon);
    }
    if !min_lat.is_finite() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let dlat = margin_m / 111_132.95;
    let dlon = margin_m / (111_319.49 * min_lat.to_radians().cos().abs().max(0.05));
    (
        min_lat - dlat,
        max_lat + dlat,
        min_lon - dlon,
        max_lon + dlon,
    )
}

/// Returns `(x0, y0, tile_count)` for the tiles covering the bounds at `zoom`.
fn tile_bounds(zoom: u8, min_lat: f64, max_lat: f64, min_lon: f64, max_lon: f64) -> (u32, u32, usize) {
    let n = 1u32 << zoom;
    let x_for = |lon: f64| (((lon + 180.0) / 360.0) * n as f64).floor().clamp(0.0, (n - 1) as f64) as u32;
    let y_for = |lat: f64| {
        let lat = lat.clamp(-85.0, 85.0).to_radians();
        let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0;
        (y * n as f64).floor().clamp(0.0, (n - 1) as f64) as u32
    };
    let x0 = x_for(min_lon);
    let x1 = x_for(max_lon);
    let y0 = y_for(max_lat);
    let y1 = y_for(min_lat);
    let count = ((x1 - x0 + 1) as usize) * ((y1 - y0 + 1) as usize);
    (x0, y0, count)
}

/// Background tile loader. Results arrive over a channel so the render thread
/// never blocks on HTTP or PNG decoding.
pub struct DemLoader {
    pub rx: Receiver<DemMessage>,
    pub zoom: u8,
}

impl DemLoader {
    pub fn spawn(route: Arc<Route>, cache_dir: PathBuf) -> Self {
        let zoom = choose_zoom(&route);
        let (min_lat, max_lat, min_lon, max_lon) = route_bounds(&route, 2_500.0);
        let (x0, y0, _) = tile_bounds(zoom, min_lat, max_lat, min_lon, max_lon);
        let (x1, y1) = {
            let n = 1u32 << zoom;
            let x_for = |lon: f64| (((lon + 180.0) / 360.0) * n as f64).floor().clamp(0.0, (n - 1) as f64) as u32;
            let y_for = |lat: f64| {
                let lat = lat.clamp(-85.0, 85.0).to_radians();
                let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0;
                (y * n as f64).floor().clamp(0.0, (n - 1) as f64) as u32
            };
            (x_for(max_lon), y_for(min_lat))
        };
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("velocity-dem".to_string())
            .spawn(move || fetch_tiles(tx, zoom, x0, y0, x1, y1, cache_dir))
            .ok();
        Self { rx, zoom }
    }
}

fn fetch_tiles(
    tx: Sender<DemMessage>,
    zoom: u8,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    cache_dir: PathBuf,
) {
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(8)))
            .user_agent("VELOCITY/0.1 (open-source cycling app)")
            .build(),
    );
    let mut downloaded = 0;
    let mut cached = 0;
    let mut failed = 0;
    let started = Instant::now();
    for y in y0..=y1 {
        for x in x0..=x1 {
            let path = cache_dir.join(format!("{zoom}/{x}/{y}.png"));
            let bytes = match std::fs::read(&path) {
                Ok(bytes) => {
                    cached += 1;
                    Some(bytes)
                }
                Err(_) => {
                    let url = format!("{TILE_URL}/{zoom}/{x}/{y}.png");
                    match agent.get(&url).call() {
                        Ok(mut response) => {
                            let mut bytes = Vec::new();
                            match response.body_mut().as_reader().read_to_end(&mut bytes) {
                                Ok(_) => {
                                    downloaded += 1;
                                    if let Some(parent) = path.parent() {
                                        let _ = std::fs::create_dir_all(parent);
                                    }
                                    let _ = std::fs::write(&path, &bytes);
                                    Some(bytes)
                                }
                                Err(_) => {
                                    failed += 1;
                                    None
                                }
                            }
                        }
                        Err(_) => {
                            failed += 1;
                            None
                        }
                    }
                }
            };
            if let Some(bytes) = bytes {
                if let Some(heights) = decode_terrarium(&bytes) {
                    if tx
                        .send(DemMessage::Tile {
                            zoom,
                            x,
                            y,
                            heights,
                        })
                        .is_err()
                    {
                        return;
                    }
                } else {
                    failed += 1;
                }
            }
            // Never hammer the bucket when many tiles are missing.
            if downloaded > 0 && downloaded % 6 == 0 {
                std::thread::sleep(Duration::from_millis(40));
            }
        }
    }
    let _ = started;
    let _ = tx.send(DemMessage::Done {
        downloaded,
        cached,
        failed,
    });
}

pub fn decode_terrarium(bytes: &[u8]) -> Option<Vec<f32>> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    if reader.info().width != TILE_SIZE || reader.info().height != TILE_SIZE {
        return None;
    }
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Indexed => return None,
    };
    if info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let mut heights = Vec::with_capacity((TILE_SIZE * TILE_SIZE) as usize);
    for px in buf.chunks_exact(channels) {
        let (r, g, b) = match channels {
            3 | 4 => (px[0] as f32, px[1] as f32, px[2] as f32),
            _ => (px[0] as f32, px[0] as f32, px[0] as f32),
        };
        heights.push(r * 256.0 + g + b / 256.0 - 32_768.0);
    }
    Some(heights)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_roundtrip() {
        let origin = GeoOrigin::new(48.0, 11.3);
        let (x, z) = origin.to_world(48.001, 11.303);
        let (lat, lon) = origin.to_latlon(x, z);
        assert!((lat - 48.001).abs() < 1e-6);
        assert!((lon - 11.303).abs() < 1e-6);
        assert!(x > 0.0 && z < 0.0);
    }

    #[test]
    fn zoom_choice_is_bounded() {
        let route = velocity_core::route::demo_route();
        let zoom = choose_zoom(&route);
        assert!((MIN_ZOOM..=MAX_ZOOM).contains(&zoom));
        let (min_lat, max_lat, min_lon, max_lon) = route_bounds(&route, 2_500.0);
        assert!(tile_bounds(zoom, min_lat, max_lat, min_lon, max_lon).2 <= MAX_TILES);
    }

    #[test]
    fn samples_tile_and_seam() {
        let zoom = 11u8;
        let mut tiles = DemTiles::default();
        for ty in 0..2 {
            for tx in 0..2 {
                let mut h = vec![0.0f32; (TILE_SIZE * TILE_SIZE) as usize];
                for (i, v) in h.iter_mut().enumerate() {
                    *v = (tx * TILE_SIZE + (i as u32 % TILE_SIZE)) as f32;
                }
                tiles.insert(zoom, tx, ty, h);
            }
        }
        let n = (1u64 << zoom) as f64 * TILE_SIZE as f64;
        // Exactly between the last pixel of tile 0 and the first of tile 1:
        // the sampler has to cross the tile boundary.
        let gx = 255.5f64;
        let gy = 128.5f64;
        // Pixel (i, j) is centred at continuous coordinate (i + 0.5, j + 0.5).
        let lon = ((gx + 0.5) / n) * 360.0 - 180.0;
        let lat = ((1.0 - 2.0 * (gy + 0.5) / n) * std::f64::consts::PI)
            .sinh()
            .atan()
            .to_degrees();
        let h = tiles
            .height(zoom, lat, lon)
            .expect("sample across the tile seam");
        assert!((h - 255.5).abs() < 0.01, "got {h}");
    }
}
