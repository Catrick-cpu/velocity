//! Deterministic demo data so the UI and analytics are fully explorable
//! without hardware. Everything is marked as simulated in the sample flags.

use crate::analytics;
use crate::physics::{Environment, ResistiveModel};
use crate::types::{Activity, BikeProfile, DataOrigin, RiderProfile, Sample, Sex, Sport, sim, now_ms};
use chrono::{Duration, Local, TimeZone};

struct RideSeed {
    days_ago: i64,
    hour: u32,
    minutes: u32,
    base_power: f32,
    intensity: f32,
    name: &'static str,
    sport: Sport,
}

pub fn demo_activities() -> Vec<(Activity, Vec<Sample>)> {
    let seeds = [
        RideSeed {
            days_ago: 1,
            hour: 17,
            minutes: 42,
            base_power: 195.0,
            intensity: 0.86,
            name: "Evening Endurance",
            sport: Sport::VirtualRide,
        },
        RideSeed {
            days_ago: 3,
            hour: 7,
            minutes: 28,
            base_power: 168.0,
            intensity: 0.74,
            name: "Morning Spin",
            sport: Sport::VirtualRide,
        },
        RideSeed {
            days_ago: 5,
            hour: 16,
            minutes: 55,
            base_power: 232.0,
            intensity: 1.05,
            name: "Sweet Spot Blocks",
            sport: Sport::VirtualRide,
        },
        RideSeed {
            days_ago: 8,
            hour: 9,
            minutes: 12,
            base_power: 205.0,
            intensity: 0.92,
            name: "Rolling Hills",
            sport: Sport::VirtualRide,
        },
        RideSeed {
            days_ago: 12,
            hour: 18,
            minutes: 5,
            base_power: 150.0,
            intensity: 0.68,
            name: "Recovery Cruise",
            sport: Sport::VirtualRide,
        },
        RideSeed {
            days_ago: 16,
            hour: 10,
            minutes: 30,
            base_power: 240.0,
            intensity: 1.08,
            name: "Threshold Session",
            sport: Sport::VirtualRide,
        },
        RideSeed {
            days_ago: 22,
            hour: 8,
            minutes: 15,
            base_power: 178.0,
            intensity: 0.80,
            name: "Long Ride",
            sport: Sport::VirtualRide,
        },
        RideSeed {
            days_ago: 29,
            hour: 17,
            minutes: 20,
            base_power: 212.0,
            intensity: 0.95,
            name: "Tempo Loop",
            sport: Sport::VirtualRide,
        },
    ];

    let rider = RiderProfile {
        age: 34,
        sex: Sex::Unspecified,
        weight_kg: 76.0,
        ..Default::default()
    };
    let bike = BikeProfile::default();
    let model = ResistiveModel::new(&rider, &bike, Environment::default());
    let today = Local::now().date_naive();

    let mut out = Vec::new();
    for (idx, seed) in seeds.iter().enumerate() {
        let date = today - Duration::days(seed.days_ago);
        let Some(naive) = date.and_hms_opt(seed.hour, 0, 0) else {
            continue;
        };
        let Some(start_dt) = Local.from_local_datetime(&naive).single() else {
            continue;
        };
        let start_ms = start_dt.timestamp_millis() + seed.minutes as i64 * 60_000;
        let duration_s = 1_800 + (idx as u32 * 317) % 3_600;
        let samples = build_ride(
            start_ms,
            duration_s,
            seed.base_power,
            seed.intensity,
            idx as f32 * 1.7,
            &model,
        );
        let summary = analytics::summarize(&samples, rider.ftp_w);
        let activity = Activity {
            id: 0,
            name: format!("{} · {}", seed.name, date.format("%d %b")),
            sport: seed.sport,
            started_at_ms: start_ms,
            ended_at_ms: Some(start_ms + duration_s as i64 * 1000),
            summary,
            route_id: None,
            route_name: Some("Starnberger See · Demo Loop".to_string()),
            source: "VELOCITY demo data (simulated)".to_string(),
            notes: "Demo ride generated locally. Import real rides to replace it.".to_string(),
            created_at_ms: now_ms(),
        };
        out.push((activity, samples));
    }
    out
}

fn build_ride(
    start_ms: i64,
    duration_s: u32,
    base_power: f32,
    intensity: f32,
    phase: f32,
    model: &ResistiveModel,
) -> Vec<Sample> {
    let mut samples = Vec::with_capacity(duration_s as usize + 1);
    let mut distance = 0.0f64;
    let mut altitude = 560.0f32;
    let mut hr = 95.0f32;
    let mut prev_alt = altitude;
    // Smoothed random-ish terrain derived from two sine waves plus intervals.
    for i in 0..=duration_s {
        let t = i as f32;
        let terrain_phase = t / 240.0 + phase;
        let grade = (terrain_phase.sin() * 3.8 + (terrain_phase * 0.37).cos() * 1.9) * intensity;
        altitude += grade / 100.0 * 7.0;
        let _ = prev_alt;
        prev_alt = altitude;

        // Interval structure: blocks of work and recovery.
        let block = ((t / 300.0).floor() as u32) % 3;
        let surge = match block {
            0 => 1.06,
            1 => 1.22,
            _ => 0.92,
        };
        let wave = 1.0 + 0.05 * (t / 17.0).sin() + 0.03 * (t / 5.3).cos();
        let warmup = (t / 240.0).min(1.0);
        let cooldown = ((duration_s as f32 - t) / 180.0).clamp(0.0, 1.0);
        let power = (base_power * surge * wave * warmup * cooldown).max(40.0);
        let speed = model.speed_for_power(power, grade);
        distance += speed as f64;
        // Heart rate follows power with a first-order lag.
        let target_hr = 92.0 + power * 0.245;
        hr += (target_hr - hr) * 0.055;
        hr = hr.clamp(85.0, 181.0);
        let cadence = (88.0 + (t / 23.0).sin() * 6.0 + (power - base_power) * 0.015).clamp(65.0, 108.0);

        samples.push(Sample {
            t_ms: start_ms + i as i64 * 1000,
            elapsed_s: i as f64,
            lat: None,
            lon: None,
            altitude_m: Some(altitude),
            distance_m: distance,
            speed_mps: Some(speed),
            power_w: Some(power),
            cadence_rpm: Some(cadence),
            heart_rate_bpm: Some(hr),
            grade_pct: Some(grade),
            sim_flags: sim::ALL,
            origin: DataOrigin::Simulated,
        });
    }
    samples
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_data_is_rich_and_plausible() {
        let rides = demo_activities();
        assert_eq!(rides.len(), 8);
        let mut total_tss = 0.0;
        for (activity, samples) in &rides {
            assert!(samples.len() > 1_000);
            assert!(activity.summary.distance_m > 5_000.0);
            assert!(activity.summary.avg_power_w.unwrap() > 50.0);
            assert!(activity.summary.avg_heart_rate_bpm.unwrap() > 90.0);
            assert!(activity.summary.tss.unwrap() > 5.0);
            assert!(activity.summary.elevation_gain_m > 0.0);
            assert!(samples.iter().all(|s| s.origin == DataOrigin::Simulated));
            total_tss += activity.summary.tss.unwrap();
        }
        assert!(total_tss > 100.0);
    }
}
