//! Bluetooth GATT protocol codecs used by VELOCITY.
//!
//! Everything in this module is pure: byte slices in, plain data out. That
//! keeps the wire formats testable without a Bluetooth adapter and makes the
//! BLE hub responsible only for connection lifecycle.
//!
//! Supported services:
//! - FTMS `0x1826` — Indoor Bike Data, Control Point, Feature
//! - Cycling Power `0x1818` — Cycling Power Measurement
//! - Heart Rate `0x180D` — Heart Rate Measurement
//! - Cycling Speed and Cadence `0x1816`

use btleplug::api::bleuuid::uuid_from_u16;
use uuid::Uuid;

pub const FTMS_SERVICE: Uuid = uuid_from_u16(0x1826);
pub const FTMS_INDOOR_BIKE_DATA: Uuid = uuid_from_u16(0x2AD2);
pub const FTMS_CONTROL_POINT: Uuid = uuid_from_u16(0x2AD9);
pub const FTMS_STATUS: Uuid = uuid_from_u16(0x2ADA);
pub const FTMS_FEATURE: Uuid = uuid_from_u16(0x2ACC);

pub const CPS_SERVICE: Uuid = uuid_from_u16(0x1818);
pub const CPS_MEASUREMENT: Uuid = uuid_from_u16(0x2A63);

pub const HR_SERVICE: Uuid = uuid_from_u16(0x180D);
pub const HR_MEASUREMENT: Uuid = uuid_from_u16(0x2A37);

pub const CSC_SERVICE: Uuid = uuid_from_u16(0x1816);
pub const CSC_MEASUREMENT: Uuid = uuid_from_u16(0x2A5B);

/// Wahoo's proprietary trainer service. Used only as a detection hint; all
/// control traffic goes through standard FTMS.
pub const WAHOO_SERVICE: Uuid = Uuid::from_u128(0xA026_E000_0A7D_4AB3_97FA_F150_0F9F_EB8B);

pub const OP_REQUEST_CONTROL: u8 = 0x00;
pub const OP_RESET: u8 = 0x01;
pub const OP_SET_TARGET_SPEED: u8 = 0x02;
pub const OP_SET_TARGET_INCLINATION: u8 = 0x03;
pub const OP_SET_TARGET_RESISTANCE: u8 = 0x04;
pub const OP_SET_TARGET_POWER: u8 = 0x05;
pub const OP_SET_TARGET_HEART_RATE: u8 = 0x06;
pub const OP_START_RESUME: u8 = 0x07;
pub const OP_STOP_PAUSE: u8 = 0x08;
pub const OP_SET_TARGET_CADENCE: u8 = 0x09;
pub const OP_SET_SIMULATION: u8 = 0x11;
pub const OP_RESPONSE: u8 = 0x80;

/// Decoded `0x2AD2` Indoor Bike Data.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IndoorBikeData {
    pub speed_mps: Option<f32>,
    pub average_speed_mps: Option<f32>,
    pub cadence_rpm: Option<f32>,
    pub average_cadence_rpm: Option<f32>,
    pub distance_m: Option<f64>,
    pub resistance_level: Option<f32>,
    pub power_w: Option<f32>,
    pub average_power_w: Option<f32>,
    pub total_energy_kcal: Option<u16>,
    pub heart_rate_bpm: Option<f32>,
    pub metabolic_equivalent: Option<f32>,
    pub elapsed_s: Option<u16>,
    pub remaining_s: Option<u16>,
}

fn u8_at(buf: &[u8], cursor: &mut usize) -> Option<u8> {
    let v = *buf.get(*cursor)?;
    *cursor += 1;
    Some(v)
}

fn u16_at(buf: &[u8], cursor: &mut usize) -> Option<u16> {
    let bytes = buf.get(*cursor..*cursor + 2)?;
    *cursor += 2;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn s16_at(buf: &[u8], cursor: &mut usize) -> Option<i16> {
    u16_at(buf, cursor).map(|v| v as i16)
}

fn u24_at(buf: &[u8], cursor: &mut usize) -> Option<u32> {
    let bytes = buf.get(*cursor..*cursor + 3)?;
    *cursor += 3;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]))
}

fn u32_at(buf: &[u8], cursor: &mut usize) -> Option<u32> {
    let bytes = buf.get(*cursor..*cursor + 4)?;
    *cursor += 4;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// FTMS Indoor Bike Data flags. Bit 0 is inverted: it means "speed absent".
fn indoor_flags(flags: u16) -> (bool, bool) {
    let speed_present = flags & 0x0001 == 0;
    (speed_present, flags & 0x0002 != 0)
}

pub fn parse_indoor_bike_data(buf: &[u8]) -> Option<IndoorBikeData> {
    let flags = u16::from_le_bytes([*buf.first()?, *buf.get(1)?]);
    let (speed_present, avg_speed_present) = indoor_flags(flags);
    let mut c = 2usize;
    let mut d = IndoorBikeData::default();

    if speed_present {
        d.speed_mps = Some(u16_at(buf, &mut c)? as f32 / 100.0 / 3.6);
    }
    if avg_speed_present {
        d.average_speed_mps = Some(u16_at(buf, &mut c)? as f32 / 100.0 / 3.6);
    }
    if flags & (1 << 2) != 0 {
        d.cadence_rpm = Some(u16_at(buf, &mut c)? as f32 / 2.0);
    }
    if flags & (1 << 3) != 0 {
        d.average_cadence_rpm = Some(u16_at(buf, &mut c)? as f32 / 2.0);
    }
    if flags & (1 << 4) != 0 {
        d.distance_m = Some(u24_at(buf, &mut c)? as f64);
    }
    if flags & (1 << 5) != 0 {
        // Unitless resistance, 0.1 resolution.
        d.resistance_level = Some(s16_at(buf, &mut c)? as f32 / 10.0);
    }
    if flags & (1 << 6) != 0 {
        d.power_w = Some(s16_at(buf, &mut c)? as f32);
    }
    if flags & (1 << 7) != 0 {
        d.average_power_w = Some(s16_at(buf, &mut c)? as f32);
    }
    if flags & (1 << 8) != 0 {
        d.total_energy_kcal = Some(u16_at(buf, &mut c)?);
        // Energy per hour (kcal) and per minute (kcal); parsed to stay in sync.
        let _per_hour = u16_at(buf, &mut c)?;
        let _per_minute = u8_at(buf, &mut c)?;
    }
    if flags & (1 << 9) != 0 {
        d.heart_rate_bpm = Some(u8_at(buf, &mut c)? as f32);
    }
    if flags & (1 << 10) != 0 {
        // Metabolic equivalent, 0.1 resolution.
        d.metabolic_equivalent = Some(u8_at(buf, &mut c)? as f32 / 10.0);
    }
    if flags & (1 << 11) != 0 {
        d.elapsed_s = Some(u16_at(buf, &mut c)?);
    }
    if flags & (1 << 12) != 0 {
        d.remaining_s = Some(u16_at(buf, &mut c)?);
    }
    Some(d)
}

/// Decoded `0x2A37` Heart Rate Measurement.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeartRateData {
    pub bpm: f32,
    pub sensor_contact: Option<bool>,
    pub energy_expended_kj: Option<u16>,
    /// RR intervals in seconds.
    pub rr_intervals_s: Vec<f32>,
}

pub fn parse_heart_rate(buf: &[u8]) -> Option<HeartRateData> {
    let flags = *buf.first()?;
    let mut c = 1usize;
    let bpm = if flags & 0x01 != 0 {
        u16_at(buf, &mut c)? as f32
    } else {
        u8_at(buf, &mut c)? as f32
    };
    let sensor_contact = if flags & 0x04 != 0 {
        Some(flags & 0x02 != 0)
    } else {
        None
    };
    let energy_expended_kj = if flags & 0x08 != 0 {
        Some(u16_at(buf, &mut c)?)
    } else {
        None
    };
    let mut rr_intervals_s = Vec::new();
    if flags & 0x10 != 0 {
        while let Some(raw) = u16_at(buf, &mut c) {
            rr_intervals_s.push(raw as f32 / 1024.0);
        }
    }
    Some(HeartRateData {
        bpm,
        sensor_contact,
        energy_expended_kj,
        rr_intervals_s,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrankData {
    pub revolutions: u16,
    /// 1/1024 s.
    pub event_time: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WheelData {
    pub revolutions: u32,
    /// 1/1024 s.
    pub event_time: u16,
}

/// Decoded `0x2A63` Cycling Power Measurement.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CyclingPowerData {
    pub power_w: Option<f32>,
    pub crank: Option<CrankData>,
    pub wheel: Option<WheelData>,
    pub accumulated_energy_kj: Option<u16>,
}

pub fn parse_cycling_power(buf: &[u8]) -> Option<CyclingPowerData> {
    let flags = u16::from_le_bytes([*buf.first()?, *buf.get(1)?]);
    let mut c = 2usize;
    let mut d = CyclingPowerData::default();
    if flags & (1 << 0) != 0 {
        let _balance = u8_at(buf, &mut c)?;
    }
    if flags & (1 << 2) != 0 {
        let _torque = u16_at(buf, &mut c)?;
    }
    if flags & (1 << 4) != 0 {
        d.wheel = Some(WheelData {
            revolutions: u32_at(buf, &mut c)?,
            event_time: u16_at(buf, &mut c)?,
        });
    }
    if flags & (1 << 5) != 0 {
        d.crank = Some(CrankData {
            revolutions: u16_at(buf, &mut c)?,
            event_time: u16_at(buf, &mut c)?,
        });
    }
    if flags & (1 << 6) != 0 {
        let _max = u16_at(buf, &mut c)?;
        let _min = u16_at(buf, &mut c)?;
    }
    if flags & (1 << 7) != 0 {
        let _max = u16_at(buf, &mut c)?;
        let _min = u16_at(buf, &mut c)?;
    }
    if flags & (1 << 8) != 0 {
        // Extreme angles: two packed 12-bit values, 3 bytes.
        buf.get(c..c + 3)?;
        c += 3;
    }
    if flags & (1 << 9) != 0 {
        let _top = u16_at(buf, &mut c)?;
    }
    if flags & (1 << 10) != 0 {
        let _bottom = u16_at(buf, &mut c)?;
    }
    d.power_w = Some(s16_at(buf, &mut c)? as f32);
    if flags & (1 << 11) != 0 {
        d.accumulated_energy_kj = Some(u16_at(buf, &mut c)?);
    }
    Some(d)
}

/// Decoded `0x2A5B` Cycling Speed and Cadence Measurement.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CscData {
    pub wheel: Option<WheelData>,
    pub crank: Option<CrankData>,
}

pub fn parse_csc(buf: &[u8]) -> Option<CscData> {
    let flags = *buf.first()?;
    let mut c = 1usize;
    let mut d = CscData::default();
    if flags & 0x01 != 0 {
        d.wheel = Some(WheelData {
            revolutions: u32_at(buf, &mut c)?,
            event_time: u16_at(buf, &mut c)?,
        });
    }
    if flags & 0x02 != 0 {
        d.crank = Some(CrankData {
            revolutions: u16_at(buf, &mut c)?,
            event_time: u16_at(buf, &mut c)?,
        });
    }
    Some(d)
}

/// Decoded FTMS control-point response (`0x80`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlResponse {
    pub request_opcode: u8,
    pub result: u8,
}

impl ControlResponse {
    pub fn ok(self) -> bool {
        self.result == 0x01
    }

    pub fn result_label(self) -> &'static str {
        match self.result {
            0x01 => "Success",
            0x02 => "Operation not supported",
            0x03 => "Invalid parameter",
            0x04 => "Operation failed",
            0x05 => "Control not permitted",
            _ => "Unknown result",
        }
    }

    pub fn opcode_label(self) -> &'static str {
        match self.request_opcode {
            OP_REQUEST_CONTROL => "Request control",
            OP_RESET => "Reset",
            OP_SET_TARGET_SPEED => "Set target speed",
            OP_SET_TARGET_INCLINATION => "Set incline",
            OP_SET_TARGET_RESISTANCE => "Set resistance",
            OP_SET_TARGET_POWER => "Set target power",
            OP_SET_TARGET_HEART_RATE => "Set target heart rate",
            OP_START_RESUME => "Start/resume",
            OP_STOP_PAUSE => "Stop/pause",
            OP_SET_TARGET_CADENCE => "Set target cadence",
            OP_SET_SIMULATION => "Set simulation",
            _ => "Control command",
        }
    }
}

pub fn parse_control_response(buf: &[u8]) -> Option<ControlResponse> {
    if buf.len() < 3 || buf[0] != OP_RESPONSE {
        return None;
    }
    Some(ControlResponse {
        request_opcode: buf[1],
        result: buf[2],
    })
}

/// Feature bitmask from `0x2ACC`. Only used for diagnostics in the UI.
pub fn parse_feature_flags(buf: &[u8]) -> Option<u32> {
    let b = buf.get(0..4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

pub fn feature_labels(flags: u32) -> Vec<&'static str> {
    const LABELS: [(u32, &str); 10] = [
        (1 << 0, "Average speed"),
        (1 << 1, "Cadence"),
        (1 << 2, "Total distance"),
        (1 << 3, "Resistance level"),
        (1 << 4, "Power"),
        (1 << 5, "Energy"),
        (1 << 6, "Heart rate"),
        (1 << 7, "Metabolic equivalent"),
        (1 << 8, "Elapsed time"),
        (1 << 9, "Remaining time"),
    ];
    let mut out = Vec::new();
    for (bit, label) in LABELS {
        if flags & bit != 0 {
            out.push(label);
        }
    }
    if flags >> 10 != 0 {
        out.push("Target setting");
    }
    out
}

pub fn cmds_request_control() -> Vec<u8> {
    vec![OP_REQUEST_CONTROL]
}

pub fn cmds_reset() -> Vec<u8> {
    vec![OP_RESET]
}

pub fn cmds_start_resume() -> Vec<u8> {
    vec![OP_START_RESUME]
}

pub fn cmds_stop() -> Vec<u8> {
    vec![OP_STOP_PAUSE, 0x01]
}

pub fn cmds_set_target_power(watts: f32) -> Vec<u8> {
    let mut out = vec![OP_SET_TARGET_POWER];
    out.extend_from_slice(&(watts.clamp(0.0, 3000.0).round() as i16).to_le_bytes());
    out
}

pub fn cmds_set_target_inclination(grade_pct: f32) -> Vec<u8> {
    let mut out = vec![OP_SET_TARGET_INCLINATION];
    let centi = (grade_pct.clamp(-40.0, 40.0) * 10.0).round() as i16;
    out.extend_from_slice(&centi.to_le_bytes());
    out
}

pub fn cmds_set_target_resistance(percent: f32) -> Vec<u8> {
    // FTMS resistance is unitless with 0.1 resolution; map 0..100 % to 0..10.
    let raw = (percent.clamp(0.0, 100.0) * 0.1).round().clamp(0.0, 255.0) as u8;
    vec![OP_SET_TARGET_RESISTANCE, raw]
}

pub fn cmds_set_target_cadence(rpm: f32) -> Vec<u8> {
    let mut out = vec![OP_SET_TARGET_CADENCE];
    let raw = (rpm.clamp(0.0, 255.0) * 2.0).round() as u16;
    out.extend_from_slice(&raw.to_le_bytes());
    out
}

pub fn cmds_set_simulation(grade_pct: f32, wind_mps: f32, crr: f32, cw_kg_per_m: f32) -> Vec<u8> {
    let mut out = vec![OP_SET_SIMULATION];
    let wind = (wind_mps.clamp(-32.0, 32.0) * 1000.0).round() as i16;
    let grade = (grade_pct.clamp(-40.0, 40.0) * 100.0).round() as i16;
    let rolling = (crr.clamp(0.0, 0.0255) * 10_000.0).round() as u8;
    let wind_coeff = (cw_kg_per_m.clamp(0.0, 2.55) * 100.0).round() as u8;
    out.extend_from_slice(&wind.to_le_bytes());
    out.extend_from_slice(&grade.to_le_bytes());
    out.push(rolling);
    out.push(wind_coeff);
    out
}

/// Converts wheel and crank revolution counters into speed/cadence.
///
/// Devices send cumulative counters with 1/1024 s event timestamps. Wrapping is
/// normal, so differences use wrapping arithmetic and implausible results are
/// discarded rather than shown as spikes.
#[derive(Clone, Copy, Debug)]
pub struct RevTracker {
    pub wheel_circumference_m: f32,
    last_wheel: Option<WheelData>,
    last_crank: Option<CrankData>,
}

impl Default for RevTracker {
    fn default() -> Self {
        Self {
            wheel_circumference_m: 2.105,
            last_wheel: None,
            last_crank: None,
        }
    }
}

impl RevTracker {
    pub fn new(wheel_circumference_m: f32) -> Self {
        Self {
            wheel_circumference_m: wheel_circumference_m.clamp(0.5, 3.5),
            ..Default::default()
        }
    }

    pub fn speed_mps(&mut self, wheel: Option<WheelData>) -> Option<f32> {
        let w = wheel?;
        let prev = self.last_wheel.replace(w);
        let prev = prev?;
        let dt = ((w.event_time.wrapping_sub(prev.event_time)) as f32) / 1024.0;
        if !(0.05..=10.0).contains(&dt) {
            return None;
        }
        let revs = w.revolutions.wrapping_sub(prev.revolutions) as f32;
        let speed = revs * self.wheel_circumference_m / dt;
        (0.0..=40.0).contains(&speed).then_some(speed)
    }

    pub fn cadence_rpm(&mut self, crank: Option<CrankData>) -> Option<f32> {
        let c = crank?;
        let prev = self.last_crank.replace(c);
        let prev = prev?;
        let dt = ((c.event_time.wrapping_sub(prev.event_time)) as f32) / 1024.0;
        if !(0.05..=10.0).contains(&dt) {
            return None;
        }
        let revs = c.revolutions.wrapping_sub(prev.revolutions) as f32;
        let rpm = revs * 60.0 / dt;
        (0.0..=250.0).contains(&rpm).then_some(rpm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_indoor_bike_data() {
        let mut b = Vec::new();
        // speed, avg speed, cadence, avg cadence, distance, resistance, power,
        // avg power, energy(3), hr, met, elapsed, remaining.
        let flags: u16 = 0x1FFE; // bit 0 clear = instantaneous speed present
        b.extend_from_slice(&flags.to_le_bytes());
        b.extend_from_slice(&2500u16.to_le_bytes()); // 25.00 km/h
        b.extend_from_slice(&2400u16.to_le_bytes()); // 24.00 km/h
        b.extend_from_slice(&180u16.to_le_bytes()); // 90 rpm
        b.extend_from_slice(&176u16.to_le_bytes()); // 88 rpm
        b.extend_from_slice(&1234u32.to_le_bytes()[..3]); // 1234 m
        b.extend_from_slice(&(-15i16).to_le_bytes()); // resistance -1.5
        b.extend_from_slice(&215i16.to_le_bytes());
        b.extend_from_slice(&210i16.to_le_bytes());
        b.extend_from_slice(&500u16.to_le_bytes());
        b.extend_from_slice(&600u16.to_le_bytes());
        b.push(10);
        b.push(142);
        b.push(80); // 8.0 MET
        b.extend_from_slice(&600u16.to_le_bytes());
        b.extend_from_slice(&300u16.to_le_bytes());

        let d = parse_indoor_bike_data(&b).unwrap();
        assert!((d.speed_mps.unwrap() - 6.944).abs() < 0.01);
        assert!((d.cadence_rpm.unwrap() - 90.0).abs() < 0.01);
        assert_eq!(d.distance_m, Some(1234.0));
        assert_eq!(d.resistance_level, Some(-1.5));
        assert_eq!(d.power_w, Some(215.0));
        assert_eq!(d.heart_rate_bpm, Some(142.0));
        assert_eq!(d.elapsed_s, Some(600));
        assert_eq!(d.remaining_s, Some(300));
    }

    #[test]
    fn parses_minimal_indoor_bike_data_without_speed() {
        // Bit 0 set means speed omitted; only instantaneous power present.
        let flags: u16 = 0x0001 | (1 << 6);
        let mut b = flags.to_le_bytes().to_vec();
        b.extend_from_slice(&180i16.to_le_bytes());
        let d = parse_indoor_bike_data(&b).unwrap();
        assert_eq!(d.speed_mps, None);
        assert_eq!(d.power_w, Some(180.0));
    }

    #[test]
    fn rejects_truncated_indoor_bike_data() {
        let flags: u16 = 1 << 6; // power present, speed present
        let mut b = flags.to_le_bytes().to_vec();
        b.push(0x01); // only half of the power field
        assert!(parse_indoor_bike_data(&b).is_none());
    }

    #[test]
    fn parses_heart_rate_u8_and_u16() {
        let d = parse_heart_rate(&[0x06, 148]).unwrap(); // contact supported+detected
        assert_eq!(d.bpm, 148.0);
        assert_eq!(d.sensor_contact, Some(true));

        let mut b = vec![0x11]; // u16 bpm + RR intervals
        b.extend_from_slice(&150u16.to_le_bytes());
        b.extend_from_slice(&1024u16.to_le_bytes());
        b.extend_from_slice(&512u16.to_le_bytes());
        let d = parse_heart_rate(&b).unwrap();
        assert_eq!(d.bpm, 150.0);
        assert_eq!(d.rr_intervals_s.len(), 2);
        assert!((d.rr_intervals_s[0] - 1.0).abs() < 0.001);
        assert!((d.rr_intervals_s[1] - 0.5).abs() < 0.001);
    }

    #[test]
    fn parses_cycling_power_with_crank_and_wheel() {
        let flags: u16 = (1 << 4) | (1 << 5);
        let mut b = flags.to_le_bytes().to_vec();
        b.extend_from_slice(&500u32.to_le_bytes());
        b.extend_from_slice(&2048u16.to_le_bytes());
        b.extend_from_slice(&100u16.to_le_bytes());
        b.extend_from_slice(&1024u16.to_le_bytes());
        b.extend_from_slice(&250i16.to_le_bytes());
        let d = parse_cycling_power(&b).unwrap();
        assert_eq!(d.power_w, Some(250.0));
        assert_eq!(d.wheel.unwrap().revolutions, 500);
        assert_eq!(d.crank.unwrap().revolutions, 100);
    }

    #[test]
    fn parses_csc() {
        let mut b = vec![0x03];
        b.extend_from_slice(&1000u32.to_le_bytes());
        b.extend_from_slice(&1024u16.to_le_bytes());
        b.extend_from_slice(&500u16.to_le_bytes());
        b.extend_from_slice(&2048u16.to_le_bytes());
        let d = parse_csc(&b).unwrap();
        assert_eq!(d.wheel.unwrap().revolutions, 1000);
        assert_eq!(d.crank.unwrap().revolutions, 500);
    }

    #[test]
    fn rev_tracker_computes_speed_and_cadence() {
        let mut t = RevTracker::new(2.0);
        assert_eq!(t.speed_mps(Some(WheelData { revolutions: 10, event_time: 0 })), None);
        let speed = t
            .speed_mps(Some(WheelData {
                revolutions: 20,
                event_time: 1024,
            }))
            .unwrap();
        assert!((speed - 20.0).abs() < 0.001, "speed {speed}");

        assert_eq!(t.cadence_rpm(Some(CrankData { revolutions: 2, event_time: 0 })), None);
        let cadence = t
            .cadence_rpm(Some(CrankData {
                revolutions: 3,
                event_time: 512,
            }))
            .unwrap();
        assert!((cadence - 120.0).abs() < 0.001, "cadence {cadence}");
    }

    #[test]
    fn rev_tracker_discards_implausible_deltas() {
        let mut t = RevTracker::new(2.0);
        t.speed_mps(Some(WheelData { revolutions: 10, event_time: 0 }));
        // 2000 revolutions in 0.1 s is not a bicycle.
        assert_eq!(
            t.speed_mps(Some(WheelData {
                revolutions: 2010,
                event_time: 102,
            })),
            None
        );
    }

    #[test]
    fn control_commands_match_ftms_layout() {
        assert_eq!(cmds_request_control(), vec![0x00]);
        assert_eq!(cmds_set_target_power(215.4), vec![0x05, 0xD7, 0x00]);
        assert_eq!(cmds_set_target_resistance(50.0), vec![0x04, 5]);
        assert_eq!(cmds_set_target_inclination(6.5), vec![0x03, 65, 0x00]);
        let sim = cmds_set_simulation(5.0, 0.0, 0.005, 0.51);
        assert_eq!(sim, vec![0x11, 0x00, 0x00, 0xF4, 0x01, 50, 51]);
        let resp = parse_control_response(&[0x80, 0x00, 0x01]).unwrap();
        assert!(resp.ok());
        assert_eq!(resp.opcode_label(), "Request control");
        assert_eq!(
            parse_control_response(&[0x80, 0x05, 0x05]).unwrap().result_label(),
            "Control not permitted"
        );
    }

    #[test]
    fn decodes_feature_flags() {
        let flags = parse_feature_flags(&[0x5B, 0x00, 0x00, 0x00]).unwrap();
        let labels = feature_labels(flags);
        assert!(labels.contains(&"Cadence"));
        assert!(labels.contains(&"Power"));
    }
}
