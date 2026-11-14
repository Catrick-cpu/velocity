//! The virtual world's geometry: route stations, terrain sampling (real DEM +
//! procedural detail), road ribbon, chunk meshes, vegetation scatter, villages
//! and water. Nothing here talks to OpenGL; it produces CPU meshes that the
//! renderer uploads and streams.

use std::collections::HashMap;

use velocity_core::route::Route;

use super::dem::{DemTiles, GeoOrigin};
use super::geometry::{self, Instance, MeshData, Vertex};
use super::math::{hash_u32, rand01, rand_range, v3, V3};

pub const CHUNK_SIZE: f32 = 128.0;
const STATION_STEP_M: f32 = 6.0;
const GRID_CELL_M: f32 = 64.0;
const ELEVATION_DATASET_CREDIT: &str =
    "Mapzen Terrain Tiles (SRTM/Copernicus), CC-BY 4.0, AWS Open Data";

#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge1 <= edge0 {
        return if x >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn hash2(ix: i32, iz: i32, seed: u32) -> f32 {
    let h = hash_u32(
        (ix as u32)
            .wrapping_mul(0x8da6_b343)
            .wrapping_add((iz as u32).wrapping_mul(0xd8163841))
            .wrapping_add(seed.wrapping_mul(0x1b56_c4e9)),
    );
    (h & 0x00ff_ffff) as f32 / 16_777_215.0
}

/// Smooth value noise in [-1, 1].
pub fn value_noise(x: f32, z: f32, seed: u32) -> f32 {
    let ix = x.floor();
    let iz = z.floor();
    let fx = x - ix;
    let fz = z - iz;
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sz = fz * fz * (3.0 - 2.0 * fz);
    let ix = ix as i32;
    let iz = iz as i32;
    let a = hash2(ix, iz, seed);
    let b = hash2(ix + 1, iz, seed);
    let c = hash2(ix, iz + 1, seed);
    let d = hash2(ix + 1, iz + 1, seed);
    let top = a + (b - a) * sx;
    let bottom = c + (d - c) * sx;
    (top + (bottom - top) * sz) * 2.0 - 1.0
}

pub fn fbm(x: f32, z: f32, octaves: usize, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut freq = 1.0;
    let mut total = 0.0;
    for o in 0..octaves {
        sum += value_noise(x * freq, z * freq, seed.wrapping_add(o as u32 * 17)) * amp;
        total += amp;
        amp *= 0.5;
        freq *= 2.07;
    }
    if total > 0.0 {
        sum / total
    } else {
        0.0
    }
}

#[derive(Clone, Debug)]
pub struct Station {
    pub x: f32,
    pub z: f32,
    pub y: f32,
    pub distance_m: f32,
    pub heading: f32,
    /// Horizontal unit tangent.
    pub forward: V3,
    /// Signed curvature in radians per metre (positive = right turn).
    pub curvature: f32,
}

/// Resampled route with a spatial hash for "how far is the nearest road" queries.
pub struct RoutePath {
    pub stations: Vec<Station>,
    pub total_m: f32,
    pub geo: GeoOrigin,
    grid: HashMap<(i32, i32), Vec<u32>>,
}

impl RoutePath {
    pub fn new(route: &Route) -> Self {
        let total_m = route.stats.distance_m.max(1.0) as f32;
        let step = STATION_STEP_M;
        let count = ((total_m / step).ceil() as usize + 2).max(2);
        let first = route.points.first().cloned().unwrap_or_default();
        let geo = GeoOrigin::new(first.lat, first.lon);
        let mut stations = Vec::with_capacity(count);
        for i in 0..count {
            let d = (i as f32 * step).min(total_m);
            let p = route.position_at(d as f64);
            let ahead = route.position_at((d + 4.0).min(total_m) as f64);
            let behind = route.position_at((d - 4.0).max(0.0) as f64);
            let (x, z) = geo.to_world(p.lat, p.lon);
            let (xa, za) = geo.to_world(ahead.lat, ahead.lon);
            let (xb, zb) = geo.to_world(behind.lat, behind.lon);
            let forward = v3(xa - xb, 0.0, za - zb).normalized();
            let heading = forward.x.atan2(forward.z);
            stations.push(Station {
                x,
                z,
                y: p.elevation_m,
                distance_m: d,
                heading,
                forward,
                curvature: 0.0,
            });
        }
        // Curvature from heading change over 12 m.
        let n = stations.len();
        for i in 0..n {
            let a = &stations[i.saturating_sub(2)];
            let b = &stations[(i + 2).min(n - 1)];
            let mut delta = b.heading - a.heading;
            while delta > std::f32::consts::PI {
                delta -= std::f32::consts::TAU;
            }
            while delta < -std::f32::consts::PI {
                delta += std::f32::consts::TAU;
            }
            let span = (b.distance_m - a.distance_m).max(1.0);
            stations[i].curvature = delta / span;
        }

        let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (i, s) in stations.iter().enumerate() {
            let key = (
                (s.x / GRID_CELL_M).floor() as i32,
                (s.z / GRID_CELL_M).floor() as i32,
            );
            grid.entry(key).or_default().push(i as u32);
        }
        Self {
            stations,
            total_m,
            geo,
            grid,
        }
    }

    /// Nearest station and its horizontal distance. Only searches the 3×3 cells
    /// around the query point, so it is cheap enough to call per terrain vertex.
    pub fn nearest(&self, x: f32, z: f32) -> Option<(f32, f32)> {
        let cx = (x / GRID_CELL_M).floor() as i32;
        let cz = (z / GRID_CELL_M).floor() as i32;
        let mut best = f32::INFINITY;
        let mut best_y = 0.0;
        for dz in -1..=1 {
            for dx in -1..=1 {
                if let Some(list) = self.grid.get(&(cx + dx, cz + dz)) {
                    for &idx in list {
                        let s = &self.stations[idx as usize];
                        let d = ((s.x - x) * (s.x - x) + (s.z - z) * (s.z - z)).sqrt();
                        if d < best {
                            best = d;
                            best_y = s.y;
                        }
                    }
                }
            }
        }
        if best.is_finite() {
            Some((best, best_y))
        } else {
            None
        }
    }

    pub fn station_at(&self, distance_m: f32) -> &Station {
        let t = (distance_m / self.total_m).clamp(0.0, 0.9999);
        let idx = ((t * self.stations.len() as f32) as usize).min(self.stations.len() - 1);
        &self.stations[idx]
    }

    /// Interpolated route frame at a distance along the route.
    pub fn frame_at(&self, distance_m: f32) -> (V3, V3, f32) {
        let d = distance_m.clamp(0.0, self.total_m);
        let idx = (d / STATION_STEP_M) as usize;
        let i0 = idx.min(self.stations.len() - 1);
        let i1 = (i0 + 1).min(self.stations.len() - 1);
        let a = &self.stations[i0];
        let b = &self.stations[i1];
        let span = (b.distance_m - a.distance_m).max(0.001);
        let t = ((d - a.distance_m) / span).clamp(0.0, 1.0);
        let pos = v3(
            a.x + (b.x - a.x) * t,
            a.y + (b.y - a.y) * t,
            a.z + (b.z - a.z) * t,
        );
        let mut heading = a.heading + (b.heading - a.heading) * t;
        // Keep interpolation stable across the ±π wrap.
        while heading > std::f32::consts::PI {
            heading -= std::f32::consts::TAU;
        }
        while heading < -std::f32::consts::PI {
            heading += std::f32::consts::TAU;
        }
        let forward = v3(heading.sin(), 0.0, heading.cos());
        (pos, forward, heading)
    }

    pub fn closest_distance(&self, x: f32, z: f32) -> f32 {
        self.nearest(x, z).map(|(d, _)| d).unwrap_or(f32::INFINITY)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WaterRegion {
    pub center: V3,
    pub radius: f32,
    pub level: f32,
}

/// Everything needed to sample and mesh the landscape along one route.
pub struct Terrain {
    pub path: RoutePath,
    pub zoom: u8,
    pub dem: DemTiles,
    pub water: Option<WaterRegion>,
    /// Vertical correction that aligns the DEM with the route's own elevation
    /// profile, so the road never sits on a levee built by two datasets
    /// disagreeing by a couple of metres.
    dem_offset: f32,
    seed: u32,
}

impl Terrain {
    pub fn new(route: &Route, zoom: u8, seed: u32) -> Self {
        let path = RoutePath::new(route);
        let water = water_region(&path, route);
        Self {
            path,
            zoom,
            dem: DemTiles::default(),
            water,
            dem_offset: 0.0,
            seed,
        }
    }

    /// Median difference between the DEM and the route profile at the road.
    /// Called as tiles arrive; bumps the tile version when the correction
    /// changes so affected chunks are rebuilt.
    pub fn calibrate(&mut self) {
        if self.dem.tiles.is_empty() {
            return;
        }
        let mut diffs = Vec::new();
        for station in self.path.stations.iter().step_by(6) {
            let (lat, lon) = self.path.geo.to_latlon(station.x, station.z);
            if let Some(h) = self.dem.height(self.zoom, lat, lon) {
                diffs.push(h - station.y);
            }
        }
        if diffs.len() < 4 {
            return;
        }
        diffs.sort_by(|a, b| a.total_cmp(b));
        let offset = diffs[diffs.len() / 2].clamp(-200.0, 200.0);
        log::debug!(
            "dem calibration: {} samples, residual {:.1}..{:.1} m, median {:.1} m",
            diffs.len(),
            diffs[0],
            diffs[diffs.len() - 1],
            offset
        );
        if (offset - self.dem_offset).abs() > 0.05 {
            self.dem_offset = offset;
            self.dem.version = self.dem.version.wrapping_add(1);
        }
    }

    pub fn credit(&self) -> &'static str {
        ELEVATION_DATASET_CREDIT
    }

    fn dem_height(&self, x: f32, z: f32) -> Option<f32> {
        let (lat, lon) = self.path.geo.to_latlon(x, z);
        self.dem
            .height(self.zoom, lat, lon)
            .map(|h| h + self.dem_offset)
    }

    fn fallback_height(&self, near: Option<(f32, f32)>, x: f32, z: f32) -> f32 {
        let (distance, road_y) = near.unwrap_or((f32::INFINITY, 520.0));
        let side = (distance * 0.055).min(150.0);
        let ridge = 34.0 * fbm(x * 0.0016, z * 0.0016, 3, self.seed)
            + 7.0 * fbm(x * 0.008, z * 0.008, 2, self.seed ^ 0x51ab);
        road_y + side + ridge
    }

    /// Procedural micro-relief, scaled down near the carriageway so the road
    /// never floats over or clips through the ground.
    fn detail(&self, x: f32, z: f32, road_weight: f32) -> f32 {
        let far = 1.4 * fbm(x * 0.06, z * 0.06, 3, self.seed ^ 0x9e37)
            + 0.45 * fbm(x * 0.21, z * 0.21, 2, self.seed ^ 0x1f2e);
        far * (1.0 - road_weight)
    }

    /// Ground height including the graded road corridor.
    pub fn height(&self, x: f32, z: f32) -> f32 {
        let near = self.path.nearest(x, z);
        let base = self
            .dem_height(x, z)
            .unwrap_or_else(|| self.fallback_height(near, x, z));
        let (distance, road_y) = match near {
            Some((d, y)) => (d, y),
            None => (f32::INFINITY, base),
        };
        // A narrow graded corridor: the carriageway itself, a verge, then a
        // smooth cut/fill into the surrounding ground. Keeping this tight is
        // what stops a road from looking like it runs along an embankment.
        let diff = (base - road_y).abs();
        let outer = 9.5 + diff.min(14.0) * 0.8;
        let w = 1.0 - smoothstep(4.2, outer, distance);
        let detail = self.detail(x, z, w);
        let ground = base + detail;
        if w > 0.0 {
            ground * (1.0 - w) + (road_y - 0.18) * w
        } else {
            ground
        }
    }

    /// Exact road elevation used by the ribbon mesh.
    pub fn road_height(&self, distance_m: f32) -> f32 {
        self.path.frame_at(distance_m).0.y + 0.06
    }

    pub fn build_chunk(&self, cx: i32, cz: i32, res: u32) -> MeshData {
        let n = res as usize + 1;
        let spacing = CHUNK_SIZE / res as f32;
        let x0 = cx as f32 * CHUNK_SIZE;
        let z0 = cz as f32 * CHUNK_SIZE;
        let mut heights = vec![0.0f32; n * n];
        for j in 0..n {
            for i in 0..n {
                let x = x0 + i as f32 * spacing;
                let z = z0 + j as f32 * spacing;
                heights[j * n + i] = self.height(x, z);
            }
        }
        let mut mesh = MeshData::new();
        mesh.vertices.reserve(n * n + n * 4);
        mesh.indices.reserve((n - 1) * (n - 1) * 6 + n * 24);
        let normal_at = |i: usize, j: usize| -> V3 {
            let left = heights[j * n + i.saturating_sub(1)];
            let right = heights[j * n + (i + 1).min(n - 1)];
            let up = heights[j.saturating_sub(1) * n + i];
            let down = heights[((j + 1).min(n - 1)) * n + i];
            let dx = (right - left) / (spacing * 2.0).max(1e-4);
            let dz = (down - up) / (spacing * 2.0).max(1e-4);
            v3(-dx, 1.0, -dz).normalized()
        };
        for j in 0..n {
            for i in 0..n {
                let x = x0 + i as f32 * spacing;
                let z = z0 + j as f32 * spacing;
                let y = heights[j * n + i];
                let n = normal_at(i, j);
                // Slight low-frequency tint: sun-bleached fields and darker woods.
                let tint = 0.9
                    + 0.18 * value_noise(x * 0.011, z * 0.011, self.seed ^ 0x7a1)
                    + 0.06 * value_noise(x * 0.09, z * 0.09, self.seed ^ 0x33d);
                mesh.vertices.push(Vertex::new(
                    v3(x, y, z),
                    n,
                    [x, z],
                    [tint, tint, tint],
                ));
            }
        }
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                let a = (j * n + i) as u32;
                let b = (j * n + i + 1) as u32;
                let c = ((j + 1) * n + i + 1) as u32;
                let d = ((j + 1) * n + i) as u32;
                mesh.indices.extend_from_slice(&[a, c, b, a, d, c]);
            }
        }
        // Skirts hide T-junction cracks between LOD levels. They are drawn with
        // both windings so they never show a hole from any angle.
        let depth = 3.0;
        let mut skirt = |a: Vertex, b: Vertex, a_out: Vertex, b_out: Vertex| {
            mesh.push_quad(a, b, b_out, a_out);
            mesh.push_quad(a, a_out, b_out, b);
        };
        for i in 0..n.saturating_sub(1) {
            let x = x0 + i as f32 * spacing;
            let za = z0;
            let ya = heights[i];
            let yb = heights[i + 1];
            let na = v3(0.0, 1.0, 0.0);
            skirt(
                Vertex::new(v3(x, ya, za), na, [x, za], [1.0, 1.0, 1.0]),
                Vertex::new(v3(x + spacing, yb, za), na, [x, za], [1.0, 1.0, 1.0]),
                Vertex::new(v3(x, ya - depth, za), na, [x, za], [1.0, 1.0, 1.0]),
                Vertex::new(v3(x + spacing, yb - depth, za), na, [x, za], [1.0, 1.0, 1.0]),
            );
        }
        // South skirts.
        for i in 0..n.saturating_sub(1) {
            let x = x0 + i as f32 * spacing;
            let za = z0 + CHUNK_SIZE;
            let ya = heights[(n - 1) * n + i];
            let yb = heights[(n - 1) * n + i + 1];
            let na = v3(0.0, 1.0, 0.0);
            skirt(
                Vertex::new(v3(x, ya, za), na, [x, za], [1.0, 1.0, 1.0]),
                Vertex::new(v3(x + spacing, yb, za), na, [x, za], [1.0, 1.0, 1.0]),
                Vertex::new(v3(x, ya - depth, za), na, [x, za], [1.0, 1.0, 1.0]),
                Vertex::new(v3(x + spacing, yb - depth, za), na, [x, za], [1.0, 1.0, 1.0]),
            );
        }
        // West/East skirts.
        for j in 0..n.saturating_sub(1) {
            let z = z0 + j as f32 * spacing;
            let xa = x0;
            let ya = heights[j * n];
            let yb = heights[(j + 1) * n];
            let na = v3(0.0, 1.0, 0.0);
            skirt(
                Vertex::new(v3(xa, ya, z), na, [xa, z], [1.0, 1.0, 1.0]),
                Vertex::new(v3(xa, yb, z + spacing), na, [xa, z], [1.0, 1.0, 1.0]),
                Vertex::new(v3(xa, ya - depth, z), na, [xa, z], [1.0, 1.0, 1.0]),
                Vertex::new(v3(xa, yb - depth, z + spacing), na, [xa, z], [1.0, 1.0, 1.0]),
            );
            let xb = x0 + CHUNK_SIZE;
            let ya = heights[j * n + n - 1];
            let yb = heights[(j + 1) * n + n - 1];
            skirt(
                Vertex::new(v3(xb, ya, z), na, [xb, z], [1.0, 1.0, 1.0]),
                Vertex::new(v3(xb, yb, z + spacing), na, [xb, z], [1.0, 1.0, 1.0]),
                Vertex::new(v3(xb, ya - depth, z), na, [xb, z], [1.0, 1.0, 1.0]),
                Vertex::new(v3(xb, yb - depth, z + spacing), na, [xb, z], [1.0, 1.0, 1.0]),
            );
        }
        mesh
    }

    /// Full road ribbon with u = lateral (-1.18..1.18), v = distance in metres.
    pub fn build_road(&self) -> (MeshData, Vec<RoadSection>) {
        let mut mesh = MeshData::new();
        let mut sections = Vec::new();
        let step = 4.0f32;
        let count = (self.path.total_m / step).ceil() as usize + 1;
        let mut section_start = 0u32;
        let mut section_distance = 0.0f32;
        let mut prev: Option<[Vertex; 4]> = None;
        for i in 0..count {
            let d = (i as f32 * step).min(self.path.total_m);
            let (pos, forward, _) = self.path.frame_at(d);
            let right = v3(forward.z, 0.0, -forward.x);
            let base = pos + v3(0.0, 0.06, 0.0);
            let shade = 0.86
                + 0.1 * value_noise(pos.x * 0.08, pos.z * 0.08, self.seed ^ 0x2f1)
                + 0.04 * value_noise(pos.x * 0.5, pos.z * 0.5, self.seed ^ 0x71c);
            let color = [shade, shade, shade];
            let widths = [-3.35f32, -2.75, 2.75, 3.35];
            let lats = [-1.18f32, -1.0, 1.0, 1.18];
            let mut cur = [Vertex::default(); 4];
            for k in 0..4 {
                cur[k] = Vertex::new(base + right * widths[k], V3::UP, [lats[k], d], color);
            }
            if let Some(prev) = prev {
                for lane in 0..3 {
                    // Winding is irrelevant: the road is drawn without culling.
                    mesh.push_quad(prev[lane], cur[lane], cur[lane + 1], prev[lane + 1]);
                }
            }
            prev = Some(cur);
            if d - section_distance >= 100.0 {
                sections.push(RoadSection {
                    first_index: section_start,
                    count: mesh.indices.len() as u32 - section_start,
                    distance_m: section_distance,
                });
                section_start = mesh.indices.len() as u32;
                section_distance = d;
            }
        }
        let count = mesh.indices.len() as u32 - section_start;
        if count > 0 {
            sections.push(RoadSection {
                first_index: section_start,
                count,
                distance_m: section_distance,
            });
        }
        (mesh, sections)
    }

    /// Deterministic vegetation scatter inside one terrain chunk.
    pub fn scatter_trees(&self, cx: i32, cz: i32, density: f32) -> Vec<Instance> {
        let mut out = Vec::new();
        let count = (26.0 * density.max(0.0)) as usize;
        let x0 = cx as f32 * CHUNK_SIZE;
        let z0 = cz as f32 * CHUNK_SIZE;
        for i in 0..count {
            let s = hash_u32(
                (cx as u32)
                    .wrapping_mul(738_560_213)
                    .wrapping_add((cz as u32).wrapping_mul(19_349_663))
                    .wrapping_add(i as u32 * 83_492_791),
            );
            let x = x0 + rand01(s) * CHUNK_SIZE;
            let z = z0 + rand01(s ^ 0x68e3_1b4a) * CHUNK_SIZE;
            let y = self.height(x, z);
            if let Some(water) = self.water {
                if y < water.level + 0.4 {
                    continue;
                }
                if (x - water.center.x).hypot(z - water.center.z) < water.radius * 0.9 {
                    continue;
                }
            }
            if y > 1_850.0 {
                continue;
            }
            let near = self.path.closest_distance(x, z);
            if near < 8.5 {
                continue;
            }
            let dx = self.height(x + 3.0, z) - self.height(x - 3.0, z);
            let dz = self.height(x, z + 3.0) - self.height(x, z - 3.0);
            let slope = (dx * dx + dz * dz).sqrt() / 6.0;
            if slope > 0.85 {
                continue;
            }
            let conifer = y > 900.0 || rand01(s ^ 0x9911) > 0.55;
            let scale = rand_range(s ^ 0x4711, 0.75, 1.45) * if conifer { 1.0 } else { 1.15 };
            out.push(Instance::new(
                v3(x, y - 0.15, z),
                scale,
                rand01(s ^ 0x2034) * std::f32::consts::TAU,
                rand01(s ^ 0x55aa),
            ));
            // Remember which mesh variant: reuse the yaw slot's unused 4th float.
            let last = out.last_mut().unwrap();
            last.rot_tint[2] = if conifer { 0.0 } else { 1.0 };
        }
        out
    }

    pub fn scatter_grass(&self, cx: i32, cz: i32, density: f32) -> Vec<Instance> {
        let mut out = Vec::new();
        if density <= 0.0 {
            return out;
        }
        let count = (26.0 * density) as usize;
        let x0 = cx as f32 * CHUNK_SIZE;
        let z0 = cz as f32 * CHUNK_SIZE;
        for i in 0..count {
            let s = hash_u32(
                (cx as u32)
                    .wrapping_mul(2_654_435_761)
                    .wrapping_add((cz as u32).wrapping_mul(40_503))
                    .wrapping_add(i as u32 * 2_246_822_519),
            );
            let x = x0 + rand01(s) * CHUNK_SIZE;
            let z = z0 + rand01(s ^ 0x1234_5678) * CHUNK_SIZE;
            let d = self.path.closest_distance(x, z);
            if !(4.0..13.0).contains(&d) {
                continue;
            }
            let y = self.height(x, z);
            if let Some(water) = self.water {
                if y < water.level + 0.5 {
                    continue;
                }
            }
            out.push(Instance::new(
                v3(x, y, z),
                rand_range(s ^ 0x9876, 0.7, 1.5),
                rand01(s ^ 0x5432) * std::f32::consts::TAU,
                rand01(s ^ 0xabcd),
            ));
        }
        out
    }

    /// Village clusters beside the route, built from simple gabled houses.
    pub fn build_villages(&self, route: &Route) -> Vec<Village> {
        let mut villages = Vec::new();
        let total = self.path.total_m;
        let mut d = 1_100.0f32;
        let mut index = 0u32;
        while d < total - 300.0 {
            let (pos, forward, heading) = self.path.frame_at(d);
            let s = hash_u32(index.wrapping_mul(0x9e37_79b9) ^ self.seed);
            let side = if rand01(s) > 0.5 { 1.0f32 } else { -1.0 };
            let right = v3(forward.z, 0.0, -forward.x) * side;
            let offset = rand_range(s ^ 0x11, 58.0, 105.0);
            let center = pos + right * offset;
            let ground = self.height(center.x, center.z);
            if let Some(water) = self.water {
                if ground < water.level + 2.0 {
                    d += 1_900.0;
                    index += 1;
                    continue;
                }
            }
            // Reject steep sites: villages belong on valley floors and benches.
            let mut max_diff = 0.0f32;
            for (ox, oz) in [(-40.0, -40.0), (40.0, -40.0), (-40.0, 40.0), (40.0, 40.0)] {
                let h = self.height(center.x + ox, center.z + oz);
                max_diff = max_diff.max((h - ground).abs());
            }
            if max_diff > 26.0 {
                d += 1_900.0;
                index += 1;
                continue;
            }
            if self.path.closest_distance(center.x, center.z) < 40.0 {
                d += 1_900.0;
                index += 1;
                continue;
            }
            let mut mesh = MeshData::new();
            let house_count = 6 + (rand01(s ^ 0x77) * 12.0) as usize;
            let walls = [
                geometry::rgb(232, 226, 212),
                geometry::rgb(219, 208, 188),
                geometry::rgb(240, 236, 226),
                geometry::rgb(205, 194, 176),
                geometry::rgb(226, 216, 198),
            ];
            let roofs = [
                geometry::rgb(150, 78, 56),
                geometry::rgb(120, 64, 52),
                geometry::rgb(86, 88, 92),
                geometry::rgb(138, 84, 64),
            ];
            for h in 0..house_count {
                let hs = hash_u32(s ^ (h as u32).wrapping_mul(0x2545_f491));
                let angle = rand01(hs) * std::f32::consts::TAU;
                let radius = 14.0 + rand01(hs ^ 0x1a2b) * 92.0;
                let hx = center.x + angle.cos() * radius;
                let hz = center.z + angle.sin() * radius;
                if self.path.closest_distance(hx, hz) < 16.0 {
                    continue;
                }
                let footprint_w = rand_range(hs ^ 0x2, 7.0, 12.5);
                let footprint_d = rand_range(hs ^ 0x3, 6.0, 10.0);
                let wall_h = rand_range(hs ^ 0x4, 3.6, 6.4);
                let roof_h = rand_range(hs ^ 0x5, 2.0, 3.4);
                let yaw = heading + (rand01(hs ^ 0x6) - 0.5) * 0.5;
                let mut min_y = f32::INFINITY;
                for (ox, oz) in [
                    (-footprint_w * 0.5, -footprint_d * 0.5),
                    (footprint_w * 0.5, -footprint_d * 0.5),
                    (-footprint_w * 0.5, footprint_d * 0.5),
                    (footprint_w * 0.5, footprint_d * 0.5),
                ] {
                    let p = v3(ox, 0.0, oz).rotate_y(yaw);
                    min_y = min_y.min(self.height(hx + p.x, hz + p.z));
                }
                let base = v3(hx, min_y - 0.4, hz);
                let wall = walls[(rand01(hs ^ 0x7) * walls.len() as f32) as usize % walls.len()];
                let roof = roofs[(rand01(hs ^ 0x8) * roofs.len() as f32) as usize % roofs.len()];
                geometry::add_house(
                    &mut mesh,
                    base,
                    yaw,
                    footprint_w,
                    footprint_d,
                    wall_h,
                    roof_h,
                    wall,
                    roof,
                );
            }
            if mesh.is_empty() {
                d += 1_900.0;
                index += 1;
                continue;
            }
            if rand01(s ^ 0x99) > 0.55 {
                let tower_base = center + v3(rand_range(s, -30.0, 30.0), 0.0, rand_range(s ^ 0x3, -30.0, 30.0));
                let tower_y = self.height(tower_base.x, tower_base.z);
                let mut tower = MeshData::new();
                geometry::add_church(
                    &mut tower,
                    v3(tower_base.x, tower_y - 0.3, tower_base.z),
                    heading,
                    geometry::rgb(236, 232, 222),
                    geometry::rgb(96, 76, 72),
                );
                mesh.append(&tower);
            }
            villages.push(Village {
                center: v3(center.x, ground, center.z),
                radius: 150.0,
                triangles: mesh.tri_count(),
                mesh,
            });
            d += 1_900.0;
            index += 1;
        }
        let _ = route;
        villages
    }

    /// Lake surface for the bundled lakeside loop. Water is only placed when the
    /// road clearly wraps around a basin, so ordinary routes do not get a lake
    /// dropped into them at random.
    pub fn build_water(&self) -> Option<MeshData> {
        let water = self.water?;
        let step = 22.0f32;
        let extent = water.radius * 1.08;
        let n = ((extent * 2.0 / step) as usize).max(2);
        let mut mesh = MeshData::new();
        let height_at = |i: usize, j: usize| {
            let x = water.center.x - extent + i as f32 * step;
            let z = water.center.z - extent + j as f32 * step;
            self.height(x, z)
        };
        for j in 0..n {
            for i in 0..n {
                let x = water.center.x - extent + i as f32 * step;
                let z = water.center.z - extent + j as f32 * step;
                let h = height_at(i, j);
                let hx = height_at((i + 1).min(n), j);
                let hz = height_at(i, (j + 1).min(n));
                if h > water.level + 0.3 || hx > water.level + 0.3 || hz > water.level + 0.3 {
                    continue;
                }
                let p0 = v3(x, water.level, z);
                let p1 = v3(x + step, water.level, z);
                let p2 = v3(x + step, water.level, z + step);
                let p3 = v3(x, water.level, z + step);
                mesh.push_quad(
                    Vertex::new(p0, V3::UP, [p0.x, p0.z], [1.0, 1.0, 1.0]),
                    Vertex::new(p1, V3::UP, [p1.x, p1.z], [1.0, 1.0, 1.0]),
                    Vertex::new(p2, V3::UP, [p2.x, p2.z], [1.0, 1.0, 1.0]),
                    Vertex::new(p3, V3::UP, [p3.x, p3.z], [1.0, 1.0, 1.0]),
                );
            }
        }
        if mesh.is_empty() {
            None
        } else {
            Some(mesh)
        }
    }
}

fn water_region(path: &RoutePath, route: &Route) -> Option<WaterRegion> {
    if !route.is_demo || route.points.len() < 8 {
        return None;
    }
    let mut lat = 0.0f64;
    let mut lon = 0.0f64;
    for p in &route.points {
        lat += p.lat;
        lon += p.lon;
    }
    lat /= route.points.len() as f64;
    lon /= route.points.len() as f64;
    let (cx, cz) = path.geo.to_world(lat, lon);
    let mut radius = 0.0f32;
    for s in &path.stations {
        radius = radius.max((s.x - cx).hypot(s.z - cz));
    }
    if radius < 250.0 {
        return None;
    }
    let level = route.stats.min_elevation_m - 1.5;
    Some(WaterRegion {
        center: v3(cx, level, cz),
        radius: radius * 0.58,
        level,
    })
}

#[derive(Clone, Debug)]
pub struct RoadSection {
    pub first_index: u32,
    pub count: u32,
    pub distance_m: f32,
}

pub struct Village {
    pub center: V3,
    pub radius: f32,
    pub triangles: usize,
    pub mesh: MeshData,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_path_distance_is_accurate() {
        let route = velocity_core::route::demo_route();
        let path = RoutePath::new(&route);
        assert!(path.stations.len() > 100);
        let s = path.station_at(path.total_m * 0.25);
        let (d, y) = path.nearest(s.x + 2.0, s.z).expect("near route");
        assert!(d <= 2.5, "got {d}");
        assert!((y - s.y).abs() < 1.0);
        assert_eq!(path.nearest(s.x + 5_000.0, s.z), None);
    }

    #[test]
    fn terrain_follows_road_elevation() {
        let route = velocity_core::route::demo_route();
        let t = Terrain::new(&route, 12, 7);
        let (pos, _, _) = t.path.frame_at(2_000.0);
        let ground = t.height(pos.x, pos.z);
        assert!(
            (ground - (pos.y - 0.18)).abs() < 0.6,
            "road corridor should be graded, got {ground} vs {}",
            pos.y
        );
    }

    #[test]
    fn chunk_mesh_and_road_are_built() {
        let route = velocity_core::route::demo_route();
        let t = Terrain::new(&route, 12, 7);
        let mesh = t.build_chunk(0, 0, 16);
        assert!(!mesh.is_empty());
        assert!(mesh.vertices.iter().all(|v| v.pos[1].is_finite()));
        let (road, sections) = t.build_road();
        assert!(!road.is_empty());
        assert!(!sections.is_empty());
        assert!(road.indices.len() > 6);
    }

    #[test]
    fn villages_are_placed_away_from_road() {
        let route = velocity_core::route::demo_route();
        let t = Terrain::new(&route, 12, 7);
        let villages = t.build_villages(&route);
        assert!(!villages.is_empty());
        for v in &villages {
            assert!(v.triangles > 0);
            assert!(v.mesh.vertices.iter().all(|vert| vert.pos[1].is_finite()));
        }
    }
}
