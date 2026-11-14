//! GPX 1.1 reader/writer with common extension support.

use super::{ImportError, RawPoint, escape};
use crate::types::{Activity, Sample};
use chrono::{DateTime, SecondsFormat, Utc};

#[derive(Debug, Default)]
pub struct ParsedGpx {
    pub name: Option<String>,
    pub points: Vec<RawPoint>,
}

pub fn parse(xml: &str) -> Result<ParsedGpx, ImportError> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| ImportError::Xml(e.to_string()))?;
    let mut name = doc
        .descendants()
        .find(|n| n.has_tag_name("trk"))
        .and_then(|trk| trk.children().find(|c| c.has_tag_name("name")))
        .and_then(|n| n.text())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if name.is_none() {
        name = doc
            .root_element()
            .children()
            .find(|n| n.has_tag_name("metadata"))
            .and_then(|m| m.children().find(|c| c.has_tag_name("name")))
            .and_then(|n| n.text())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
    }

    let mut points = Vec::new();
    for node in doc
        .descendants()
        .filter(|n| n.has_tag_name("trkpt") || n.has_tag_name("rtept"))
    {
        let (Some(lat), Some(lon)) = (
            node.attribute("lat").and_then(|v| v.parse::<f64>().ok()),
            node.attribute("lon").and_then(|v| v.parse::<f64>().ok()),
        ) else {
            continue;
        };
        let elevation_m = child_f32(&node, "ele");
        let time_ms = child_text(&node, "time")
            .and_then(|s| DateTime::parse_from_rfc3339(s.trim()).ok())
            .map(|dt| dt.timestamp_millis());
        let extensions: Vec<roxmltree::Node<'_, '_>> = node.descendants().collect();
        let find_ext = |names: &[&str]| {
            extensions
                .iter()
                .find(|n| n.is_element() && names.iter().any(|name| n.has_tag_name(*name)))
                .and_then(|n| n.text())
                .and_then(|t| t.trim().parse::<f32>().ok())
        };
        points.push(RawPoint {
            lat: Some(lat),
            lon: Some(lon),
            elevation_m,
            time_ms,
            heart_rate_bpm: find_ext(&["hr", "heartrate", "heart_rate"]),
            cadence_rpm: find_ext(&["cad", "cadence"]),
            power_w: find_ext(&["power", "watts", "PowerInWatts"]),
            speed_mps: find_ext(&["speed"]),
            distance_m: child_f64(&node, "distance"),
        });
    }

    if points.len() < 2 {
        return Err(ImportError::NoPoints);
    }
    Ok(ParsedGpx { name, points })
}

fn child_text<'a>(node: &roxmltree::Node<'a, 'a>, name: &str) -> Option<&'a str> {
    node.children()
        .find(|c| c.is_element() && c.has_tag_name(name))
        .and_then(|n| n.text())
}

fn child_f32(node: &roxmltree::Node<'_, '_>, name: &str) -> Option<f32> {
    child_text(node, name).and_then(|t| t.trim().parse().ok())
}

fn child_f64(node: &roxmltree::Node<'_, '_>, name: &str) -> Option<f64> {
    child_text(node, name).and_then(|t| t.trim().parse().ok())
}

pub fn write(activity: &Activity, samples: &[Sample]) -> String {
    let mut out = String::with_capacity(samples.len() * 260 + 1024);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(
        "<gpx version=\"1.1\" creator=\"VELOCITY\" \
         xmlns=\"http://www.topografix.com/GPX/1/1\" \
         xmlns:gpxtpx=\"http://www.garmin.com/xmlschemas/TrackPointExtension/v1\" \
         xmlns:pwr=\"http://www.garmin.com/xmlschemas/PowerExtension/v1\">\n",
    );
    out.push_str(&format!(
        "  <metadata>\n    <name>{}</name>\n    <time>{}</time>\n  </metadata>\n",
        escape(&activity.name),
        format_time(activity.started_at_ms)
    ));
    out.push_str(&format!(
        "  <trk>\n    <name>{}</name>\n    <type>{}</type>\n    <trkseg>\n",
        escape(&activity.name),
        activity.sport.key()
    ));
    for s in samples {
        let (Some(lat), Some(lon)) = (s.lat, s.lon) else {
            continue;
        };
        out.push_str(&format!("      <trkpt lat=\"{lat:.7}\" lon=\"{lon:.7}\">\n"));
        if let Some(ele) = s.altitude_m {
            out.push_str(&format!("        <ele>{ele:.1}</ele>\n"));
        }
        out.push_str(&format!("        <time>{}</time>\n", format_time(s.t_ms)));
        if let Some(hr) = s.heart_rate_bpm {
            out.push_str(&format!("        <extensions><gpxtpx:TrackPointExtension><gpxtpx:hr>{hr:.0}</gpxtpx:hr>"));
            if let Some(cad) = s.cadence_rpm {
                out.push_str(&format!("<gpxtpx:cad>{cad:.0}</gpxtpx:cad>"));
            }
            out.push_str("</gpxtpx:TrackPointExtension>");
            if let Some(power) = s.power_w {
                out.push_str(&format!("<pwr:PowerInWatts>{power:.0}</pwr:PowerInWatts>"));
            }
            out.push_str("</extensions>\n");
        } else if let Some(power) = s.power_w {
            out.push_str(&format!(
                "        <extensions><pwr:PowerInWatts>{power:.0}</pwr:PowerInWatts></extensions>\n"
            ));
        }
        out.push_str("      </trkpt>\n");
    }
    out.push_str("    </trkseg>\n  </trk>\n</gpx>\n");
    out
}

fn format_time(ms: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(ms)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap())
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_extensions() {
        let xml = r#"<gpx xmlns="http://www.topografix.com/GPX/1/1"
            xmlns:gpxtpx="http://www.garmin.com/xmlschemas/TrackPointExtension/v1"
            xmlns:pwr="http://www.garmin.com/xmlschemas/PowerExtension/v1">
          <trk><name>With data</name><trkseg>
            <trkpt lat="48.1" lon="11.2"><ele>610</ele><time>2026-02-01T08:00:00Z</time>
              <extensions><gpxtpx:TrackPointExtension><gpxtpx:hr>142</gpxtpx:hr><gpxtpx:cad>88</gpxtpx:cad></gpxtpx:TrackPointExtension>
              <pwr:PowerInWatts>215</pwr:PowerInWatts></extensions>
            </trkpt>
            <trkpt lat="48.101" lon="11.2"><ele>615</ele><time>2026-02-01T08:00:05Z</time></trkpt>
          </trkseg></trk></gpx>"#;
        let p = parse(xml).unwrap();
        assert_eq!(p.name.as_deref(), Some("With data"));
        assert_eq!(p.points.len(), 2);
        assert_eq!(p.points[0].heart_rate_bpm, Some(142.0));
        assert_eq!(p.points[0].power_w, Some(215.0));
        assert_eq!(p.points[0].cadence_rpm, Some(88.0));
    }

    #[test]
    fn rejects_empty() {
        assert!(parse("<gpx></gpx>").is_err());
    }
}
