//! Activity file formats: GPX, TCX and FIT import/export.

pub mod fit;
pub mod gpx;
pub mod tcx;

use crate::types::{Activity, DataOrigin, Sample, Sport, now_ms};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Gpx,
    Tcx,
    Fit,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Gpx => "gpx",
            Self::Tcx => "tcx",
            Self::Fit => "fit",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Gpx => "GPX",
            Self::Tcx => "TCX",
            Self::Fit => "FIT",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ImportedActivity {
    pub name: String,
    pub sport: Sport,
    pub samples: Vec<Sample>,
    pub source: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("unsupported file format: {0}")]
    UnsupportedFormat(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("XML parse error: {0}")]
    Xml(String),
    #[error("FIT parse error: {0}")]
    Fit(String),
    #[error("no track points found")]
    NoPoints,
    #[error("file contains no timestamps; import it as a route instead")]
    NoTimestamps,
    #[error("{0}")]
    Other(String),
}

#[derive(Clone, Debug, Default)]
pub struct RawPoint {
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub elevation_m: Option<f32>,
    pub time_ms: Option<i64>,
    pub heart_rate_bpm: Option<f32>,
    pub cadence_rpm: Option<f32>,
    pub power_w: Option<f32>,
    pub speed_mps: Option<f32>,
    pub distance_m: Option<f64>,
}

impl RawPoint {
    pub fn has_position(&self) -> bool {
        self.lat.is_some() && self.lon.is_some()
    }
}

/// Determine the format from the extension, then the file signature.
pub fn detect_format(path: &Path, bytes: &[u8]) -> Option<ExportFormat> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("gpx") => return Some(ExportFormat::Gpx),
        Some("tcx") => return Some(ExportFormat::Tcx),
        Some("fit") => return Some(ExportFormat::Fit),
        _ => {}
    }
    if bytes.len() > 12 && &bytes[8..12] == b".FIT" {
        return Some(ExportFormat::Fit);
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]);
    if head.contains("<gpx") {
        Some(ExportFormat::Gpx)
    } else if head.contains("<TrainingCenterDatabase") {
        Some(ExportFormat::Tcx)
    } else {
        None
    }
}

pub fn import_activity_file(path: impl AsRef<Path>) -> Result<ImportedActivity, ImportError> {
    let path = path.as_ref();
    let bytes = std::fs::read(path)?;
    let Some(format) = detect_format(path, &bytes) else {
        return Err(ImportError::UnsupportedFormat(
            path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("unknown")
                .to_string(),
        ));
    };
    import_activity_bytes(&path.display().to_string(), &bytes, format)
}

pub fn import_activity_bytes(
    name: &str,
    bytes: &[u8],
    format: ExportFormat,
) -> Result<ImportedActivity, ImportError> {
    match format {
        ExportFormat::Gpx => {
            let doc = std::str::from_utf8(bytes)
                .map_err(|e| ImportError::Xml(e.to_string()))?;
            let parsed = gpx::parse(doc)?;
            if parsed.points.iter().all(|p| p.time_ms.is_none()) {
                return Err(ImportError::NoTimestamps);
            }
            finish_import(name, parsed.name, Sport::Cycling, parsed.points, "GPX import")
        }
        ExportFormat::Tcx => {
            let doc = std::str::from_utf8(bytes)
                .map_err(|e| ImportError::Xml(e.to_string()))?;
            let parsed = tcx::parse(doc)?;
            let mut imported = finish_import(
                name,
                parsed.name,
                parsed.sport,
                parsed.points,
                "TCX import",
            )?;
            imported.warnings.extend(parsed.warnings);
            Ok(imported)
        }
        ExportFormat::Fit => {
            let parsed = fit::parse(bytes)?;
            finish_import(name, parsed.name, parsed.sport, parsed.points, "FIT import")
        }
    }
}

/// Shared normalization: timestamps, elapsed time, distance and derived speed.
pub fn finish_import(
    fallback_name: &str,
    file_name: Option<String>,
    sport: Sport,
    mut points: Vec<RawPoint>,
    source: &str,
) -> Result<ImportedActivity, ImportError> {
    points.retain(|p| p.has_position() || p.distance_m.is_some());
    if points.len() < 2 {
        return Err(ImportError::NoPoints);
    }
    points.sort_by_key(|p| p.time_ms.unwrap_or(i64::MAX));
    let with_time: Vec<bool> = points.iter().map(|p| p.time_ms.is_some()).collect();
    if !with_time.iter().any(|v| *v) {
        return Err(ImportError::NoTimestamps);
    }

    let start = points.iter().filter_map(|p| p.time_ms).min().unwrap_or_else(now_ms);
    let mut warnings = Vec::new();
    let mut samples = Vec::with_capacity(points.len());
    let mut distance = 0.0f64;
    let mut prev_pos: Option<(f64, f64)> = None;
    let mut prev_time: Option<i64> = None;
    let mut missing_time_filled = 0usize;

    for (i, p) in points.iter().enumerate() {
        let t_ms = match p.time_ms {
            Some(t) => t,
            None => {
                missing_time_filled += 1;
                prev_time.map(|t| t + 1000).unwrap_or(start + i as i64 * 1000)
            }
        };
        let elapsed = ((t_ms - start) as f64 / 1000.0).max(0.0);

        // Derive speed from the previous position before advancing `prev_pos`.
        let derived_speed = match (prev_time, p.time_ms, prev_pos) {
            (Some(pt), Some(t), Some((plat, plon))) if t > pt && p.speed_mps.is_none() => {
                if let (Some(lat), Some(lon)) = (p.lat, p.lon) {
                    let d = haversine_m(plat, plon, lat, lon);
                    Some((d / ((t - pt) as f64 / 1000.0)) as f32)
                } else {
                    None
                }
            }
            _ => None,
        };

        if let (Some(lat), Some(lon)) = (p.lat, p.lon) {
            if let Some((plat, plon)) = prev_pos {
                let d = haversine_m(plat, plon, lat, lon);
                if d < 2_000.0 {
                    distance += d;
                }
            }
            prev_pos = Some((lat, lon));
        } else if let Some(d) = p.distance_m {
            distance = distance.max(d);
        }

        samples.push(Sample {
            t_ms,
            elapsed_s: elapsed,
            lat: p.lat,
            lon: p.lon,
            altitude_m: p.elevation_m,
            distance_m: distance,
            speed_mps: p.speed_mps.or(derived_speed),
            power_w: p.power_w,
            cadence_rpm: p.cadence_rpm,
            heart_rate_bpm: p.heart_rate_bpm,
            grade_pct: None,
            sim_flags: 0,
            origin: DataOrigin::FileImport,
        });
        prev_time = Some(t_ms);
    }

    // Speed from distance when GPS was unavailable.
    if samples.iter().all(|s| s.speed_mps.is_none()) {
        for w in 0..samples.len() {
            let (d, t) = if w == 0 {
                let next = &samples[1];
                (next.distance_m, next.elapsed_s)
            } else {
                (
                    samples[w].distance_m - samples[w - 1].distance_m,
                    samples[w].elapsed_s - samples[w - 1].elapsed_s,
                )
            };
            if t > 0.0 && d >= 0.0 {
                samples[w].speed_mps = Some((d / t) as f32);
            }
        }
    }
    // Estimate gradient from elevation when missing.
    fill_gradients(&mut samples);

    if missing_time_filled > 0 {
        warnings.push(format!(
            "{missing_time_filled} points had no timestamp and were spaced at 1 s"
        ));
    }
    if samples.iter().any(|s| s.power_w.is_none()) && samples.iter().any(|s| s.power_w.is_some()) {
        warnings.push("power data is incomplete".to_string());
    }

    let name = file_name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| fallback_name.to_string());
    let name = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&name)
        .trim_end_matches(".gpx")
        .trim_end_matches(".tcx")
        .trim_end_matches(".fit")
        .to_string();

    Ok(ImportedActivity {
        name,
        sport,
        samples,
        source: source.to_string(),
        warnings,
    })
}

fn fill_gradients(samples: &mut [Sample]) {
    if samples.iter().all(|s| s.grade_pct.is_some()) {
        return;
    }
    let n = samples.len();
    let mut idx = 0usize;
    for i in 0..n {
        let d = samples[i].distance_m;
        while idx + 1 < n && samples[idx + 1].distance_m < d - 30.0 {
            idx += 1;
        }
        let mut j = idx;
        while j + 1 < n && samples[j + 1].distance_m <= d + 30.0 {
            j += 1;
        }
        if let (Some(a), Some(b)) = (samples[idx].altitude_m, samples[j].altitude_m) {
            let dd = (samples[j].distance_m - samples[idx].distance_m).max(1.0);
            samples[i].grade_pct = Some((((b - a) as f64 / dd) * 100.0) as f32);
        }
    }
}

pub fn export_activity(
    activity: &Activity,
    samples: &[Sample],
    format: ExportFormat,
) -> Result<Vec<u8>, ImportError> {
    match format {
        ExportFormat::Gpx => Ok(gpx::write(activity, samples).into_bytes()),
        ExportFormat::Tcx => Ok(tcx::write(activity, samples).into_bytes()),
        ExportFormat::Fit => fit::write(activity, samples).map_err(|e| ImportError::Fit(e.to_string())),
    }
}

pub fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp * 0.5).sin().powi(2) + p1.cos() * p2.cos() * (dl * 0.5).sin().powi(2);
    2.0 * 6_371_008.8 * a.sqrt().asin()
}

pub(crate) fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(crate) use xml_escape as escape;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_gpx_import_derives_distance_and_speed() {
        let gpx = r#"<?xml version="1.0"?>
        <gpx version="1.1" creator="test" xmlns="http://www.topografix.com/GPX/1/1">
          <trk><name>Loop</name><trkseg>
            <trkpt lat="48.0000" lon="11.0000"><ele>500</ele><time>2026-01-01T10:00:00Z</time></trkpt>
            <trkpt lat="48.0010" lon="11.0000"><ele>510</ele><time>2026-01-01T10:00:20Z</time></trkpt>
            <trkpt lat="48.0020" lon="11.0000"><ele>520</ele><time>2026-01-01T10:00:40Z</time></trkpt>
          </trkseg></trk>
        </gpx>"#;
        let imported = import_activity_bytes("x.gpx", gpx.as_bytes(), ExportFormat::Gpx).unwrap();
        assert_eq!(imported.name, "Loop");
        assert_eq!(imported.samples.len(), 3);
        assert!((imported.samples[2].distance_m - 222.0).abs() < 5.0);
        assert!(imported.samples[1].speed_mps.unwrap() > 4.0);
        assert_eq!(imported.samples[0].origin, DataOrigin::FileImport);
    }

    #[test]
    fn gpx_without_time_is_rejected_with_hint() {
        let gpx = r#"<gpx><trk><trkseg>
            <trkpt lat="48.0" lon="11.0"><ele>1</ele></trkpt>
            <trkpt lat="48.1" lon="11.1"><ele>2</ele></trkpt>
        </trkseg></trk></gpx>"#;
        let err = import_activity_bytes("x.gpx", gpx.as_bytes(), ExportFormat::Gpx).unwrap_err();
        assert!(matches!(err, ImportError::NoTimestamps));
    }

    #[test]
    fn gpx_export_roundtrip() {
        let samples: Vec<Sample> = (0..5)
            .map(|i| Sample {
                t_ms: 1_767_000_000_000 + i * 1000,
                elapsed_s: i as f64,
                lat: Some(48.0 + i as f64 * 0.0001),
                lon: Some(11.0),
                altitude_m: Some(500.0 + i as f32),
                distance_m: i as f64 * 11.0,
                speed_mps: Some(7.0),
                heart_rate_bpm: Some(140.0),
                cadence_rpm: Some(90.0),
                power_w: Some(200.0),
                ..Default::default()
            })
            .collect();
        let activity = Activity {
            id: 1,
            name: "Ride <Test>".into(),
            sport: Sport::Cycling,
            started_at_ms: 1_767_000_000_000,
            ended_at_ms: Some(1_767_000_005_000),
            summary: Default::default(),
            route_id: None,
            route_name: None,
            source: "test".into(),
            notes: String::new(),
            created_at_ms: 0,
        };
        let gpx = export_activity(&activity, &samples, ExportFormat::Gpx).unwrap();
        let imp = import_activity_bytes("a.gpx", &gpx, ExportFormat::Gpx).unwrap();
        assert_eq!(imp.samples.len(), 5);
        assert!(imp.name.contains("Ride"));
        let tcx = export_activity(&activity, &samples, ExportFormat::Tcx).unwrap();
        let imp = import_activity_bytes("a.tcx", &tcx, ExportFormat::Tcx).unwrap();
        assert_eq!(imp.samples.len(), 5);
        assert_eq!(imp.samples[2].heart_rate_bpm, Some(140.0));
    }
}
