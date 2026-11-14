//! Minimal, standards-compliant FIT encoder plus import via `fitparser`.

use super::{ImportError, RawPoint};
use crate::types::{Activity, Sample, Sport};

pub struct ParsedFit {
    pub name: Option<String>,
    pub sport: Sport,
    pub points: Vec<RawPoint>,
}

const FIT_EPOCH_OFFSET_S: i64 = 631_065_600; // 1989-12-31T00:00:00Z
const INVALID_U8: u8 = 0xFF;
const INVALID_U16: u16 = 0xFFFF;
const INVALID_U32: u32 = 0xFFFF_FFFF;
const INVALID_S32: i32 = 0x7FFF_FFFF;

pub fn parse(bytes: &[u8]) -> Result<ParsedFit, ImportError> {
    let records = fitparser::from_bytes(bytes).map_err(|e| ImportError::Fit(e.to_string()))?;
    let mut points = Vec::new();
    let mut sport = Sport::Cycling;
    let mut name = None;
    for rec in &records {
        match rec.kind() {
            fitparser::profile::MesgNum::Session => {
                if let Some(s) = field_u8(rec, "sport") {
                    sport = match s {
                        1 => Sport::Running,
                        2 => Sport::Cycling,
                        6 => Sport::VirtualRide,
                        _ => Sport::Cycling,
                    };
                }
                if let Some(sub) = field_u8(rec, "sub_sport") {
                    if sub == 6 {
                        sport = Sport::VirtualRide;
                    }
                }
            }
            fitparser::profile::MesgNum::Record => {
                let time_ms = field_timestamp(rec, "timestamp");
                let lat = field_f64(rec, "position_lat").map(semicircles_to_degrees);
                let lon = field_f64(rec, "position_long").map(semicircles_to_degrees);
                let elevation_m = field_f64(rec, "enhanced_altitude")
                    .or_else(|| field_f64(rec, "altitude"))
                    .map(|v| v as f32);
                let distance_m = field_f64(rec, "distance");
                let speed_mps = field_f64(rec, "enhanced_speed")
                    .or_else(|| field_f64(rec, "speed"))
                    .map(|v| v as f32);
                let power_w = field_f64(rec, "power").map(|v| v as f32);
                let cadence_rpm = field_f64(rec, "cadence").map(|v| v as f32);
                let heart_rate_bpm = field_f64(rec, "heart_rate").map(|v| v as f32);
                if time_ms.is_none()
                    && lat.is_none()
                    && distance_m.is_none()
                    && power_w.is_none()
                    && heart_rate_bpm.is_none()
                {
                    continue;
                }
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
            fitparser::profile::MesgNum::FileId => {
                if let Some(s) = field_string(rec, "name") {
                    name = Some(s);
                }
            }
            _ => {}
        }
    }
    if points.len() < 2 {
        return Err(ImportError::NoPoints);
    }
    Ok(ParsedFit {
        name,
        sport,
        points,
    })
}

fn field_f64(rec: &fitparser::FitDataRecord, name: &str) -> Option<f64> {
    rec.fields()
        .iter()
        .find(|f| f.name() == name)
        .and_then(|f| value_f64(f.value()))
}

fn field_timestamp(rec: &fitparser::FitDataRecord, name: &str) -> Option<i64> {
    rec.fields().iter().find(|f| f.name() == name).and_then(|f| {
        match f.value() {
            fitparser::Value::Timestamp(dt) => Some(dt.timestamp_millis()),
            other => value_f64(other).map(|v| (v as i64 + FIT_EPOCH_OFFSET_S) * 1000),
        }
    })
}

fn field_u8(rec: &fitparser::FitDataRecord, name: &str) -> Option<u8> {
    rec.fields()
        .iter()
        .find(|f| f.name() == name)
        .and_then(|f| value_f64(f.value()))
        .map(|v| v as u8)
}

fn field_string(rec: &fitparser::FitDataRecord, name: &str) -> Option<String> {
    rec.fields()
        .iter()
        .find(|f| f.name() == name)
        .and_then(|f| match f.value() {
            fitparser::Value::String(s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        })
}

fn value_f64(value: &fitparser::Value) -> Option<f64> {
    use fitparser::Value;
    match value {
        Value::Byte(v) => Some(*v as f64),
        Value::Enum(v) => Some(*v as f64),
        Value::SInt8(v) => Some(*v as f64),
        Value::UInt8(v) | Value::UInt8z(v) => Some(*v as f64),
        Value::SInt16(v) => Some(*v as f64),
        Value::UInt16(v) | Value::UInt16z(v) => Some(*v as f64),
        Value::SInt32(v) => Some(*v as f64),
        Value::UInt32(v) | Value::UInt32z(v) => Some(*v as f64),
        Value::SInt64(v) => Some(*v as f64),
        Value::UInt64(v) | Value::UInt64z(v) => Some(*v as f64),
        Value::Float32(v) => Some(*v as f64),
        Value::Float64(v) => Some(*v),
        Value::Array(arr) => arr.first().and_then(value_f64),
        Value::Timestamp(dt) => Some(dt.timestamp_millis() as f64),
        Value::String(_) | Value::Invalid => None,
    }
}

fn semicircles_to_degrees(v: f64) -> f64 {
    v * (180.0 / 2_147_483_648.0)
}

fn degrees_to_semicircles(v: f64) -> i32 {
    (v * (2_147_483_648.0 / 180.0))
        .round()
        .clamp(i32::MIN as f64 + 1.0, i32::MAX as f64) as i32
}

fn unix_ms_to_fit(ms: i64) -> u32 {
    (((ms / 1000) - FIT_EPOCH_OFFSET_S).max(0) as u64).min(u32::MAX as u64) as u32
}

struct FitWriter {
    data: Vec<u8>,
}

impl FitWriter {
    fn new() -> Self {
        Self { data: Vec::new() }
    }

    fn definition(&mut self, local: u8, global: u16, fields: &[(u8, u8, u8)]) {
        self.data.push(0x40 | (local & 0x0F));
        self.data.push(0); // reserved
        self.data.push(0); // little endian
        self.data.extend_from_slice(&global.to_le_bytes());
        self.data.push(fields.len() as u8);
        for (num, size, base) in fields {
            self.data.push(*num);
            self.data.push(*size);
            self.data.push(*base);
        }
    }

    fn message(&mut self, local: u8, payload: &[u8]) {
        self.data.push(local & 0x0F);
        self.data.extend_from_slice(payload);
    }
}

fn u16_opt(v: Option<f32>, scale: f32, offset: f32) -> u16 {
    match v {
        Some(v) => (((v + offset) * scale).round().clamp(0.0, 65534.0)) as u16,
        None => INVALID_U16,
    }
}

fn u32_opt(v: Option<f64>, scale: f64) -> u32 {
    match v {
        Some(v) => (v * scale).round().clamp(0.0, (u32::MAX - 1) as f64) as u32,
        None => INVALID_U32,
    }
}

fn u16_power(v: Option<f32>) -> u16 {
    v.map(|p| p.clamp(0.0, 65534.0).round() as u16).unwrap_or(INVALID_U16)
}

fn u8_opt(v: Option<f32>) -> u8 {
    v.map(|x| x.clamp(0.0, 254.0).round() as u8).unwrap_or(INVALID_U8)
}

pub fn write(activity: &Activity, samples: &[Sample]) -> Result<Vec<u8>, ImportError> {
    if samples.is_empty() {
        return Err(ImportError::Other(
            "cannot export an empty activity to FIT".to_string(),
        ));
    }
    let start_ms = activity.started_at_ms;
    let end_ms = activity
        .ended_at_ms
        .unwrap_or_else(|| samples.last().map(|s| s.t_ms).unwrap_or(start_ms));
    let mut w = FitWriter::new();

    // File ID (local 0)
    w.definition(
        0,
        0,
        &[
            (0, 1, 0x00), // type (enum)
            (1, 2, 0x84), // manufacturer
            (2, 2, 0x84), // product
            (3, 4, 0x86), // serial number
            (4, 4, 0x86), // time created
            (5, 2, 0x84), // number
        ],
    );
    let mut file_id = Vec::with_capacity(15);
    file_id.push(4); // activity
    file_id.extend_from_slice(&255u16.to_le_bytes()); // development
    file_id.extend_from_slice(&1u16.to_le_bytes());
    file_id.extend_from_slice(&1u32.to_le_bytes());
    file_id.extend_from_slice(&unix_ms_to_fit(crate::types::now_ms()).to_le_bytes());
    file_id.extend_from_slice(&0u16.to_le_bytes());
    w.message(0, &file_id);

    // Event: timer start (local 5)
    w.definition(
        5,
        21,
        &[(253, 4, 0x86), (0, 1, 0x00), (1, 1, 0x00), (3, 4, 0x86)],
    );
    let mut ev = Vec::with_capacity(10);
    ev.extend_from_slice(&unix_ms_to_fit(start_ms).to_le_bytes());
    ev.push(0); // timer
    ev.push(0); // start
    ev.extend_from_slice(&0u32.to_le_bytes());
    w.message(5, &ev);

    // Record definition (local 1)
    w.definition(
        1,
        20,
        &[
            (253, 4, 0x86),
            (0, 4, 0x85),
            (1, 4, 0x85),
            (2, 2, 0x84),
            (3, 1, 0x02),
            (4, 1, 0x02),
            (5, 4, 0x86),
            (6, 2, 0x84),
            (7, 2, 0x84),
        ],
    );
    for s in samples {
        let mut rec = Vec::with_capacity(24);
        rec.extend_from_slice(&unix_ms_to_fit(s.t_ms).to_le_bytes());
        match (s.lat, s.lon) {
            (Some(lat), Some(lon)) => {
                rec.extend_from_slice(&degrees_to_semicircles(lat).to_le_bytes());
                rec.extend_from_slice(&degrees_to_semicircles(lon).to_le_bytes());
            }
            _ => {
                rec.extend_from_slice(&INVALID_S32.to_le_bytes());
                rec.extend_from_slice(&INVALID_S32.to_le_bytes());
            }
        }
        rec.extend_from_slice(&u16_opt(s.altitude_m, 5.0, 500.0).to_le_bytes());
        rec.push(u8_opt(s.heart_rate_bpm));
        rec.push(u8_opt(s.cadence_rpm));
        rec.extend_from_slice(&u32_opt(Some(s.distance_m), 100.0).to_le_bytes());
        rec.extend_from_slice(&u16_opt(s.speed_mps, 1000.0, 0.0).to_le_bytes());
        rec.extend_from_slice(&u16_power(s.power_w).to_le_bytes());
        w.message(1, &rec);
    }

    // Event: timer stop
    let mut ev = Vec::with_capacity(10);
    ev.extend_from_slice(&unix_ms_to_fit(end_ms).to_le_bytes());
    ev.push(0);
    ev.push(1); // stop_all
    ev.extend_from_slice(&0u32.to_le_bytes());
    w.message(5, &ev);

    let summary = &activity.summary;
    let sport_code = match activity.sport {
        Sport::Running => 1u8,
        Sport::Cycling | Sport::Gravel | Sport::MountainBike => 2u8,
        Sport::VirtualRide => 2u8,
        Sport::Other => 0u8,
    };
    let sub_sport = match activity.sport {
        Sport::VirtualRide => 6u8,
        Sport::MountainBike => 8u8,
        Sport::Gravel => 9u8,
        _ => 7u8,
    };

    // Lap (local 3)
    w.definition(
        3,
        19,
        &[
            (253, 4, 0x86),
            (2, 4, 0x86),
            (7, 4, 0x86),
            (8, 4, 0x86),
            (9, 4, 0x86),
            (11, 2, 0x84),
            (13, 2, 0x84),
            (14, 2, 0x84),
            (15, 1, 0x02),
            (16, 1, 0x02),
            (17, 1, 0x02),
            (18, 1, 0x02),
            (19, 2, 0x84),
            (20, 2, 0x84),
            (21, 2, 0x84),
            (22, 2, 0x84),
            (24, 1, 0x00),
            (25, 1, 0x00),
        ],
    );
    let mut lap = Vec::with_capacity(45);
    lap.extend_from_slice(&unix_ms_to_fit(end_ms).to_le_bytes());
    lap.extend_from_slice(&unix_ms_to_fit(start_ms).to_le_bytes());
    lap.extend_from_slice(&u32_opt(Some(summary.duration_s), 1000.0).to_le_bytes());
    lap.extend_from_slice(&u32_opt(Some(summary.moving_s), 1000.0).to_le_bytes());
    lap.extend_from_slice(&u32_opt(Some(summary.distance_m), 100.0).to_le_bytes());
    let lap_calories = summary.calories_kcal.unwrap_or(0.0).clamp(0.0, 65534.0) as u16;
    lap.extend_from_slice(&lap_calories.to_le_bytes());
    lap.extend_from_slice(&u16_opt(summary.avg_speed_mps, 1000.0, 0.0).to_le_bytes());
    lap.extend_from_slice(&u16_opt(summary.max_speed_mps, 1000.0, 0.0).to_le_bytes());
    lap.push(u8_opt(summary.avg_heart_rate_bpm));
    lap.push(u8_opt(summary.max_heart_rate_bpm));
    lap.push(u8_opt(summary.avg_cadence_rpm));
    lap.push(u8_opt(summary.avg_cadence_rpm));
    lap.extend_from_slice(&u16_power(summary.avg_power_w).to_le_bytes());
    lap.extend_from_slice(&u16_power(summary.max_power_w).to_le_bytes());
    lap.extend_from_slice(&u16_opt(Some(summary.elevation_gain_m), 1.0, 0.0).to_le_bytes());
    lap.extend_from_slice(&u16_opt(Some(summary.elevation_loss_m), 1.0, 0.0).to_le_bytes());
    lap.push(0); // manual lap trigger
    lap.push(sport_code);
    w.message(3, &lap);

    // Session (local 2)
    w.definition(
        2,
        18,
        &[
            (253, 4, 0x86),
            (2, 4, 0x86),
            (5, 1, 0x00),
            (6, 1, 0x00),
            (7, 4, 0x86),
            (8, 4, 0x86),
            (9, 4, 0x86),
            (11, 2, 0x84),
            (14, 2, 0x84),
            (15, 2, 0x84),
            (16, 1, 0x02),
            (17, 1, 0x02),
            (18, 1, 0x02),
            (19, 1, 0x02),
            (20, 2, 0x84),
            (21, 2, 0x84),
            (22, 2, 0x84),
            (23, 2, 0x84),
            (34, 2, 0x84),
            (35, 2, 0x84),
            (36, 2, 0x84),
            (49, 2, 0x84),
            (50, 2, 0x84),
        ],
    );
    let mut session = Vec::with_capacity(50);
    let min_alt = samples.iter().filter_map(|s| s.altitude_m).fold(f32::INFINITY, f32::min);
    let max_alt = samples.iter().filter_map(|s| s.altitude_m).fold(f32::NEG_INFINITY, f32::max);
    session.extend_from_slice(&unix_ms_to_fit(end_ms).to_le_bytes());
    session.extend_from_slice(&unix_ms_to_fit(start_ms).to_le_bytes());
    session.push(sport_code);
    session.push(sub_sport);
    session.extend_from_slice(&u32_opt(Some(summary.duration_s), 1000.0).to_le_bytes());
    session.extend_from_slice(&u32_opt(Some(summary.moving_s), 1000.0).to_le_bytes());
    session.extend_from_slice(&u32_opt(Some(summary.distance_m), 100.0).to_le_bytes());
    let session_calories = summary.calories_kcal.unwrap_or(0.0).clamp(0.0, 65534.0) as u16;
    session.extend_from_slice(&session_calories.to_le_bytes());
    session.extend_from_slice(&u16_opt(summary.avg_speed_mps, 1000.0, 0.0).to_le_bytes());
    session.extend_from_slice(&u16_opt(summary.max_speed_mps, 1000.0, 0.0).to_le_bytes());
    session.push(u8_opt(summary.avg_heart_rate_bpm));
    session.push(u8_opt(summary.max_heart_rate_bpm));
    session.push(u8_opt(summary.avg_cadence_rpm));
    session.push(u8_opt(summary.avg_cadence_rpm));
    session.extend_from_slice(&u16_power(summary.avg_power_w).to_le_bytes());
    session.extend_from_slice(&u16_power(summary.max_power_w).to_le_bytes());
    session.extend_from_slice(&u16_opt(Some(summary.elevation_gain_m), 1.0, 0.0).to_le_bytes());
    session.extend_from_slice(&u16_opt(Some(summary.elevation_loss_m), 1.0, 0.0).to_le_bytes());
    session.extend_from_slice(&u16_power(summary.normalized_power_w).to_le_bytes());
    let tss_scaled = (summary.tss.unwrap_or(0.0) * 10.0).clamp(0.0, 65534.0) as u16;
    session.extend_from_slice(&tss_scaled.to_le_bytes());
    let if_scaled = (summary.intensity_factor.unwrap_or(0.0) * 1000.0).clamp(0.0, 65534.0) as u16;
    session.extend_from_slice(&if_scaled.to_le_bytes());
    session.extend_from_slice(&u16_opt(min_alt.is_finite().then_some(min_alt), 5.0, 500.0).to_le_bytes());
    session.extend_from_slice(&u16_opt(max_alt.is_finite().then_some(max_alt), 5.0, 500.0).to_le_bytes());
    w.message(2, &session);

    // Activity (local 4)
    w.definition(
        4,
        34,
        &[
            (253, 4, 0x86),
            (1, 2, 0x84),
            (2, 1, 0x00),
            (3, 1, 0x00),
            (4, 1, 0x00),
        ],
    );
    let mut act = Vec::with_capacity(9);
    act.extend_from_slice(&unix_ms_to_fit(end_ms).to_le_bytes());
    act.extend_from_slice(&1u16.to_le_bytes());
    act.push(0); // manual
    act.push(26); // activity
    act.push(1); // stop
    w.message(4, &act);

    // Assemble file: 14-byte header (including its CRC) + data + file CRC.
    let data = w.data;
    let mut out = Vec::with_capacity(data.len() + 18);
    out.push(14);
    out.push(0x10);
    out.extend_from_slice(&2200u16.to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(b".FIT");
    let header_crc = crc16(&out);
    out.extend_from_slice(&header_crc.to_le_bytes());
    out.extend_from_slice(&data);
    let file_crc = crc16(&out);
    out.extend_from_slice(&file_crc.to_le_bytes());
    Ok(out)
}

fn crc16(data: &[u8]) -> u16 {
    const fn table() -> [u16; 256] {
        let mut table = [0u16; 256];
        let mut i = 0;
        while i < 256 {
            let mut crc = i as u16;
            let mut j = 0;
            while j < 8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xA001
                } else {
                    crc >> 1
                };
                j += 1;
            }
            table[i] = crc;
            i += 1;
        }
        table
    }
    const TABLE: [u16; 256] = table();
    let mut crc = 0u16;
    for &byte in data {
        crc = (crc >> 8) ^ TABLE[((crc as u8) ^ byte) as usize];
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ActivitySummary, DataOrigin, Sport};

    fn test_activity_and_samples() -> (Activity, Vec<Sample>) {
        let start = 1_767_000_000_000i64;
        let samples: Vec<Sample> = (0..180)
            .map(|i| Sample {
                t_ms: start + i * 1000,
                elapsed_s: i as f64,
                lat: Some(48.0 + i as f64 * 0.0001),
                lon: Some(11.0 + i as f64 * 0.0001),
                altitude_m: Some(520.0 + (i as f32 * 0.2)),
                distance_m: i as f64 * 8.0,
                speed_mps: Some(8.0),
                power_w: Some(210.0),
                cadence_rpm: Some(90.0),
                heart_rate_bpm: Some(148.0),
                grade_pct: Some(0.5),
                sim_flags: 0,
                origin: DataOrigin::Bluetooth,
            })
            .collect();
        let activity = Activity {
            id: 1,
            name: "FIT Test".into(),
            sport: Sport::Cycling,
            started_at_ms: start,
            ended_at_ms: Some(start + 180_000),
            summary: ActivitySummary {
                duration_s: 180.0,
                moving_s: 180.0,
                distance_m: 1432.0,
                elevation_gain_m: 35.0,
                elevation_loss_m: 0.0,
                avg_speed_mps: Some(8.0),
                max_speed_mps: Some(8.0),
                avg_power_w: Some(210.0),
                max_power_w: Some(210.0),
                normalized_power_w: Some(210.0),
                avg_heart_rate_bpm: Some(148.0),
                max_heart_rate_bpm: Some(148.0),
                avg_cadence_rpm: Some(90.0),
                calories_kcal: Some(40.0),
                work_kj: Some(37.8),
                tss: Some(23.0),
                intensity_factor: Some(0.95),
            },
            route_id: None,
            route_name: None,
            source: "test".into(),
            notes: String::new(),
            created_at_ms: start,
        };
        (activity, samples)
    }

    #[test]
    fn fit_roundtrip() {
        let (activity, samples) = test_activity_and_samples();
        let bytes = write(&activity, &samples).unwrap();
        assert_eq!(&bytes[8..12], b".FIT");
        let parsed = parse(&bytes).unwrap();
        assert!(parsed.points.len() >= 178, "got {} points", parsed.points.len());
        assert_eq!(parsed.sport, Sport::Cycling);
        let p = &parsed.points[50];
        assert!((p.lat.unwrap() - (48.0 + 50.0 * 0.0001)).abs() < 1e-6);
        assert!((p.power_w.unwrap() - 210.0).abs() < 1.0);
        assert!((p.heart_rate_bpm.unwrap() - 148.0).abs() < 1.0);
        assert!((p.elevation_m.unwrap() - 530.0).abs() < 1.5);
    }

    #[test]
    fn crc_matches_fit_reference_vector() {
        // CRC of an empty file is 0; sanity-check the table-driven implementation.
        assert_eq!(crc16(&[]), 0);
        let data = b"123456789";
        assert_eq!(crc16(data), 0xBB3D);
    }

    #[test]
    fn rejects_empty_activity() {
        let (activity, _) = test_activity_and_samples();
        assert!(write(&activity, &[]).is_err());
    }
}
