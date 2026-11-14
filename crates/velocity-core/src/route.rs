//! Route model: GPX import, distance/elevation profile, gradient lookup and
//! position interpolation for the virtual world and trainer resistance.

use crate::types::fmt_distance_km;
use serde::{Deserialize, Serialize};

const EARTH_RADIUS_M: f64 = 6_371_008.8;
const GRADE_WINDOW_M: f64 = 25.0;
const ELEVATION_SMOOTH_M: f64 = 40.0;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RoutePoint {
    pub lat: f64,
    pub lon: f64,
    pub elevation_m: f32,
    pub distance_m: f64,
    pub grade_pct: f32,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct RoutePosition {
    pub lat: f64,
    pub lon: f64,
    pub elevation_m: f32,
    pub grade_pct: f32,
    pub distance_m: f64,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct ProfilePoint {
    pub distance_m: f64,
    pub elevation_m: f32,
    pub grade_pct: f32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RouteStats {
    pub distance_m: f64,
    pub elevation_gain_m: f32,
    pub elevation_loss_m: f32,
    pub min_elevation_m: f32,
    pub max_elevation_m: f32,
    pub avg_grade_pct: f32,
    pub max_grade_pct: f32,
}

impl RouteStats {
    pub fn difficulty(&self) -> &'static str {
        // Simple climb-oriented score, similar in spirit to common cycling scales.
        let score = self.distance_m / 1000.0 + (self.elevation_gain_m as f64 / 100.0) * 1.5;
        if score < 20.0 {
            "Easy"
        } else if score < 45.0 {
            "Moderate"
        } else if score < 80.0 {
            "Hard"
        } else {
            "Epic"
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Route {
    pub name: String,
    pub description: String,
    pub points: Vec<RoutePoint>,
    pub stats: RouteStats,
    /// True when the geometry is bundled demo material, not a real-world track.
    pub is_demo: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum RouteError {
    #[error("not enough track points (need at least 2)")]
    TooFewPoints,
    #[error("invalid coordinate: {0}")]
    InvalidCoordinate(String),
    #[error("GPX parse error: {0}")]
    Xml(String),
    #[error("no track points found in GPX")]
    NoTrackPoints,
    #[error("I/O error: {0}")]
    Io(String),
}

impl Route {
    pub fn from_track(
        name: impl Into<String>,
        track: Vec<(f64, f64, Option<f32>)>,
    ) -> Result<Self, RouteError> {
        if track.len() < 2 {
            return Err(RouteError::TooFewPoints);
        }
        for (lat, lon, _) in &track {
            if !(-90.0..=90.0).contains(lat) || !(-180.0..=180.0).contains(lon) {
                return Err(RouteError::InvalidCoordinate(format!("{lat},{lon}")));
            }
        }

        let mut points: Vec<RoutePoint> = Vec::with_capacity(track.len());
        let mut distance = 0.0f64;
        for (i, (lat, lon, ele)) in track.iter().enumerate() {
            if i > 0 {
                let (plat, plon) = (track[i - 1].0, track[i - 1].1);
                distance += haversine_m(plat, plon, *lat, *lon);
            }
            points.push(RoutePoint {
                lat: *lat,
                lon: *lon,
                elevation_m: ele.unwrap_or(0.0),
                distance_m: distance,
                grade_pct: 0.0,
            });
        }

        smooth_elevation(&mut points);
        compute_grades(&mut points);
        let stats = compute_stats(&points);

        Ok(Self {
            name: name.into(),
            description: String::new(),
            points,
            stats,
            is_demo: false,
        })
    }

    pub fn from_gpx(gpx: &str, fallback_name: &str) -> Result<Self, RouteError> {
        let doc = roxmltree::Document::parse(gpx).map_err(|e| RouteError::Xml(e.to_string()))?;
        let mut name = fallback_name.to_string();
        if let Some(node) = doc
            .descendants()
            .find(|n| n.has_tag_name("trk") || n.has_tag_name("rte"))
        {
            if let Some(n) = node.children().find(|c| c.has_tag_name("name")) {
                if let Some(text) = n.text() {
                    if !text.trim().is_empty() {
                        name = text.trim().to_string();
                    }
                }
            }
        }

        let mut track = Vec::new();
        for node in doc
            .descendants()
            .filter(|n| n.has_tag_name("trkpt") || n.has_tag_name("rtept"))
        {
            let Some(lat) = node.attribute("lat").and_then(|s| s.parse::<f64>().ok()) else {
                continue;
            };
            let Some(lon) = node.attribute("lon").and_then(|s| s.parse::<f64>().ok()) else {
                continue;
            };
            let ele = node
                .children()
                .find(|c| c.has_tag_name("ele"))
                .and_then(|n| n.text())
                .and_then(|t| t.trim().parse::<f32>().ok());
            track.push((lat, lon, ele));
        }

        if track.len() < 2 {
            return Err(RouteError::NoTrackPoints);
        }
        Self::from_track(name, track)
    }

    pub fn is_empty(&self) -> bool {
        self.points.len() < 2
    }

    pub fn total_distance_m(&self) -> f64 {
        self.stats.distance_m
    }

    pub fn duration_label(&self) -> String {
        fmt_distance_km(self.stats.distance_m)
    }

    /// Linearly interpolated position at a distance along the route.
    pub fn position_at(&self, distance_m: f64) -> RoutePosition {
        if self.points.is_empty() {
            return RoutePosition::default();
        }
        let d = distance_m.clamp(0.0, self.stats.distance_m);
        let idx = match self
            .points
            .binary_search_by(|p| p.distance_m.total_cmp(&d))
        {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) if i >= self.points.len() => self.points.len() - 1,
            Err(i) => i,
        };
        let b = &self.points[idx];
        let a = if idx > 0 { &self.points[idx - 1] } else { b };
        let span = (b.distance_m - a.distance_m).max(1e-6);
        let t = ((d - a.distance_m) / span).clamp(0.0, 1.0) as f32;
        RoutePosition {
            lat: a.lat + (b.lat - a.lat) * t as f64,
            lon: a.lon + (b.lon - a.lon) * t as f64,
            elevation_m: a.elevation_m + (b.elevation_m - a.elevation_m) * t,
            grade_pct: a.grade_pct + (b.grade_pct - a.grade_pct) * t,
            distance_m: d,
        }
    }

    /// Evenly spaced profile used by charts and the 3D world.
    pub fn profile(&self, samples: usize) -> Vec<ProfilePoint> {
        if self.points.len() < 2 || samples < 2 {
            return Vec::new();
        }
        let total = self.stats.distance_m.max(1.0);
        (0..samples)
            .map(|i| {
                let d = total * i as f64 / (samples - 1) as f64;
                let p = self.position_at(d);
                ProfilePoint {
                    distance_m: d,
                    elevation_m: p.elevation_m,
                    grade_pct: p.grade_pct,
                }
            })
            .collect()
    }

    /// Gradient at a distance, smoothed over a small window.
    pub fn grade_at(&self, distance_m: f64) -> f32 {
        self.position_at(distance_m).grade_pct
    }

    pub fn to_gpx(&self) -> String {
        let mut out = String::with_capacity(self.points.len() * 90 + 512);
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        out.push_str("<gpx version=\"1.1\" creator=\"VELOCITY\" xmlns=\"http://www.topografix.com/GPX/1/1\">\n");
        out.push_str(&format!("  <trk>\n    <name>{}</name>\n    <trkseg>\n", xml_escape(&self.name)));
        for p in &self.points {
            out.push_str(&format!(
                "      <trkpt lat=\"{:.7}\" lon=\"{:.7}\"><ele>{:.1}</ele></trkpt>\n",
                p.lat, p.lon, p.elevation_m
            ));
        }
        out.push_str("    </trkseg>\n  </trk>\n</gpx>\n");
        out
    }
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp * 0.5).sin().powi(2) + p1.cos() * p2.cos() * (dl * 0.5).sin().powi(2);
    2.0 * EARTH_RADIUS_M * a.sqrt().asin()
}

/// Moving-average elevation smoothing over a distance window. This removes
/// GPS/DEM noise without flattening real climbs.
fn smooth_elevation(points: &mut [RoutePoint]) {
    if points.len() < 3 {
        return;
    }
    let raw: Vec<f32> = points.iter().map(|p| p.elevation_m).collect();
    let mut lo = 0usize;
    let mut hi = 0usize;
    let mut sum = 0.0f64;
    for i in 0..points.len() {
        let d = points[i].distance_m;
        while lo < points.len() && points[lo].distance_m < d - ELEVATION_SMOOTH_M {
            sum -= raw[lo] as f64;
            lo += 1;
        }
        while hi < points.len() && points[hi].distance_m <= d + ELEVATION_SMOOTH_M {
            sum += raw[hi] as f64;
            hi += 1;
        }
        let count = hi.saturating_sub(lo).max(1);
        points[i].elevation_m = (sum / count as f64) as f32;
    }
}

fn compute_grades(points: &mut [RoutePoint]) {
    if points.is_empty() {
        return;
    }
    let n = points.len();
    let mut idx = 0usize;
    for i in 0..n {
        let d = points[i].distance_m;
        while idx + 1 < n && points[idx + 1].distance_m < d - GRADE_WINDOW_M {
            idx += 1;
        }
        let mut j = idx;
        while j + 1 < n && points[j + 1].distance_m <= d + GRADE_WINDOW_M {
            j += 1;
        }
        let a = &points[idx];
        let b = &points[j];
        let dd = (b.distance_m - a.distance_m).max(1.0);
        let grade = ((b.elevation_m - a.elevation_m) as f64 / dd * 100.0) as f32;
        points[i].grade_pct = grade.clamp(-35.0, 35.0);
    }
}

fn compute_stats(points: &[RoutePoint]) -> RouteStats {
    let mut stats = RouteStats::default();
    if points.len() < 2 {
        return stats;
    }
    stats.distance_m = points.last().map(|p| p.distance_m).unwrap_or(0.0);
    stats.min_elevation_m = f32::INFINITY;
    stats.max_elevation_m = f32::NEG_INFINITY;
    let mut gain = 0.0f32;
    let mut loss = 0.0f32;
    let mut prev = points[0].elevation_m;
    for p in points {
        let delta = p.elevation_m - prev;
        if delta > 0.4 {
            gain += delta;
        } else if delta < -0.4 {
            loss -= delta;
        }
        prev = p.elevation_m;
        stats.min_elevation_m = stats.min_elevation_m.min(p.elevation_m);
        stats.max_elevation_m = stats.max_elevation_m.max(p.elevation_m);
        stats.max_grade_pct = stats.max_grade_pct.max(p.grade_pct.abs());
    }
    stats.elevation_gain_m = gain;
    stats.elevation_loss_m = loss;
    if stats.min_elevation_m.is_infinite() {
        stats.min_elevation_m = 0.0;
    }
    if stats.max_elevation_m.is_infinite() {
        stats.max_elevation_m = 0.0;
    }
    // Average grade excludes descending sections so climbs read sensibly.
    let mut climb = 0.0f32;
    let mut count = 0usize;
    for p in points {
        if p.grade_pct > 0.5 {
            climb += p.grade_pct;
            count += 1;
        }
    }
    stats.avg_grade_pct = if count > 0 { climb / count as f32 } else { 0.0 };
    stats
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A clearly labelled, deterministic demo loop inspired by rolling terrain near
/// a sub-alpine lake. It is not a surveyed track; real rides should import GPX.
pub fn demo_route() -> Route {
    let center_lat = 47.9985;
    let center_lon = 11.3420;
    let n = 720;
    let mut track = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let angle = t * std::f64::consts::TAU;
        // Slightly irregular oval with two climbs.
        let radius = 0.055 * (1.0 + 0.10 * (3.0 * angle).sin());
        let lat = center_lat + radius * angle.sin() * 0.72;
        let lon = center_lon + radius * angle.cos();
        let base = 585.0;
        let hill1 = 120.0 * (-((t - 0.18) / 0.075).powi(2)).exp();
        let hill2 = 190.0 * (-((t - 0.62) / 0.10).powi(2)).exp();
        let ele = base + hill1 + hill2 + 18.0 * (5.0 * angle).sin();
        track.push((lat, lon, Some(ele as f32)));
    }
    let mut route = Route::from_track("Starnberger See · Demo Loop", track)
        .expect("bundled demo route is valid");
    route.description = "Bundled training loop for trying VELOCITY without hardware. \
        Elevation is illustrative; import a GPX file for real-world routes."
        .to_string();
    route.is_demo = true;
    route
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_route_has_sane_stats() {
        let r = demo_route();
        assert!(r.stats.distance_m > 15_000.0);
        assert!(r.stats.elevation_gain_m > 100.0);
        assert!(r.stats.max_elevation_m > r.stats.min_elevation_m);
        assert!(r.is_demo);
        assert!(r.points.iter().any(|p| p.grade_pct.abs() > 1.0));
    }

    #[test]
    fn interpolation_matches_endpoints() {
        let r = demo_route();
        let start = r.position_at(0.0);
        let end = r.position_at(r.stats.distance_m);
        assert!((start.lat - r.points[0].lat).abs() < 1e-9);
        assert!((end.lat - r.points.last().unwrap().lat).abs() < 1e-9);
        let mid = r.position_at(r.stats.distance_m * 0.5);
        assert!(mid.distance_m > 0.0);
    }

    #[test]
    fn gpx_roundtrip() {
        let r = demo_route();
        let gpx = r.to_gpx();
        let r2 = Route::from_gpx(&gpx, "fallback").unwrap();
        assert_eq!(r2.name, r.name);
        assert!((r2.stats.distance_m - r.stats.distance_m).abs() < 1.0);
        assert!(r2.points.iter().any(|p| p.elevation_m > 600.0));
    }

    #[test]
    fn handles_missing_elevation() {
        let r = Route::from_track(
            "flat",
            vec![(48.0, 11.0, None), (48.001, 11.002, None), (48.003, 11.004, None)],
        )
        .unwrap();
        assert_eq!(r.stats.elevation_gain_m, 0.0);
        assert!(r.stats.distance_m > 100.0);
    }

    #[test]
    fn rejects_bad_tracks() {
        assert!(Route::from_track("x", vec![(48.0, 11.0, None)]).is_err());
        assert!(Route::from_track("x", vec![(99.0, 11.0, None), (48.0, 11.0, None)]).is_err());
    }
}
