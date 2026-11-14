//! TCX (Training Center XML) reader/writer.

use super::{ImportError, RawPoint, escape};
use crate::types::{Activity, Sample, Sport};
use chrono::{DateTime, SecondsFormat, Utc};

#[derive(Debug, Default)]
pub struct ParsedTcx {
    pub name: Option<String>,
    pub sport: Sport,
    pub points: Vec<RawPoint>,
    pub warnings: Vec<String>,
}

pub fn parse(xml: &str) -> Result<ParsedTcx, ImportError> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| ImportError::Xml(e.to_string()))?;
    let activity_node = doc
        .descendants()
        .find(|n| n.has_tag_name("Activity"))
        .ok_or(ImportError::NoPoints)?;
    let sport = match activity_node
        .attribute("Sport")
        .unwrap_or("Biking")
        .to_ascii_lowercase()
        .as_str()
    {
        "running" => Sport::Running,
        "biking" | "cycling" => Sport::Cycling,
        "other" => Sport::Other,
        _ => Sport::Cycling,
    };
    let name = activity_node
        .children()
        .find(|c| c.has_tag_name("Notes"))
        .and_then(|n| n.text())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let mut warnings = Vec::new();
    if !activity_node.descendants().any(|n| n.has_tag_name("Trackpoint")) {
        warnings.push("TCX contains no track points".to_string());
    }

    let mut points = Vec::new();
    for tp in doc.descendants().filter(|n| n.has_tag_name("Trackpoint")) {
        let time_ms = child_text(&tp, "Time")
            .and_then(|t| DateTime::parse_from_rfc3339(t.trim()).ok())
            .map(|d| d.timestamp_millis());
        let lat = tp
            .descendants()
            .find(|n| n.has_tag_name("LatitudeDegrees"))
            .and_then(|n| n.text())
            .and_then(|t| t.trim().parse::<f64>().ok());
        let lon = tp
            .descendants()
            .find(|n| n.has_tag_name("LongitudeDegrees"))
            .and_then(|n| n.text())
            .and_then(|t| t.trim().parse::<f64>().ok());
        let elevation_m = tp
            .descendants()
            .find(|n| n.has_tag_name("AltitudeMeters"))
            .and_then(|n| n.text())
            .and_then(|t| t.trim().parse::<f32>().ok());
        let distance_m = tp
            .descendants()
            .find(|n| n.has_tag_name("DistanceMeters"))
            .and_then(|n| n.text())
            .and_then(|t| t.trim().parse::<f64>().ok());
        let heart_rate_bpm = tp
            .descendants()
            .find(|n| n.has_tag_name("HeartRateBpm"))
            .and_then(|hr| hr.children().find(|c| c.has_tag_name("Value")))
            .and_then(|n| n.text())
            .and_then(|t| t.trim().parse::<f32>().ok());
        let cadence_rpm = tp
            .descendants()
            .find(|n| n.has_tag_name("Cadence"))
            .and_then(|n| n.text())
            .and_then(|t| t.trim().parse::<f32>().ok());
        let speed_mps = tp
            .descendants()
            .find(|n| n.has_tag_name("Speed"))
            .and_then(|n| n.text())
            .and_then(|t| t.trim().parse::<f32>().ok());
        let power_w = tp
            .descendants()
            .find(|n| n.has_tag_name("Watts"))
            .and_then(|n| n.text())
            .and_then(|t| t.trim().parse::<f32>().ok());

        if lat.is_some() && lon.is_some() || distance_m.is_some() || time_ms.is_some() {
            points.push(RawPoint {
                lat,
                lon,
                elevation_m,
                time_ms,
                heart_rate_bpm,
                cadence_rpm,
                power_w,
                speed_mps,
                distance_m,
            });
        }
    }

    if points.len() < 2 {
        return Err(ImportError::NoPoints);
    }
    Ok(ParsedTcx {
        name,
        sport,
        points,
        warnings,
    })
}

fn child_text<'a>(node: &roxmltree::Node<'a, 'a>, name: &str) -> Option<&'a str> {
    node.children()
        .find(|c| c.is_element() && c.has_tag_name(name))
        .and_then(|n| n.text())
}

pub fn write(activity: &Activity, samples: &[Sample]) -> String {
    let mut out = String::with_capacity(samples.len() * 260 + 2048);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<TrainingCenterDatabase xmlns=\"http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2\">\n");
    out.push_str("  <Activities>\n");
    let sport = match activity.sport {
        Sport::Running => "Running",
        Sport::Other => "Other",
        _ => "Biking",
    };
    out.push_str(&format!("    <Activity Sport=\"{sport}\">\n"));
    out.push_str(&format!(
        "      <Id>{}</Id>\n",
        format_time(activity.started_at_ms)
    ));
    if !activity.notes.is_empty() {
        out.push_str(&format!("      <Notes>{}</Notes>\n", escape(&activity.notes)));
    }
    out.push_str("      <Lap>\n");
    out.push_str(&format!(
        "        <TotalTimeSeconds>{:.1}</TotalTimeSeconds>\n",
        activity.summary.duration_s
    ));
    out.push_str(&format!(
        "        <DistanceMeters>{:.1}</DistanceMeters>\n",
        activity.summary.distance_m
    ));
    out.push_str(&format!(
        "        <Calories>{:.0}</Calories>\n",
        activity.summary.calories_kcal.unwrap_or(0.0)
    ));
    out.push_str(&format!(
        "        <Intensity>{}%</Intensity>\n",
        (activity.summary.intensity_factor.unwrap_or(0.75) * 100.0).round() as u32
    ));
    out.push_str("        <TriggerMethod>Manual</TriggerMethod>\n");
    out.push_str("        <Track>\n");
    for s in samples {
        out.push_str("          <Trackpoint>\n");
        out.push_str(&format!("            <Time>{}</Time>\n", format_time(s.t_ms)));
        if let (Some(lat), Some(lon)) = (s.lat, s.lon) {
            out.push_str(&format!(
                "            <Position><LatitudeDegrees>{lat:.7}</LatitudeDegrees><LongitudeDegrees>{lon:.7}</LongitudeDegrees></Position>\n"
            ));
        }
        if let Some(ele) = s.altitude_m {
            out.push_str(&format!("            <AltitudeMeters>{ele:.1}</AltitudeMeters>\n"));
        }
        out.push_str(&format!(
            "            <DistanceMeters>{:.1}</DistanceMeters>\n",
            s.distance_m
        ));
        if let Some(hr) = s.heart_rate_bpm {
            out.push_str(&format!(
                "            <HeartRateBpm><Value>{hr:.0}</Value></HeartRateBpm>\n"
            ));
        }
        if let Some(cad) = s.cadence_rpm {
            out.push_str(&format!("            <Cadence>{cad:.0}</Cadence>\n"));
        }
        if s.speed_mps.is_some() || s.power_w.is_some() {
            out.push_str("            <Extensions><TPX xmlns=\"http://www.garmin.com/xmlschemas/ActivityExtension/v2\">\n");
            if let Some(speed) = s.speed_mps {
                out.push_str(&format!("              <Speed>{speed:.3}</Speed>\n"));
            }
            if let Some(power) = s.power_w {
                out.push_str(&format!("              <Watts>{power:.0}</Watts>\n"));
            }
            out.push_str("            </TPX></Extensions>\n");
        }
        out.push_str("          </Trackpoint>\n");
    }
    out.push_str("        </Track>\n      </Lap>\n    </Activity>\n  </Activities>\n</TrainingCenterDatabase>\n");
    out
}

fn format_time(ms: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(ms)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap())
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_tcx() {
        let tcx = r#"<?xml version="1.0"?>
        <TrainingCenterDatabase xmlns="http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2">
          <Activities><Activity Sport="Biking"><Id>2026-02-01T08:00:00Z</Id><Lap><Track>
            <Trackpoint><Time>2026-02-01T08:00:00Z</Time>
              <Position><LatitudeDegrees>48.1</LatitudeDegrees><LongitudeDegrees>11.2</LongitudeDegrees></Position>
              <AltitudeMeters>600</AltitudeMeters><DistanceMeters>0</DistanceMeters>
              <HeartRateBpm><Value>130</Value></HeartRateBpm><Cadence>85</Cadence>
              <Extensions><TPX xmlns="http://www.garmin.com/xmlschemas/ActivityExtension/v2"><Speed>7.5</Speed><Watts>180</Watts></TPX></Extensions>
            </Trackpoint>
            <Trackpoint><Time>2026-02-01T08:00:10Z</Time>
              <Position><LatitudeDegrees>48.1005</LatitudeDegrees><LongitudeDegrees>11.2</LongitudeDegrees></Position>
              <DistanceMeters>55</DistanceMeters><HeartRateBpm><Value>135</Value></HeartRateBpm>
            </Trackpoint>
          </Track></Lap></Activity></Activities>
        </TrainingCenterDatabase>"#;
        let p = parse(tcx).unwrap();
        assert_eq!(p.sport, Sport::Cycling);
        assert_eq!(p.points.len(), 2);
        assert_eq!(p.points[0].power_w, Some(180.0));
        assert_eq!(p.points[1].heart_rate_bpm, Some(135.0));
    }
}
