//! Core domain types shared by every VELOCITY subsystem.
//!
//! All timestamps are Unix milliseconds UTC unless stated otherwise. Every
//! recorded sample keeps a provenance bitmask so simulated values can never be
//! mistaken for real sensor data.

use serde::{Deserialize, Serialize};

pub const APP_NAME: &str = "VELOCITY";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Bit flags describing which channels of a sample are simulated.
pub mod sim {
    pub const POWER: u16 = 1 << 0;
    pub const HEART_RATE: u16 = 1 << 1;
    pub const CADENCE: u16 = 1 << 2;
    pub const SPEED: u16 = 1 << 3;
    pub const GPS: u16 = 1 << 4;
    pub const GRADE: u16 = 1 << 5;
    pub const ALL: u16 = POWER | HEART_RATE | CADENCE | SPEED | GPS | GRADE;

    pub fn label(mask: u16) -> &'static str {
        if mask == 0 {
            "Sensor"
        } else if mask == ALL {
            "Simulation"
        } else {
            "Mixed"
        }
    }
}

/// Where a value came from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataOrigin {
    #[default]
    Unknown,
    Bluetooth,
    AntPlus,
    FileImport,
    Simulated,
    Manual,
    Gps,
}

impl DataOrigin {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Bluetooth => "Bluetooth",
            Self::AntPlus => "ANT+",
            Self::FileImport => "Import",
            Self::Simulated => "Simulated",
            Self::Manual => "Manual",
            Self::Gps => "GPS",
        }
    }

    pub fn is_simulated(self) -> bool {
        matches!(self, Self::Simulated)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sport {
    #[default]
    Cycling,
    VirtualRide,
    MountainBike,
    Gravel,
    Running,
    Other,
}

impl Sport {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cycling => "Road Cycling",
            Self::VirtualRide => "Virtual Ride",
            Self::MountainBike => "Mountain Bike",
            Self::Gravel => "Gravel",
            Self::Running => "Running",
            Self::Other => "Other",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Cycling => "cycling",
            Self::VirtualRide => "virtual_ride",
            Self::MountainBike => "mountain_bike",
            Self::Gravel => "gravel",
            Self::Running => "running",
            Self::Other => "other",
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key {
            "virtual_ride" | "virtualride" => Self::VirtualRide,
            "mountain_bike" | "mtb" => Self::MountainBike,
            "gravel" => Self::Gravel,
            "running" => Self::Running,
            "other" => Self::Other,
            _ => Self::Cycling,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sex {
    #[default]
    Unspecified,
    Female,
    Male,
}

/// The athlete. Values are user-editable; defaults are deliberately mid-range
/// so the physics engine produces plausible speeds out of the box.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RiderProfile {
    pub name: String,
    pub weight_kg: f32,
    pub height_cm: f32,
    pub age: u8,
    pub sex: Sex,
    pub ftp_w: f32,
    pub max_hr_bpm: f32,
    pub resting_hr_bpm: f32,
}

impl Default for RiderProfile {
    fn default() -> Self {
        Self {
            name: "Rider".to_string(),
            weight_kg: 78.0,
            height_cm: 178.0,
            age: 35,
            sex: Sex::Unspecified,
            ftp_w: 220.0,
            max_hr_bpm: 190.0,
            resting_hr_bpm: 55.0,
        }
    }
}

impl RiderProfile {
    pub fn total_mass_kg(&self, bike: &BikeProfile) -> f32 {
        self.weight_kg + bike.mass_kg
    }

    /// Heart-rate reserve based zones (Karvonen).
    pub fn hr_zone_bounds(&self) -> [f32; 6] {
        let reserve = (self.max_hr_bpm - self.resting_hr_bpm).max(1.0);
        [
            self.resting_hr_bpm + 0.50 * reserve,
            self.resting_hr_bpm + 0.60 * reserve,
            self.resting_hr_bpm + 0.70 * reserve,
            self.resting_hr_bpm + 0.80 * reserve,
            self.resting_hr_bpm + 0.90 * reserve,
            self.max_hr_bpm,
        ]
    }

    pub fn power_zone_bounds(&self) -> [f32; 6] {
        let ftp = self.ftp_w.max(1.0);
        [0.55 * ftp, 0.75 * ftp, 0.90 * ftp, 1.05 * ftp, 1.20 * ftp, ftp * 2.0]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BikeProfile {
    pub name: String,
    pub mass_kg: f32,
    /// Coefficient of rolling resistance. 0.004 = good asphalt, 0.008 = rough road.
    pub crr: f32,
    /// Effective frontal area (CdA) in m². 0.32 ≈ hoods, 0.25 ≈ drops.
    pub cda_m2: f32,
    pub wheel_circumference_mm: f32,
}

impl Default for BikeProfile {
    fn default() -> Self {
        Self {
            name: "Road Bike".to_string(),
            mass_kg: 8.5,
            crr: 0.005,
            cda_m2: 0.36,
            wheel_circumference_mm: 2105.0,
        }
    }
}

/// A single sensor reading with provenance.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Reading {
    pub value: f32,
    pub t_ms: i64,
    pub simulated: bool,
    pub origin: DataOrigin,
}

impl Reading {
    pub fn new(value: f32, t_ms: i64, simulated: bool, origin: DataOrigin) -> Self {
        Self {
            value,
            t_ms,
            simulated,
            origin,
        }
    }

    pub fn age_ms(self, now_ms: i64) -> i64 {
        (now_ms - self.t_ms).max(0)
    }

    pub fn is_stale(self, now_ms: i64, max_age_ms: i64) -> bool {
        self.age_ms(now_ms) > max_age_ms
    }

    /// Short badge shown in the UI for simulated or stale readings.
    pub fn badge(self, now_ms: i64) -> Option<&'static str> {
        if self.simulated {
            Some("SIM")
        } else if self.is_stale(now_ms, 5_000) {
            Some("STALE")
        } else {
            None
        }
    }
}

/// One recorded telemetry sample. Optional fields stay `None` when the channel
/// was unavailable; they are never silently zero-filled.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Sample {
    pub t_ms: i64,
    pub elapsed_s: f64,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub altitude_m: Option<f32>,
    pub distance_m: f64,
    pub speed_mps: Option<f32>,
    pub power_w: Option<f32>,
    pub cadence_rpm: Option<f32>,
    pub heart_rate_bpm: Option<f32>,
    pub grade_pct: Option<f32>,
    /// Bitmask of simulated channels, see [`sim`].
    pub sim_flags: u16,
    pub origin: DataOrigin,
}

impl Sample {
    pub fn is_simulated(&self, flag: u16) -> bool {
        self.sim_flags & flag != 0
    }

    pub fn provenance(&self) -> &'static str {
        sim::label(self.sim_flags)
    }
}

/// Aggregate statistics for an activity. Values are `None` when the channel was
/// not recorded, so the UI can show an honest dash instead of a fake zero.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ActivitySummary {
    pub duration_s: f64,
    pub moving_s: f64,
    pub distance_m: f64,
    pub elevation_gain_m: f32,
    pub elevation_loss_m: f32,
    pub avg_speed_mps: Option<f32>,
    pub max_speed_mps: Option<f32>,
    pub avg_power_w: Option<f32>,
    pub max_power_w: Option<f32>,
    pub normalized_power_w: Option<f32>,
    pub avg_heart_rate_bpm: Option<f32>,
    pub max_heart_rate_bpm: Option<f32>,
    pub avg_cadence_rpm: Option<f32>,
    pub calories_kcal: Option<f32>,
    /// Total mechanical work in kilojoules (from power meter).
    pub work_kj: Option<f32>,
    pub tss: Option<f32>,
    pub intensity_factor: Option<f32>,
}

impl ActivitySummary {
    pub fn avg_speed_kmh(&self) -> Option<f32> {
        self.avg_speed_mps.map(|v| v * 3.6)
    }

    pub fn max_speed_kmh(&self) -> Option<f32> {
        self.max_speed_mps.map(|v| v * 3.6)
    }
}

/// A completed or in-progress activity.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Activity {
    pub id: i64,
    pub name: String,
    pub sport: Sport,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub summary: ActivitySummary,
    pub route_id: Option<i64>,
    pub route_name: Option<String>,
    /// Human readable source, e.g. "Wahoo KICKR + Garmin Forerunner 245".
    pub source: String,
    pub notes: String,
    pub created_at_ms: i64,
}

impl Activity {
    pub fn is_finished(&self) -> bool {
        self.ended_at_ms.is_some()
    }
}

/// Formatting helpers kept in core so CLI, UI and exports agree.
pub fn fmt_duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

pub fn fmt_duration_compact(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{m}m {s:02}s", s = total % 60)
    }
}

pub fn fmt_distance_km(meters: f64) -> String {
    if meters >= 100_000.0 {
        format!("{:.0} km", meters / 1000.0)
    } else {
        format!("{:.1} km", meters / 1000.0)
    }
}

pub fn fmt_num(v: Option<f32>, decimals: usize) -> String {
    match v {
        Some(v) => format!("{v:.decimals$}"),
        None => "—".to_string(),
    }
}

pub fn fmt_int(v: Option<f32>) -> String {
    match v {
        Some(v) => format!("{:.0}", v),
        None => "—".to_string(),
    }
}

pub fn local_date(ms: i64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_millis_opt(ms).single() {
        Some(dt) => dt.format("%Y-%m-%d").to_string(),
        None => String::new(),
    }
}

pub fn local_datetime(ms: i64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_millis_opt(ms).single() {
        Some(dt) => dt.format("%a, %d %b %Y · %H:%M").to_string(),
        None => String::new(),
    }
}

pub fn local_time(ms: i64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_millis_opt(ms).single() {
        Some(dt) => dt.format("%H:%M:%S").to_string(),
        None => String::new(),
    }
}

pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_formatting() {
        assert_eq!(fmt_duration(0.0), "0:00");
        assert_eq!(fmt_duration(59.6), "1:00");
        assert_eq!(fmt_duration(3661.0), "1:01:01");
        assert_eq!(fmt_duration_compact(3661.0), "1h 01m");
    }

    #[test]
    fn reading_staleness() {
        let r = Reading::new(100.0, 1_000, false, DataOrigin::Bluetooth);
        assert!(!r.is_stale(4_000, 5_000));
        assert!(r.is_stale(6_001, 5_000));
        assert_eq!(r.badge(9_000), Some("STALE"));
    }
}
