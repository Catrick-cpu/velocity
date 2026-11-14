//! Live telemetry aggregation and ride-session state machine.
//!
//! The UI renders at display rate, but samples are committed to the recording
//! at 1 Hz. Every channel keeps a timestamp and a simulated flag so stale or
//! synthetic values are always visible in the interface.

use crate::types::{DataOrigin, Reading, Sample, sim};
use std::collections::VecDeque;

pub const STALE_AFTER_MS: i64 = 5_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metric {
    Speed,
    Power,
    HeartRate,
    Cadence,
    Grade,
    Altitude,
}

impl Metric {
    pub fn sim_flag(self) -> u16 {
        match self {
            Self::Speed => sim::SPEED,
            Self::Power => sim::POWER,
            Self::HeartRate => sim::HEART_RATE,
            Self::Cadence => sim::CADENCE,
            Self::Grade => sim::GRADE,
            Self::Altitude => sim::GPS,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Speed => "Speed",
            Self::Power => "Power",
            Self::HeartRate => "Heart rate",
            Self::Cadence => "Cadence",
            Self::Grade => "Gradient",
            Self::Altitude => "Altitude",
        }
    }

    pub fn unit(self) -> &'static str {
        match self {
            Self::Speed => "m/s",
            Self::Power => "W",
            Self::HeartRate => "bpm",
            Self::Cadence => "rpm",
            Self::Grade => "%",
            Self::Altitude => "m",
        }
    }
}

/// Clamp and reject implausible sensor values before they reach the UI or DB.
pub fn sanitize(metric: Metric, value: f32) -> Option<f32> {
    if !value.is_finite() {
        return None;
    }
    let (lo, hi) = match metric {
        Metric::Speed => (0.0, 40.0),      // 144 km/h
        Metric::Power => (0.0, 3000.0),
        Metric::HeartRate => (20.0, 240.0),
        Metric::Cadence => (0.0, 260.0),
        Metric::Grade => (-40.0, 40.0),
        Metric::Altitude => (-500.0, 9000.0),
    };
    (value >= lo && value <= hi).then_some(value)
}

#[derive(Clone, Copy, Debug, Default)]
struct Accum {
    sum: f64,
    time: f64,
    max: f32,
}

impl Accum {
    fn push(&mut self, value: f32, dt: f64) {
        if dt <= 0.0 || !value.is_finite() {
            return;
        }
        self.sum += value as f64 * dt;
        self.time += dt;
        self.max = self.max.max(value);
    }

    fn avg(&self) -> Option<f32> {
        (self.time > 0.0).then(|| (self.sum / self.time) as f32)
    }

    fn max(&self) -> Option<f32> {
        (self.time > 0.0).then_some(self.max)
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Channels and aggregates for the current ride.
#[derive(Clone, Debug, Default)]
pub struct LiveMetrics {
    pub speed: Option<Reading>,
    pub power: Option<Reading>,
    pub heart_rate: Option<Reading>,
    pub cadence: Option<Reading>,
    pub grade: Option<Reading>,
    pub altitude: Option<Reading>,
    pub gps: Option<(f64, f64, i64, DataOrigin)>,

    pub elapsed_s: f64,
    pub distance_m: f64,
    pub elevation_gain_m: f32,

    avg_speed: Accum,
    avg_power: Accum,
    avg_hr: Accum,
    avg_cadence: Accum,
    max_power: f32,
    max_hr: f32,
    max_speed: f32,
    recent_power: VecDeque<f32>,
    pub last_sample: Option<Sample>,
}

impl LiveMetrics {
    pub fn set(&mut self, metric: Metric, reading: Reading) {
        if sanitize(metric, reading.value).is_none() {
            log::warn!(
                "rejected implausible {} value {:.1}",
                metric.label(),
                reading.value
            );
            return;
        }
        match metric {
            Metric::Speed => self.speed = Some(reading),
            Metric::Power => self.power = Some(reading),
            Metric::HeartRate => self.heart_rate = Some(reading),
            Metric::Cadence => self.cadence = Some(reading),
            Metric::Grade => self.grade = Some(reading),
            Metric::Altitude => self.altitude = Some(reading),
        }
    }

    pub fn set_gps(&mut self, lat: f64, lon: f64, t_ms: i64, origin: DataOrigin) {
        if (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon) {
            self.gps = Some((lat, lon, t_ms, origin));
        }
    }

    fn fresh(&self, r: Option<Reading>, now_ms: i64, max_age_ms: i64) -> Option<Reading> {
        r.filter(|r| !r.is_stale(now_ms, max_age_ms))
    }

    pub fn current_speed(&self, now_ms: i64) -> Option<Reading> {
        self.fresh(self.speed, now_ms, STALE_AFTER_MS)
    }

    pub fn current_power(&self, now_ms: i64) -> Option<Reading> {
        self.fresh(self.power, now_ms, STALE_AFTER_MS)
    }

    pub fn current_hr(&self, now_ms: i64) -> Option<Reading> {
        self.fresh(self.heart_rate, now_ms, STALE_AFTER_MS)
    }

    pub fn current_cadence(&self, now_ms: i64) -> Option<Reading> {
        self.fresh(self.cadence, now_ms, STALE_AFTER_MS)
    }

    pub fn current_grade(&self, now_ms: i64) -> Option<Reading> {
        self.fresh(self.grade, now_ms, STALE_AFTER_MS)
    }

    pub fn avg_power(&self) -> Option<f32> {
        self.avg_power.avg()
    }

    pub fn avg_heart_rate(&self) -> Option<f32> {
        self.avg_hr.avg()
    }

    pub fn avg_cadence(&self) -> Option<f32> {
        self.avg_cadence.avg()
    }

    pub fn avg_speed(&self) -> Option<f32> {
        self.avg_speed.avg()
    }

    pub fn max_power(&self) -> Option<f32> {
        (self.max_power > 0.0).then_some(self.max_power)
    }

    pub fn max_heart_rate(&self) -> Option<f32> {
        (self.max_hr > 0.0).then_some(self.max_hr)
    }

    pub fn max_speed(&self) -> Option<f32> {
        (self.max_speed > 0.0).then_some(self.max_speed)
    }

    /// 30 s rolling average power, updated at 1 Hz; a live proxy for NP.
    pub fn recent_avg_power(&self) -> Option<f32> {
        if self.recent_power.is_empty() {
            None
        } else {
            Some(self.recent_power.iter().sum::<f32>() / self.recent_power.len() as f32)
        }
    }

    pub fn sim_flags(&self) -> u16 {
        let mut flags = 0;
        for (reading, flag) in [
            (self.speed, sim::SPEED),
            (self.power, sim::POWER),
            (self.heart_rate, sim::HEART_RATE),
            (self.cadence, sim::CADENCE),
            (self.grade, sim::GRADE),
        ] {
            if reading.map(|r| r.simulated).unwrap_or(false) {
                flags |= flag;
            }
        }
        if self.gps.map(|g| g.3 == DataOrigin::Simulated).unwrap_or(false) {
            flags |= sim::GPS;
        }
        flags
    }

    pub fn reset_aggregates(&mut self) {
        self.avg_speed.reset();
        self.avg_power.reset();
        self.avg_hr.reset();
        self.avg_cadence.reset();
        self.max_power = 0.0;
        self.max_hr = 0.0;
        self.max_speed = 0.0;
        self.recent_power.clear();
    }

    fn accumulate(&mut self, now_ms: i64, dt: f64) {
        if let Some(s) = self.fresh(self.speed, now_ms, STALE_AFTER_MS) {
            self.avg_speed.push(s.value, dt);
            self.max_speed = self.max_speed.max(s.value);
        }
        if let Some(p) = self.fresh(self.power, now_ms, STALE_AFTER_MS) {
            self.avg_power.push(p.value, dt);
            self.max_power = self.max_power.max(p.value);
            self.recent_power.push_back(p.value);
            while self.recent_power.len() > 30 {
                self.recent_power.pop_front();
            }
        }
        if let Some(h) = self.fresh(self.heart_rate, now_ms, STALE_AFTER_MS) {
            self.avg_hr.push(h.value, dt);
            self.max_hr = self.max_hr.max(h.value);
        }
        if let Some(c) = self.fresh(self.cadence, now_ms, STALE_AFTER_MS) {
            self.avg_cadence.push(c.value, dt);
        }
    }

    fn make_sample(&mut self, now_ms: i64, elapsed_s: f64, sim_flags: u16) -> Sample {
        let speed = self.fresh(self.speed, now_ms, STALE_AFTER_MS);
        let power = self.fresh(self.power, now_ms, STALE_AFTER_MS);
        let hr = self.fresh(self.heart_rate, now_ms, STALE_AFTER_MS);
        let cadence = self.fresh(self.cadence, now_ms, STALE_AFTER_MS);
        let grade = self.fresh(self.grade, now_ms, STALE_AFTER_MS);
        let altitude = self.fresh(self.altitude, now_ms, STALE_AFTER_MS);
        let gps = self
            .gps
            .filter(|g| now_ms - g.2 <= STALE_AFTER_MS)
            .map(|g| (g.0, g.1, g.3));
        let origin = [
            speed.map(|r| r.origin),
            power.map(|r| r.origin),
            hr.map(|r| r.origin),
            cadence.map(|r| r.origin),
            self.gps.map(|g| g.3),
        ]
        .into_iter()
        .flatten()
        .find(|o| *o != DataOrigin::Unknown)
        .unwrap_or(DataOrigin::Unknown);
        let sample = Sample {
            t_ms: now_ms,
            elapsed_s,
            lat: gps.map(|g| g.0),
            lon: gps.map(|g| g.1),
            altitude_m: altitude.map(|r| r.value),
            distance_m: self.distance_m,
            speed_mps: speed.map(|r| r.value),
            power_w: power.map(|r| r.value),
            cadence_rpm: cadence.map(|r| r.value),
            heart_rate_bpm: hr.map(|r| r.value),
            grade_pct: grade.map(|r| r.value),
            sim_flags,
            origin,
        };
        self.last_sample = Some(sample.clone());
        sample
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SessionState {
    #[default]
    Idle,
    Running,
    Paused,
}

impl SessionState {
    pub fn is_active(self) -> bool {
        !matches!(self, Self::Idle)
    }
}

/// A ride in progress. Owns the sample buffer, distance integration and
/// pause/elapsed bookkeeping. The app periodically flushes `samples` to SQLite.
#[derive(Clone, Debug, Default)]
pub struct RideSession {
    pub state: SessionState,
    pub live: LiveMetrics,
    pub samples: Vec<Sample>,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub auto_pause: bool,
    pub route_distance_offset_m: f64,
    last_update_ms: i64,
    paused_total_ms: i64,
    pause_started_ms: Option<i64>,
    next_sample_ms: i64,
    last_gps_integrated: Option<(f64, f64, i64)>,
    last_altitude: Option<f32>,
    last_gain_altitude: Option<f32>,
    pub name: String,
    pub route_id: Option<i64>,
    pub route_name: Option<String>,
}

impl RideSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self, now_ms: i64) {
        self.reset();
        self.state = SessionState::Running;
        self.started_at_ms = Some(now_ms);
        self.last_update_ms = now_ms;
        self.next_sample_ms = now_ms;
        self.name = default_activity_name(now_ms);
    }

    /// Resume a session recovered from disk.
    pub fn restore(&mut self, started_at_ms: i64, elapsed_s: f64, samples: Vec<Sample>) {
        self.reset();
        self.state = SessionState::Paused;
        self.started_at_ms = Some(started_at_ms);
        self.live.elapsed_s = elapsed_s;
        self.samples = samples;
        if let Some(last) = self.samples.last() {
            self.live.distance_m = last.distance_m;
            self.live.altitude = last.altitude_m.map(|v| Reading::new(v, last.t_ms, false, DataOrigin::Unknown));
        }
        self.last_update_ms = crate::types::now_ms();
        self.next_sample_ms = self.last_update_ms;
    }

    pub fn reset(&mut self) {
        let name = std::mem::take(&mut self.name);
        let route_id = self.route_id;
        let route_name = self.route_name.take();
        *self = Self::default();
        self.name = name;
        self.route_id = route_id;
        self.route_name = route_name;
    }

    pub fn pause(&mut self, now_ms: i64) {
        if self.state == SessionState::Running {
            self.state = SessionState::Paused;
            self.pause_started_ms = Some(now_ms);
            self.live.elapsed_s = self.elapsed_s(now_ms);
        }
    }

    pub fn resume(&mut self, now_ms: i64) {
        if self.state == SessionState::Paused {
            if let Some(p) = self.pause_started_ms.take() {
                self.paused_total_ms += (now_ms - p).max(0);
            }
            self.state = SessionState::Running;
            self.last_update_ms = now_ms;
            self.next_sample_ms = now_ms;
        }
    }

    pub fn elapsed_s(&self, now_ms: i64) -> f64 {
        let Some(start) = self.started_at_ms else {
            return 0.0;
        };
        let mut paused = self.paused_total_ms;
        if let Some(p) = self.pause_started_ms {
            paused += (now_ms - p).max(0);
        }
        ((now_ms - start - paused).max(0) as f64) / 1000.0
    }

    /// Finish the ride and return all samples. The caller persists the activity.
    pub fn finish(&mut self, now_ms: i64) -> Vec<Sample> {
        if self.state == SessionState::Running {
            self.update(now_ms);
        }
        self.live.elapsed_s = self.elapsed_s(now_ms);
        self.finished_at_ms = Some(now_ms);
        self.state = SessionState::Idle;
        self.samples.clone()
    }

    /// Integrate telemetry; emits at most one sample per second.
    pub fn update(&mut self, now_ms: i64) {
        if self.state != SessionState::Running {
            if self.state == SessionState::Paused {
                self.live.elapsed_s = self.elapsed_s(now_ms);
            }
            return;
        }
        let dt = ((now_ms - self.last_update_ms).max(0) as f64 / 1000.0).min(2.0);
        self.last_update_ms = now_ms;
        self.live.elapsed_s = self.elapsed_s(now_ms);

        // Distance: prefer fresh GPS, fall back to integrated wheel speed.
        let mut gps_used = false;
        if let Some((lat, lon, t, origin)) = self.live.gps {
            if now_ms - t <= 3_000 {
                if let Some((plat, plon, pt)) = self.last_gps_integrated {
                    let dt_gps = ((now_ms - pt).max(0) as f64) / 1000.0;
                    if dt_gps > 0.2 && dt_gps < 5.0 {
                        let d = haversine_m(plat, plon, lat, lon);
                        if d < 500.0 && d > 0.1 {
                            self.live.distance_m += d;
                            gps_used = true;
                            if origin == DataOrigin::Simulated {
                                // GPS distance is still marked simulated via the flag.
                            }
                        }
                    }
                }
                self.last_gps_integrated = Some((lat, lon, now_ms));
            }
        }
        if !gps_used {
            if let Some(speed) = self.live.current_speed(now_ms) {
                self.live.distance_m += speed.value as f64 * dt;
            }
        }

        // Elevation gain from the barometric/GPS altitude channel.
        if let Some(alt) = self.live.fresh(self.live.altitude, now_ms, STALE_AFTER_MS) {
            if let Some(prev) = self.last_altitude {
                let delta = alt.value - prev;
                if let Some(gain_prev) = self.last_gain_altitude {
                    let d = alt.value - gain_prev;
                    if d > 0.5 {
                        self.live.elevation_gain_m += d;
                        self.last_gain_altitude = Some(alt.value);
                    } else if d < -0.5 {
                        self.last_gain_altitude = Some(alt.value);
                    }
                } else {
                    self.last_gain_altitude = Some(alt.value);
                }
                let _ = delta;
            } else {
                self.last_altitude = Some(alt.value);
                self.last_gain_altitude = Some(alt.value);
            }
        }

        self.live.accumulate(now_ms, dt);

        if now_ms >= self.next_sample_ms {
            let sim_flags = self.live.sim_flags();
            let elapsed = self.live.elapsed_s;
            let sample = self.live.make_sample(now_ms, elapsed, sim_flags);
            self.samples.push(sample);
            self.next_sample_ms = now_ms + 1_000;
            // Keep the in-memory buffer bounded for very long rides.
            if self.samples.len() > 100_000 {
                self.samples.drain(0..5_000);
            }
        }
    }

    pub fn distance_m(&self) -> f64 {
        self.live.distance_m
    }

    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    pub fn simulated(&self) -> bool {
        self.live.sim_flags() != 0
    }
}

pub fn default_activity_name(started_at_ms: i64) -> String {
    use chrono::{Local, TimeZone};
    let dt = Local
        .timestamp_millis_opt(started_at_ms)
        .single()
        .map(|d| {
            let hour = d.format("%H").to_string().parse::<u32>().unwrap_or(12);
            match hour {
                5..=11 => "Morning Ride",
                12..=16 => "Afternoon Ride",
                17..=21 => "Evening Ride",
                _ => "Night Ride",
            }
        })
        .unwrap_or("Ride");
    format!("{dt} · {}", Local.timestamp_millis_opt(started_at_ms).single().map(|d| d.format("%d %b").to_string()).unwrap_or_default())
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp * 0.5).sin().powi(2) + p1.cos() * p2.cos() * (dl * 0.5).sin().powi(2);
    2.0 * 6_371_008.8 * a.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_power(session: &mut RideSession, now: i64, watts: f32) {
        session.live.set(
            Metric::Power,
            Reading::new(watts, now, true, DataOrigin::Simulated),
        );
        session.live.set(
            Metric::Speed,
            Reading::new(8.0, now, true, DataOrigin::Simulated),
        );
        session.live.set(
            Metric::HeartRate,
            Reading::new(150.0, now, true, DataOrigin::Simulated),
        );
        session.update(now);
    }

    #[test]
    fn samples_are_emitted_at_one_hz() {
        let mut s = RideSession::new();
        s.start(0);
        for i in 0..350i64 {
            let t = i * 10;
            if t % 1000 == 0 {
                feed_power(&mut s, t, 200.0);
            }
        }
        assert!(s.samples.len() >= 3, "got {}", s.samples.len());
        assert!(
            (s.live.distance_m - 8.0 * 3.0).abs() < 2.0,
            "distance was {} m",
            s.live.distance_m
        );
        assert_eq!(s.samples[0].power_w, Some(200.0));
        assert!(s.simulated());
    }

    #[test]
    fn pause_does_not_accumulate() {
        let mut s = RideSession::new();
        s.start(0);
        feed_power(&mut s, 1000, 200.0);
        let dist = s.live.distance_m;
        s.pause(2000);
        feed_power(&mut s, 3000, 200.0);
        assert!((s.live.distance_m - dist).abs() < 0.01);
        s.resume(5000);
        assert_eq!(s.elapsed_s(6000), 3.0);
    }

    #[test]
    fn rejects_implausible_values() {
        assert!(sanitize(Metric::HeartRate, 400.0).is_none());
        assert!(sanitize(Metric::Power, -10.0).is_none());
        assert!(sanitize(Metric::Power, f32::NAN).is_none());
        let mut s = RideSession::new();
        s.live.set(
            Metric::Power,
            Reading::new(9999.0, 0, false, DataOrigin::Bluetooth),
        );
        assert!(s.live.power.is_none());
    }

    #[test]
    fn stale_data_is_not_recorded() {
        let mut s = RideSession::new();
        s.start(0);
        s.live.set(
            Metric::Power,
            Reading::new(200.0, 0, false, DataOrigin::Bluetooth),
        );
        s.live.set(
            Metric::Speed,
            Reading::new(8.0, 0, false, DataOrigin::Bluetooth),
        );
        s.update(10_000);
        let last = s.live.last_sample.as_ref().unwrap();
        assert!(last.power_w.is_none(), "stale power leaked into sample");
        assert!(last.speed_mps.is_none(), "stale speed leaked into sample");
    }

    #[test]
    fn gps_distance_integration() {
        let mut s = RideSession::new();
        s.start(0);
        s.live.set_gps(48.0000, 11.0000, 1_000, DataOrigin::Gps);
        s.update(1_000);
        // ~111 m north.
        s.live.set_gps(48.0010, 11.0000, 2_000, DataOrigin::Gps);
        s.update(2_000);
        assert!(s.live.distance_m > 100.0 && s.live.distance_m < 120.0);
    }
}
