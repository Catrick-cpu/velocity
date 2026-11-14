//! Deterministic indoor-ride simulator.
//!
//! The simulator exists for two reasons:
//!
//! 1. VELOCITY must be fully usable, and demonstrable, without a trainer.
//! 2. Screenshots and tests need repeatable data.
//!
//! Every value produced here is marked [`DataOrigin::Simulated`] and the app
//! shows a persistent SIM badge. The model reuses the same physics as virtual
//! speed and trainer resistance, so the numbers are plausible rather than
//! random.

use velocity_core::physics::{Environment, ResistiveModel};
use velocity_core::types::{BikeProfile, DataOrigin, Reading, RiderProfile};

#[derive(Clone, Debug)]
pub struct SimulatorConfig {
    pub seed: u64,
    pub ftp_w: f32,
    pub weight_kg: f32,
    pub max_hr_bpm: f32,
    pub resting_hr_bpm: f32,
    pub bike_mass_kg: f32,
    pub cda_m2: f32,
    pub crr: f32,
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        let rider = RiderProfile::default();
        let bike = BikeProfile::default();
        Self {
            seed: 0x5EED_1234_ABCD_0001,
            ftp_w: rider.ftp_w,
            weight_kg: rider.weight_kg,
            max_hr_bpm: rider.max_hr_bpm,
            resting_hr_bpm: rider.resting_hr_bpm,
            bike_mass_kg: bike.mass_kg,
            cda_m2: bike.cda_m2,
            crr: bike.crr,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SimReadings {
    pub power_w: f32,
    pub cadence_rpm: f32,
    pub heart_rate_bpm: f32,
    pub speed_mps: f32,
    pub grade_pct: f32,
}

impl SimReadings {
    pub fn power(&self, t_ms: i64) -> Reading {
        Reading::new(self.power_w, t_ms, true, DataOrigin::Simulated)
    }

    pub fn cadence(&self, t_ms: i64) -> Reading {
        Reading::new(self.cadence_rpm, t_ms, true, DataOrigin::Simulated)
    }

    pub fn heart_rate(&self, t_ms: i64) -> Reading {
        Reading::new(self.heart_rate_bpm, t_ms, true, DataOrigin::Simulated)
    }

    pub fn speed(&self, t_ms: i64) -> Reading {
        Reading::new(self.speed_mps, t_ms, true, DataOrigin::Simulated)
    }

    pub fn grade(&self, t_ms: i64) -> Reading {
        Reading::new(self.grade_pct, t_ms, true, DataOrigin::Simulated)
    }
}

/// Small deterministic PRNG (xorshift64*); no dependency, stable across runs.
#[derive(Clone, Debug)]
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in [-1, 1].
    fn signed(&mut self) -> f32 {
        ((self.next_u64() >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0) as f32
    }
}

#[derive(Clone, Debug)]
pub struct Simulator {
    cfg: SimulatorConfig,
    rng: Rng,
    model: ResistiveModel,
    target_power_w: f32,
    grade_pct: f32,
    power_w: f32,
    cadence_rpm: f32,
    heart_rate_bpm: f32,
    speed_mps: f32,
    last_ms: Option<i64>,
}

impl Simulator {
    pub fn new(cfg: SimulatorConfig) -> Self {
        let rider = RiderProfile {
            weight_kg: cfg.weight_kg,
            ftp_w: cfg.ftp_w,
            max_hr_bpm: cfg.max_hr_bpm,
            resting_hr_bpm: cfg.resting_hr_bpm,
            ..RiderProfile::default()
        };
        let bike = BikeProfile {
            mass_kg: cfg.bike_mass_kg,
            cda_m2: cfg.cda_m2,
            crr: cfg.crr,
            ..BikeProfile::default()
        };
        let model = ResistiveModel::new(&rider, &bike, Environment::default());
        let resting_hr_bpm = cfg.resting_hr_bpm;
        Self {
            rng: Rng(cfg.seed | 1),
            cfg,
            model,
            target_power_w: 0.0,
            grade_pct: 0.0,
            power_w: 0.0,
            cadence_rpm: 0.0,
            heart_rate_bpm: resting_hr_bpm,
            speed_mps: 0.0,
            last_ms: None,
        }
    }

    pub fn reset(&mut self, now_ms: i64) {
        let cfg = self.cfg.clone();
        *self = Self::new(cfg);
        self.last_ms = Some(now_ms);
    }

    pub fn set_target_power(&mut self, watts: f32) {
        self.target_power_w = watts.clamp(0.0, 2500.0);
    }

    pub fn target_power(&self) -> f32 {
        self.target_power_w
    }

    pub fn set_grade(&mut self, grade_pct: f32) {
        self.grade_pct = grade_pct.clamp(-25.0, 25.0);
    }

    pub fn grade(&self) -> f32 {
        self.grade_pct
    }

    pub fn config(&self) -> &SimulatorConfig {
        &self.cfg
    }

    /// Advance the simulation to `now_ms` and return the current readings.
    pub fn tick(&mut self, now_ms: i64) -> SimReadings {
        let dt = match self.last_ms {
            None => 0.25,
            Some(prev) => ((now_ms - prev).max(0) as f32 / 1000.0).clamp(0.05, 2.0),
        };
        self.last_ms = Some(now_ms);

        // Power follows the target with crank-like smoothness. When the rider
        // stops pedalling, power and cadence decay quickly instead of jittering.
        let power_alpha = (1.0 - (-dt / 2.2).exp()).clamp(0.0, 1.0);
        if self.target_power_w <= 5.0 {
            self.power_w = (self.power_w - dt * 40.0).max(0.0);
        } else {
            let pedal_noise = self.rng.signed() * 4.0 * dt.sqrt();
            self.power_w += (self.target_power_w - self.power_w) * power_alpha;
            self.power_w = (self.power_w + pedal_noise).clamp(0.0, 2500.0);
        }

        // Cadence has a slow natural oscillation plus a little jitter.
        let phase = now_ms as f32 / 1000.0;
        let cadence_target = if self.target_power_w > 5.0 {
            86.0 + 5.0 * (phase / 17.0).sin() + self.power_w / self.cfg.ftp_w.max(1.0) * 3.0
        } else {
            0.0
        };
        let cadence_alpha = (1.0 - (-dt / 1.5).exp()).clamp(0.0, 1.0);
        self.cadence_rpm += (cadence_target - self.cadence_rpm) * cadence_alpha;
        self.cadence_rpm = (self.cadence_rpm + self.rng.signed() * 0.6).clamp(0.0, 140.0);

        // Virtual speed from the same power balance used for trainer resistance.
        let target_speed = self.model.speed_for_power(self.power_w, self.grade_pct);
        let tau = if target_speed > self.speed_mps { 3.0 } else { 6.0 };
        let speed_alpha = (1.0 - (-dt / tau).exp()).clamp(0.0, 1.0);
        self.speed_mps += (target_speed - self.speed_mps) * speed_alpha;
        self.speed_mps = self.speed_mps.clamp(0.0, 30.0);

        // Heart rate lags effort: fast rise, slower recovery.
        let ftp = self.cfg.ftp_w.max(1.0);
        let intensity = (self.power_w / ftp).clamp(0.0, 1.8);
        let fraction = (0.30 + 0.62 * intensity.min(1.0) + 0.18 * (intensity - 1.0).max(0.0))
            .clamp(0.0, 1.0);
        let hr_target =
            self.cfg.resting_hr_bpm + (self.cfg.max_hr_bpm - self.cfg.resting_hr_bpm) * fraction;
        let hr_tau = if hr_target > self.heart_rate_bpm { 22.0 } else { 55.0 };
        let hr_alpha = (1.0 - (-dt / hr_tau).exp()).clamp(0.0, 1.0);
        self.heart_rate_bpm += (hr_target - self.heart_rate_bpm) * hr_alpha;
        self.heart_rate_bpm = (self.heart_rate_bpm + self.rng.signed() * 0.7).clamp(35.0, 220.0);

        SimReadings {
            power_w: self.power_w,
            cadence_rpm: self.cadence_rpm,
            heart_rate_bpm: self.heart_rate_bpm,
            speed_mps: self.speed_mps,
            grade_pct: self.grade_pct,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(seconds: u64, target: f32, grade: f32) -> Simulator {
        let mut sim = Simulator::new(SimulatorConfig::default());
        sim.set_target_power(target);
        sim.set_grade(grade);
        for t in 0..=seconds {
            sim.tick(t as i64 * 1000);
        }
        sim
    }

    #[test]
    fn settles_near_erg_target() {
        let mut sim = run(120, 200.0, 0.0);
        let r = sim.tick(121_000);
        assert!(
            (r.power_w - 200.0).abs() < 8.0,
            "ERG target power was {:.1} W",
            r.power_w
        );
        assert!((r.cadence_rpm - 89.0).abs() < 12.0, "cadence {:.1}", r.cadence_rpm);
        assert!(
            (25.0..38.0).contains(&(r.speed_mps * 3.6)),
            "flat speed {:.1} km/h",
            r.speed_mps * 3.6
        );
        assert!(
            (150.0..190.0).contains(&r.heart_rate_bpm),
            "heart rate {:.1}",
            r.heart_rate_bpm
        );
    }

    #[test]
    fn climbing_is_slower_than_flat() {
        let flat = run(150, 220.0, 0.0).tick(151_000);
        let climb = run(150, 220.0, 7.0).tick(151_000);
        assert!(
            climb.speed_mps < flat.speed_mps - 1.5,
            "climb {:.1} vs flat {:.1} m/s",
            climb.speed_mps,
            flat.speed_mps
        );
    }

    #[test]
    fn same_seed_is_reproducible() {
        let a = run(60, 180.0, 1.0).tick(61_000);
        let b = run(60, 180.0, 1.0).tick(61_000);
        assert_eq!(a, b);
    }

    #[test]
    fn readings_are_marked_simulated() {
        let mut sim = run(10, 150.0, 0.0);
        let r = sim.tick(11_000);
        assert!(r.power(0).simulated);
        assert_eq!(r.power(0).origin, DataOrigin::Simulated);
        assert_eq!(r.heart_rate(0).origin, DataOrigin::Simulated);
    }

    #[test]
    fn coasting_slows_then_stops() {
        let mut sim = Simulator::new(SimulatorConfig::default());
        sim.set_target_power(250.0);
        for t in 0..30 {
            sim.tick(t * 1000);
        }
        sim.set_target_power(0.0);
        for t in 30..150 {
            sim.tick(t * 1000);
        }
        let r = sim.tick(150_000);
        assert!(r.speed_mps < 0.5, "coasting speed {:.2} m/s", r.speed_mps);
    }
}
