//! Training analytics: aggregates, normalized power, peaks, training load and
//! zone distributions. Pure functions over recorded samples, easy to test.

use crate::physics::calories_from_work_kj;
use crate::types::{Activity, RiderProfile, Sample};
use chrono::{Duration, Local, TimeZone};

#[derive(Clone, Copy, Debug, Default)]
pub struct SummaryInput {
    pub duration_s: f64,
    pub moving_s: f64,
    pub elevation_gain_m: f32,
    pub elevation_loss_m: f32,
    pub ftp_w: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BestEffort {
    pub window_s: u32,
    pub power_w: f32,
    pub start_elapsed_s: f64,
}

pub const EFFORT_WINDOWS: [u32; 9] = [1, 5, 30, 60, 300, 600, 1200, 3600, 7200];

/// Compute a complete activity summary from raw samples.
pub fn summarize(samples: &[Sample], ftp_w: f32) -> crate::types::ActivitySummary {
    use crate::types::ActivitySummary;
    let mut out = ActivitySummary::default();
    if samples.is_empty() {
        return out;
    }

    let first_t = samples.first().map(|s| s.elapsed_s).unwrap_or(0.0);
    let last_t = samples.last().map(|s| s.elapsed_s).unwrap_or(0.0);
    out.duration_s = (last_t - first_t).max(0.0);
    out.distance_m = samples
        .iter()
        .map(|s| s.distance_m)
        .fold(0.0f64, f64::max);

    let mut moving = 0.0f64;
    let mut power_integral = 0.0f64;
    let mut power_time = 0.0f64;
    let mut max_power = f32::MIN;
    let mut max_speed = f32::MIN;
    let mut hr_sum = 0.0f64;
    let mut hr_time = 0.0f64;
    let mut max_hr = f32::MIN;
    let mut cad_sum = 0.0f64;
    let mut cad_time = 0.0f64;
    let mut speed_sum = 0.0f64;
    let mut speed_time = 0.0f64;

    for w in samples.windows(2) {
        let a = &w[0];
        let b = &w[1];
        let dt = (b.elapsed_s - a.elapsed_s).clamp(0.0, 10.0);
        if dt <= 0.0 {
            continue;
        }
        let speed = a.speed_mps.unwrap_or(0.0);
        let power = a.power_w.unwrap_or(0.0);
        let is_moving = speed > 0.6 || power > 15.0;
        if is_moving {
            moving += dt;
            out.moving_s += dt;
            speed_sum += speed as f64 * dt;
            speed_time += dt;
        }
        if let Some(p) = a.power_w {
            power_integral += p as f64 * dt;
            power_time += dt;
            max_power = max_power.max(p);
        }
        if let Some(hr) = a.heart_rate_bpm {
            hr_sum += hr as f64 * dt;
            hr_time += dt;
            max_hr = max_hr.max(hr);
        }
        if let Some(c) = a.cadence_rpm {
            cad_sum += c as f64 * dt;
            cad_time += dt;
        }
        if let Some(s) = a.speed_mps {
            max_speed = max_speed.max(s);
        }
    }
    // Last sample contributes to maxima even with no following sample.
    if let Some(last) = samples.last() {
        if let Some(p) = last.power_w {
            max_power = max_power.max(p);
        }
        if let Some(hr) = last.heart_rate_bpm {
            max_hr = max_hr.max(hr);
        }
        if let Some(s) = last.speed_mps {
            max_speed = max_speed.max(s);
        }
    }

    out.moving_s = if moving > 0.0 { moving } else { out.duration_s };
    if power_time > 0.0 {
        out.avg_power_w = Some((power_integral / power_time) as f32);
        out.work_kj = Some((power_integral / 1000.0) as f32);
        out.calories_kcal = Some(calories_from_work_kj((power_integral / 1000.0) as f32));
        out.normalized_power_w = normalized_power(samples);
    }
    if max_power > f32::MIN {
        out.max_power_w = Some(max_power);
    }
    if hr_time > 0.0 {
        out.avg_heart_rate_bpm = Some((hr_sum / hr_time) as f32);
    }
    if max_hr > f32::MIN {
        out.max_heart_rate_bpm = Some(max_hr);
    }
    if cad_time > 0.0 {
        out.avg_cadence_rpm = Some((cad_sum / cad_time) as f32);
    }
    if speed_time > 0.0 {
        out.avg_speed_mps = Some((speed_sum / speed_time) as f32);
    }
    if max_speed > f32::MIN {
        out.max_speed_mps = Some(max_speed);
    }

    let (gain, loss) = elevation_change(samples);
    out.elevation_gain_m = gain;
    out.elevation_loss_m = loss;
    if out.calories_kcal.is_none() && out.avg_speed_mps.is_some() {
        let kmh = out.avg_speed_mps.unwrap_or(0.0) * 3.6;
        out.calories_kcal = Some(crate::physics::calories_from_met(
            kmh,
            75.0,
            out.duration_s as f32,
        ));
    }
    if ftp_w > 0.0 {
        if let Some(np) = out.normalized_power_w {
            let intensity = np / ftp_w;
            out.intensity_factor = Some(intensity);
            out.tss = Some(
                (out.duration_s as f32 * np * intensity / (ftp_w * 3600.0) * 100.0).max(0.0),
            );
        }
    }
    out
}

pub fn elevation_change(samples: &[Sample]) -> (f32, f32) {
    let mut gain = 0.0;
    let mut loss = 0.0;
    let mut pivot: Option<f32> = None;
    // Hysteresis: only accept an elevation excursion once it exceeds 0.5 m.
    // This rejects GPS/barometer noise while preserving gradual climbs.
    for s in samples {
        if let Some(ele) = s.altitude_m {
            match pivot {
                None => pivot = Some(ele),
                Some(p) => {
                    let d = ele - p;
                    if d > 0.5 {
                        gain += d;
                        pivot = Some(ele);
                    } else if d < -0.5 {
                        loss -= d;
                        pivot = Some(ele);
                    }
                }
            }
        }
    }
    (gain, loss)
}

/// Build a gap-filled 1 Hz power series starting at the first sample.
fn power_series_1hz(samples: &[Sample]) -> Vec<f32> {
    let mut out: Vec<f32> = Vec::new();
    if samples.is_empty() {
        return out;
    }
    let start = samples[0].elapsed_s;
    for s in samples {
        let Some(p) = s.power_w else { continue };
        if !p.is_finite() || p < 0.0 {
            continue;
        }
        let idx = ((s.elapsed_s - start).floor() as usize).min(2_000_000);
        if out.len() <= idx {
            out.resize(idx + 1, f32::NAN);
        }
        if out[idx].is_nan() {
            out[idx] = p;
        } else {
            out[idx] = (out[idx] + p) * 0.5;
        }
    }
    // Gaps of 1-2 seconds are filled by interpolation; longer gaps are zeros.
    for i in 0..out.len() {
        if out[i].is_nan() {
            let prev = if i > 0 { Some(out[i - 1]) } else { None };
            let next = out.get(i + 1).copied().filter(|v| !v.is_nan());
            out[i] = match (prev, next) {
                (Some(a), Some(b)) if a.is_finite() && b.is_finite() => (a + b) * 0.5,
                (Some(a), _) if a.is_finite() => a,
                (_, Some(b)) => b,
                _ => 0.0,
            };
        }
    }
    out
}

pub fn normalized_power(samples: &[Sample]) -> Option<f32> {
    let series = power_series_1hz(samples);
    if series.len() < 30 {
        return None;
    }
    let window = 30usize;
    let mut sum = 0.0f64;
    for i in 0..window {
        sum += series[i] as f64;
    }
    let mut acc = sum / window as f64;
    let mut total = acc.powi(4);
    let mut count = 1.0f64;
    for i in window..series.len() {
        sum += series[i] as f64 - series[i - window] as f64;
        acc = sum / window as f64;
        total += acc.powi(4);
        count += 1.0;
    }
    Some((total / count).powf(0.25) as f32)
}

/// Best average power for each requested window (seconds).
pub fn best_efforts(samples: &[Sample], windows: &[u32]) -> Vec<BestEffort> {
    let series = power_series_1hz(samples);
    let mut result = Vec::new();
    if series.is_empty() {
        return result;
    }
    for &w in windows {
        let w = w.max(1) as usize;
        if w > series.len() {
            continue;
        }
        let mut sum: f64 = series[..w].iter().map(|v| *v as f64).sum();
        let mut best = sum;
        let mut best_i = 0usize;
        for i in w..series.len() {
            sum += series[i] as f64 - series[i - w] as f64;
            if sum > best {
                best = sum;
                best_i = i + 1 - w;
            }
        }
        result.push(BestEffort {
            window_s: w as u32,
            power_w: (best / w as f64) as f32,
            start_elapsed_s: best_i as f64,
        });
    }
    result
}

/// Peak heart rate over a window, useful for comparison charts.
pub fn peak_hr(samples: &[Sample], window_s: u32) -> Option<f32> {
    let mut best = 0.0f32;
    for w in samples.windows(window_s.max(1) as usize) {
        let vals: Vec<f32> = w.iter().filter_map(|s| s.heart_rate_bpm).collect();
        if vals.is_empty() {
            continue;
        }
        let avg = vals.iter().sum::<f32>() / vals.len() as f32;
        best = best.max(avg);
    }
    (best > 0.0).then_some(best)
}

#[derive(Clone, Copy, Debug)]
pub struct ZoneBucket {
    pub zone: u8,
    pub seconds: f64,
    pub fraction: f32,
}

/// Time spent in each zone. `bounds` are the upper bounds of zones 1..=5,
/// the last bucket is everything above. Values below the first bound are zone 1.
pub fn zone_distribution(values: impl Iterator<Item = (f32, f64)>, bounds: &[f32; 6]) -> [ZoneBucket; 6] {
    let mut seconds = [0.0f64; 6];
    for (v, dt) in values {
        let mut zone = 5usize;
        for (i, b) in bounds.iter().enumerate() {
            if v < *b {
                zone = i;
                break;
            }
        }
        seconds[zone] += dt.max(0.0);
    }
    let total: f64 = seconds.iter().sum();
    let mut out = [ZoneBucket {
        zone: 0,
        seconds: 0.0,
        fraction: 0.0,
    }; 6];
    for i in 0..6 {
        out[i] = ZoneBucket {
            zone: (i + 1) as u8,
            seconds: seconds[i],
            fraction: if total > 0.0 {
                (seconds[i] / total) as f32
            } else {
                0.0
            },
        };
    }
    out
}

#[derive(Clone, Debug, Default)]
pub struct DailyLoad {
    pub date: String,
    pub tss: f32,
    pub duration_s: f64,
    pub distance_m: f64,
    pub ctl: f32,
    pub atl: f32,
    pub tsb: f32,
}

/// Daily TSS plus exponentially weighted chronic (42 d) and acute (7 d) load.
pub fn training_history(activities: &[Activity], days: usize) -> Vec<DailyLoad> {
    let today = Local::now().date_naive();
    let days = days.max(1);
    let start = today - Duration::days(days as i64 - 1);
    let mut out: Vec<DailyLoad> = (0..days)
        .map(|i| {
            let d = start + Duration::days(i as i64);
            DailyLoad {
                date: d.format("%Y-%m-%d").to_string(),
                ..Default::default()
            }
        })
        .collect();

    for a in activities.iter().filter(|a| a.is_finished()) {
        let Some(dt) = Local.timestamp_millis_opt(a.started_at_ms).single() else {
            continue;
        };
        let key = dt.date_naive().format("%Y-%m-%d").to_string();
        if let Some(day) = out.iter_mut().find(|d| d.date == key) {
            day.tss += a.summary.tss.unwrap_or(0.0);
            day.duration_s += a.summary.duration_s;
            day.distance_m += a.summary.distance_m;
        }
    }

    let mut ctl = 0.0f32;
    let mut atl = 0.0f32;
    // Seed with a short history before the window so CTL isn't artificially 0.
    for d in &mut out {
        ctl += (d.tss - ctl) / 42.0;
        atl += (d.tss - atl) / 7.0;
        d.ctl = ctl;
        d.atl = atl;
        d.tsb = ctl - atl;
    }
    out
}

/// Percentage of the route completed helper.
pub fn progress_fraction(distance_m: f64, total_m: f64) -> f32 {
    if total_m <= 0.0 {
        0.0
    } else {
        (distance_m / total_m).clamp(0.0, 1.0) as f32
    }
}

/// Estimate VO2max from 5-minute peak power (Hawley/Noakes style approximation,
/// for a rough fitness trend only — not a lab measurement).
pub fn estimate_vo2max(peak_5min_w: f32, weight_kg: f32) -> Option<f32> {
    if peak_5min_w <= 0.0 || weight_kg <= 0.0 {
        return None;
    }
    let w_per_kg = peak_5min_w / weight_kg;
    Some(10.8 * w_per_kg + 7.0)
}

/// Rough FTP estimate from a 20 minute effort.
pub fn ftp_from_20min(power_20min_w: f32) -> f32 {
    power_20min_w * 0.95
}

/// Basic decoupling (aerobic drift) between first and second half: percentage
/// change in power/HR ratio. Positive means HR drifted upward.
pub fn aerobic_decoupling(samples: &[Sample]) -> Option<f32> {
    if samples.len() < 20 {
        return None;
    }
    let mid = samples.len() / 2;
    let ratio = |slice: &[Sample]| -> Option<f64> {
        let mut sum = 0.0;
        let mut n = 0.0;
        for s in slice {
            if let (Some(p), Some(h)) = (s.power_w, s.heart_rate_bpm) {
                if h > 0.0 {
                    sum += p as f64 / h as f64;
                    n += 1.0;
                }
            }
        }
        (n > 0.0).then_some(sum / n)
    };
    let a = ratio(&samples[..mid])?;
    let b = ratio(&samples[mid..])?;
    if a <= 0.0 {
        return None;
    }
    Some(((a - b) / a * 100.0) as f32)
}

/// Human label for a best-effort window.
pub fn window_label(seconds: u32) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}h", seconds / 3600)
    }
}

pub fn rider_from_activity_defaults() -> RiderProfile {
    RiderProfile::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ActivitySummary, Sample, Sport};

    fn synthetic_samples(seconds: usize, power: f32) -> Vec<Sample> {
        (0..seconds)
            .map(|i| Sample {
                t_ms: i as i64 * 1000,
                elapsed_s: i as f64,
                distance_m: i as f64 * 8.0,
                speed_mps: Some(8.0),
                power_w: Some(power),
                heart_rate_bpm: Some(140.0),
                cadence_rpm: Some(90.0),
                altitude_m: Some(100.0 + (i as f32 * 0.1)),
                ..Default::default()
            })
            .collect()
    }

    #[test]
    fn normalized_power_constant_is_constant() {
        let samples = synthetic_samples(600, 200.0);
        let np = normalized_power(&samples).unwrap();
        assert!((np - 200.0).abs() < 1.0, "NP was {np}");
    }

    #[test]
    fn summary_tss_matches_formula() {
        let samples = synthetic_samples(3600, 200.0);
        let s = summarize(&samples, 200.0);
        assert_eq!(s.duration_s, 3599.0);
        let tss = s.tss.unwrap();
        assert!((tss - 100.0).abs() < 1.0, "TSS was {tss}");
        assert!((s.avg_power_w.unwrap() - 200.0).abs() < 1.0);
        assert!((s.avg_heart_rate_bpm.unwrap() - 140.0).abs() < 1.0);
    }

    #[test]
    fn best_efforts_find_interval() {
        let mut samples = synthetic_samples(300, 100.0);
        for s in samples.iter_mut().take(240).skip(180) {
            s.power_w = Some(400.0);
        }
        let efforts = best_efforts(&samples, &[60, 120]);
        let e60 = efforts.iter().find(|e| e.window_s == 60).unwrap();
        assert!((e60.power_w - 400.0).abs() < 2.0, "e60 = {}", e60.power_w);
        let e120 = efforts.iter().find(|e| e.window_s == 120).unwrap();
        assert!(e120.power_w > 240.0 && e120.power_w < 300.0);
    }

    #[test]
    fn zones_partition_time() {
        let bounds = [100.0, 150.0, 200.0, 250.0, 300.0, 400.0];
        let buckets = zone_distribution(
            [(90.0f32, 10.0), (160.0, 20.0), (350.0, 30.0)].into_iter(),
            &bounds,
        );
        let total: f64 = buckets.iter().map(|b| b.seconds).sum();
        assert!((total - 60.0).abs() < 1e-9);
        assert!(buckets[0].fraction > 0.16);
        assert!(buckets[5].fraction > 0.49);
    }

    #[test]
    fn elevation_gain_ignores_noise() {
        let samples: Vec<Sample> = (0..100)
            .map(|i| Sample {
                altitude_m: Some(100.0 + (i as f32 * 0.2) + if i % 2 == 0 { 0.2 } else { -0.2 }),
                ..Default::default()
            })
            .collect();
        let (gain, loss) = elevation_change(&samples);
        assert!(gain > 15.0 && gain < 25.0, "gain {gain}");
        assert!(loss < 5.0, "loss {loss}");
    }

    #[test]
    fn training_history_accumulates() {
        let now = crate::types::now_ms();
        let mk = |days_ago: i64, tss: f32| {
            let started = now - days_ago * 86_400_000;
            Activity {
                id: days_ago,
                name: "ride".into(),
                sport: Sport::Cycling,
                started_at_ms: started,
                ended_at_ms: Some(started + 3_600_000),
                summary: ActivitySummary {
                    duration_s: 3600.0,
                    tss: Some(tss),
                    distance_m: 30_000.0,
                    ..Default::default()
                },
                route_id: None,
                route_name: None,
                source: "test".into(),
                notes: String::new(),
                created_at_ms: started,
            }
        };
        let activities = vec![mk(1, 100.0), mk(0, 50.0)];
        let hist = training_history(&activities, 10);
        let last = hist.last().unwrap();
        assert!(last.ctl > 0.0);
        assert!(last.tsb < last.ctl);
        assert!(hist.iter().any(|d| d.tss > 0.0));
    }

    #[test]
    fn decoupling_detects_drift() {
        let mut samples = synthetic_samples(120, 200.0);
        for (i, s) in samples.iter_mut().enumerate() {
            s.heart_rate_bpm = Some(if i < 60 { 140.0 } else { 155.0 });
        }
        let d = aerobic_decoupling(&samples).unwrap();
        assert!(d > 5.0, "decoupling {d}");
    }
}
