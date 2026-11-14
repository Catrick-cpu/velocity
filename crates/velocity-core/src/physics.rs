//! Physically plausible cycling model used for virtual speed, elevation-aware
//! trainer resistance and ride simulations.
//!
//! The model solves the steady-state power balance
//!
//! `P = v · (m·g·sin θ + Crr·m·g·cos θ + ½·ρ·CdA·(v + wind)²)`
//!
//! with a monotonic bisection solve. It is intentionally simple, transparent
//! and fast (no allocation), which is ideal for a 1 Hz trainer control loop.

use crate::types::{BikeProfile, RiderProfile};

pub const GRAVITY: f32 = 9.80665;
/// Air density at ~20 °C and sea level, kg/m³.
pub const AIR_DENSITY: f32 = 1.204;

#[derive(Clone, Copy, Debug)]
pub struct Environment {
    pub air_density: f32,
    /// Positive = headwind, negative = tailwind, m/s.
    pub wind_mps: f32,
    pub gravity: f32,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            air_density: AIR_DENSITY,
            wind_mps: 0.0,
            gravity: GRAVITY,
        }
    }
}

impl Environment {
    pub fn with_wind(wind_mps: f32) -> Self {
        Self {
            wind_mps,
            ..Default::default()
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ResistiveModel {
    pub total_mass_kg: f32,
    pub crr: f32,
    pub cda_m2: f32,
    pub env: Environment,
}

impl ResistiveModel {
    pub fn new(rider: &RiderProfile, bike: &BikeProfile, env: Environment) -> Self {
        Self {
            total_mass_kg: rider.total_mass_kg(bike).max(30.0),
            crr: bike.crr.max(0.0005),
            cda_m2: bike.cda_m2.max(0.05),
            env,
        }
    }

    #[inline]
    pub fn grade_radians(grade_pct: f32) -> f32 {
        (grade_pct / 100.0).atan()
    }

    /// Total resistive force in newtons at the given ground speed.
    pub fn resistive_force(&self, speed_mps: f32, grade_pct: f32) -> f32 {
        let theta = Self::grade_radians(grade_pct);
        let m = self.total_mass_kg;
        let g = self.env.gravity;
        let gravity_force = m * g * theta.sin();
        let rolling_force = self.crr * m * g * theta.cos();
        let air_speed = (speed_mps + self.env.wind_mps).max(0.0);
        let drag_force = 0.5 * self.env.air_density * self.cda_m2 * air_speed * air_speed;
        gravity_force + rolling_force + drag_force
    }

    /// Mechanical power required to hold `speed_mps` on `grade_pct`.
    pub fn required_power(&self, speed_mps: f32, grade_pct: f32) -> f32 {
        speed_mps.max(0.0) * self.resistive_force(speed_mps, grade_pct)
    }

    /// Solve ground speed for a given mechanical power.
    ///
    /// Handles coasting (0 W) by solving force equilibrium on descents.
    pub fn speed_for_power(&self, power_w: f32, grade_pct: f32) -> f32 {
        let power = power_w.max(0.0);
        if power < 1.0 {
            if grade_pct < -0.2 {
                return self.terminal_coast_speed(grade_pct);
            }
            return 0.0;
        }

        // P(v)/v increases monotonically for v >= 0 on realistic grades.
        let mut lo = 0.0f32;
        let mut hi = 35.0f32;
        // Expand the bracket for extreme descents / tailwinds.
        while self.required_power(hi, grade_pct) < power && hi < 45.0 {
            hi += 5.0;
        }
        for _ in 0..48 {
            let mid = 0.5 * (lo + hi);
            if self.required_power(mid, grade_pct) < power {
                lo = mid;
            } else {
                hi = mid;
            }
            if hi - lo < 0.0005 {
                break;
            }
        }
        0.5 * (lo + hi)
    }

    /// Speed at which gravity, rolling resistance and drag cancel out.
    pub fn terminal_coast_speed(&self, grade_pct: f32) -> f32 {
        let mut lo = 0.5f32;
        let mut hi = 40.0f32;
        for _ in 0..48 {
            let mid = 0.5 * (lo + hi);
            if self.resistive_force(mid, grade_pct) > 0.0 {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        0.5 * (lo + hi)
    }

    /// Power needed to accelerate, in addition to holding speed.
    pub fn acceleration_power(&self, speed_mps: f32, acceleration_mps2: f32) -> f32 {
        self.total_mass_kg * acceleration_mps2.max(0.0) * speed_mps
    }
}

/// Estimate calories from mechanical work. Human gross efficiency is roughly
/// 24 %; 1 kcal = 4.184 kJ, so kcal ≈ work_kj / (4.184 · 0.24).
pub fn calories_from_work_kj(work_kj: f32) -> f32 {
    (work_kj / (4.184 * 0.24)).max(0.0)
}

/// MET-based fallback when no power meter was recorded.
pub fn calories_from_met(speed_kmh: f32, weight_kg: f32, duration_s: f32) -> f32 {
    let met = if speed_kmh < 16.0 {
        4.0
    } else if speed_kmh < 20.0 {
        6.8
    } else if speed_kmh < 25.0 {
        8.0
    } else if speed_kmh < 30.0 {
        10.0
    } else if speed_kmh < 35.0 {
        12.0
    } else {
        15.0
    };
    met * weight_kg * duration_s / 3600.0
}

/// Trainer grade as reported to FTMS: 0.01 % units, clamped to a sane range.
pub fn trainer_grade_centipercent(grade_pct: f32) -> i16 {
    (grade_pct.clamp(-40.0, 40.0) * 100.0).round() as i16
}

/// Air density from temperature and barometric pressure (approximation).
pub fn air_density(temp_c: f32, pressure_hpa: f32) -> f32 {
    let t_k = temp_c + 273.15;
    (pressure_hpa * 100.0) / (287.05 * t_k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BikeProfile, RiderProfile};

    fn model() -> ResistiveModel {
        ResistiveModel::new(
            &RiderProfile::default(),
            &BikeProfile::default(),
            Environment::default(),
        )
    }

    #[test]
    fn flat_speed_is_realistic() {
        let m = model();
        let v = m.speed_for_power(200.0, 0.0);
        let kmh = v * 3.6;
        assert!((29.0..35.5).contains(&kmh), "200 W flat gave {kmh:.1} km/h");
    }

    #[test]
    fn climb_is_slower() {
        let m = model();
        let flat = m.speed_for_power(250.0, 0.0);
        let climb = m.speed_for_power(250.0, 6.0);
        assert!(climb < flat);
        assert!((10.0..17.0).contains(&(climb * 3.6)), "250 W @ 6% gave {:.1} km/h", climb * 3.6);
    }

    #[test]
    fn power_roundtrip() {
        let m = model();
        for grade in [-6.0, -2.0, 0.0, 3.0, 9.0] {
            for power in [120.0, 220.0, 400.0] {
                let v = m.speed_for_power(power, grade);
                let p = m.required_power(v, grade);
                assert!((p - power).abs() < 2.0, "grade {grade} power {power} -> {p}");
            }
        }
    }

    #[test]
    fn descent_coasts() {
        let m = model();
        let v = m.speed_for_power(0.0, -5.0);
        assert!(v * 3.6 > 25.0, "coast on -5% gave {:.1} km/h", v * 3.6);
        assert_eq!(m.speed_for_power(0.0, 0.0), 0.0);
    }

    #[test]
    fn headwind_slows_you() {
        let rider = RiderProfile::default();
        let bike = BikeProfile::default();
        let calm = ResistiveModel::new(&rider, &bike, Environment::default());
        let windy = ResistiveModel::new(&rider, &bike, Environment::with_wind(5.0));
        let v1 = calm.speed_for_power(250.0, 0.0);
        let v2 = windy.speed_for_power(250.0, 0.0);
        assert!(v2 < v1 - 1.0);
    }

    #[test]
    fn calories_are_sane() {
        // 200 W for one hour ≈ 720 kJ ≈ 717 kcal at 24 % efficiency.
        let kcal = calories_from_work_kj(720.0);
        assert!((690.0..740.0).contains(&kcal));
    }

    #[test]
    fn trainer_grade_units() {
        assert_eq!(trainer_grade_centipercent(8.0), 800);
        assert_eq!(trainer_grade_centipercent(-3.25), -325);
        assert_eq!(trainer_grade_centipercent(99.0), 4000);
    }
}
