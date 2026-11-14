//! Application state, chrome and cross-view actions.

use crate::theme::{self, Icon, Palette, ThemeMode};
use crate::views::Page;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, Frame, Id, Margin, Order, RichText, Sense, Stroke, Ui,
    Vec2, pos2, vec2,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use velocity_core::analytics;
use velocity_core::db::Store;
use velocity_core::route::{Route, demo_route};
use velocity_core::telemetry::{Metric, RideSession, SessionState};
use velocity_core::types::{
    Activity, BikeProfile, DataOrigin, Reading, RiderProfile, Sample, Sport, fmt_duration,
    local_date, local_time, now_ms,
};
use velocity_sensors::{
    BleHub, DiscoveredDevice, HubCommand, HubEvent, SensorKind, SensorMetric, SensorReading,
    Simulator, SimulatorConfig,
};

pub const RAIL_WIDTH: f32 = 238.0;

#[derive(Clone, Debug)]
pub struct AppOptions {
    pub db_path: PathBuf,
    pub export_dir: PathBuf,
    pub demo: bool,
    pub offline: bool,
    pub screenshot: Option<PathBuf>,
    pub screenshot_delay_ms: u64,
    pub world_quality: Option<crate::world::QualityPreset>,
    pub page: Page,
    pub theme: Option<ThemeMode>,
    pub width: f32,
    pub height: f32,
    pub zoom: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlMode {
    Erg,
    Grade,
    Resistance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub created_ms: i64,
}

pub struct DeviceState {
    pub device: DiscoveredDevice,
    pub connected: bool,
    pub control_ready: bool,
    pub readings: [Option<SensorReading>; 5],
    pub last_message: String,
}

impl DeviceState {
    fn new(device: DiscoveredDevice) -> Self {
        Self {
            device,
            connected: false,
            control_ready: false,
            readings: [None; 5],
            last_message: String::new(),
        }
    }

    pub fn reading(&self, metric: SensorMetric) -> Option<SensorReading> {
        self.readings[metric_index(metric)]
    }

    pub fn set_reading(&mut self, reading: SensorReading) {
        self.readings[metric_index(reading.metric)] = Some(reading);
    }

    pub fn name(&self) -> &str {
        &self.device.name
    }

    pub fn kind(&self) -> SensorKind {
        self.device.kind
    }
}

fn metric_index(metric: SensorMetric) -> usize {
    match metric {
        SensorMetric::Power => 0,
        SensorMetric::HeartRate => 1,
        SensorMetric::Cadence => 2,
        SensorMetric::Speed => 3,
        SensorMetric::Resistance => 4,
    }
}

#[derive(Clone, Debug)]
pub struct Recovery {
    pub activity: Activity,
    pub samples: Vec<Sample>,
}

struct ScreenshotPlan {
    path: PathBuf,
    frame: u32,
    requested: bool,
    done: Arc<AtomicBool>,
    delay_ms: u64,
    started: std::time::Instant,
}

pub struct VelocityApp {
    pub store: Store,
    pub palette: Palette,
    pub theme: ThemeMode,
    pub page: Page,
    pub rider: RiderProfile,
    pub bike: BikeProfile,
    pub db_path: PathBuf,
    pub export_dir: PathBuf,

    pub hub: Option<BleHub>,
    pub adapter_available: Option<bool>,
    pub adapter_message: String,
    pub scanning: bool,
    pub devices: BTreeMap<String, DeviceState>,
    pub selected_device: Option<String>,

    pub simulator: Simulator,
    pub sim_enabled: bool,
    pub sim_target_power: f32,
    pub sim_grade_pct: f32,
    pub sim_resistance_pct: f32,
    pub control_mode: ControlMode,
    pub route_driven: bool,
    pub last_sent_grade: Option<(f32, i64)>,
    pub trainer_status: String,
    pub trainer_error: Option<String>,

    pub session: RideSession,
    pub recording_id: Option<i64>,
    flushed_samples: usize,
    last_flush_ms: i64,
    pub last_saved_activity: Option<i64>,

    pub activities: Vec<Activity>,
    pub activity_count: usize,
    pub selected_activity: Option<Activity>,
    pub selected_samples: Vec<Sample>,
    pub activity_search: String,
    pub route_search: String,
    pub edit_name: String,
    pub edit_notes: String,

    pub routes: Vec<(i64, Route)>,
    pub selected_route: Option<i64>,
    pub current_route: Option<(i64, Route)>,
    pub activity_import_path: String,
    pub route_import_path: String,

    pub recovery: Option<Recovery>,
    pub pending_delete_activity: Option<i64>,
    pub pending_delete_route: Option<i64>,
    pub pending_discard: bool,
    pub(crate) last_control_ms: i64,
    pub show_about_licenses: bool,
    pub toasts: Vec<Toast>,

    pub frame_count: u64,
    screenshot: Option<ScreenshotPlan>,
    pub offline: bool,

    /// The 3D world renderer handle and its settings.
    pub world: crate::world::WorldHandle,
    pub world_route: Option<(i64, std::sync::Arc<Route>)>,
    pub world_prefs: crate::world::WorldPrefs,
}

impl VelocityApp {
    pub fn new(cc: &eframe::CreationContext<'_>, options: AppOptions) -> anyhow::Result<Self> {
        let ctx = &cc.egui_ctx;
        ctx.set_zoom_factor(options.zoom.clamp(0.6, 2.5));
        theme::install_fonts(ctx);

        let store = Store::open(&options.db_path)?;
        let world_cache = options
            .db_path
            .parent()
            .map(|dir| dir.join("dem-tiles"))
            .unwrap_or_else(|| PathBuf::from("dem-tiles"));
        let mut app = Self {
            store,
            palette: Palette::light(),
            theme: ThemeMode::Light,
            page: options.page,
            rider: RiderProfile::default(),
            bike: BikeProfile::default(),
            db_path: options.db_path.clone(),
            export_dir: options.export_dir.clone(),
            hub: None,
            adapter_available: None,
            adapter_message: "Bluetooth service starting…".to_string(),
            scanning: false,
            devices: BTreeMap::new(),
            selected_device: None,
            simulator: Simulator::new(SimulatorConfig::default()),
            sim_enabled: options.demo,
            sim_target_power: 190.0,
            sim_grade_pct: 0.0,
            sim_resistance_pct: 30.0,
            control_mode: ControlMode::Erg,
            route_driven: false,
            last_sent_grade: None,
            trainer_status: "No trainer selected".to_string(),
            trainer_error: None,
            session: RideSession::new(),
            recording_id: None,
            flushed_samples: 0,
            last_flush_ms: 0,
            last_saved_activity: None,
            activities: Vec::new(),
            activity_count: 0,
            selected_activity: None,
            selected_samples: Vec::new(),
            activity_search: String::new(),
            route_search: String::new(),
            edit_name: String::new(),
            edit_notes: String::new(),
            routes: Vec::new(),
            selected_route: None,
            current_route: None,
            activity_import_path: String::new(),
            route_import_path: String::new(),
            recovery: None,
            pending_delete_activity: None,
            pending_delete_route: None,
            pending_discard: false,
            last_control_ms: 0,
            show_about_licenses: false,
            toasts: Vec::new(),
            frame_count: 0,
            screenshot: None,
            offline: options.offline,
            world: crate::world::WorldHandle::new(world_cache),
            world_route: None,
            world_prefs: crate::world::WorldPrefs::default(),
        };

        app.load_preferences();
        if let Some(quality) = options.world_quality {
            app.world_prefs.quality = quality;
        }
        if options.demo {
            let _ = app.store.seed_demo_if_empty();
        }
        app.reload_activities();
        app.reload_routes();
        if app.routes.is_empty() {
            let mut route = demo_route();
            route.name = "Starnberger See · Demo Loop".to_string();
            let _ = app.store.save_route(&route);
            app.reload_routes();
        }
        if app.selected_route.is_none() {
            app.selected_route = app.routes.first().map(|(id, _)| *id);
        }
        app.selected_activity = app.activities.first().cloned();
        if let Some(activity) = app.selected_activity.clone() {
            app.load_activity_details(activity.id);
        }
        if let Some((id, route)) = app.routes.first().cloned() {
            app.current_route = Some((id, route));
        }

        app.simulator = Simulator::new(app.simulator_config());
        app.simulator.set_target_power(app.sim_target_power);

        if let Some(theme) = options.theme {
            app.theme = theme;
        }
        app.palette = Palette::of(app.theme);
        theme::install_style(ctx, app.theme);

        if let Ok(Some(recovered)) = app.store.unfinished_activity() {
            let samples = app.store.load_samples(recovered.id).unwrap_or_default();
            app.recovery = Some(Recovery {
                activity: recovered,
                samples,
            });
        }

        // Demo/screenshot state: a rich but clearly simulated ride in progress.
        if options.demo {
            app.seed_demo_ride();
        }

        if !options.offline {
            app.hub = Some(BleHub::spawn());
        } else {
            app.adapter_available = Some(false);
            app.adapter_message = "Bluetooth disabled for this run (--offline)".to_string();
        }

        if let Some(path) = options.screenshot {
            app.screenshot = Some(ScreenshotPlan {
                path,
                frame: 0,
                requested: false,
                done: Arc::new(AtomicBool::new(false)),
                delay_ms: options.screenshot_delay_ms,
                started: std::time::Instant::now(),
            });
        }
        Ok(app)
    }

    fn simulator_config(&self) -> SimulatorConfig {
        SimulatorConfig {
            ftp_w: self.rider.ftp_w,
            weight_kg: self.rider.weight_kg,
            max_hr_bpm: self.rider.max_hr_bpm,
            resting_hr_bpm: self.rider.resting_hr_bpm,
            bike_mass_kg: self.bike.mass_kg,
            cda_m2: self.bike.cda_m2,
            crr: self.bike.crr,
            ..SimulatorConfig::default()
        }
    }

    pub fn rebuild_simulator(&mut self) {
        let target = self.simulator.target_power();
        let grade = self.simulator.grade();
        self.simulator = Simulator::new(self.simulator_config());
        self.simulator.set_target_power(target);
        self.simulator.set_grade(grade);
    }

    pub fn load_preferences(&mut self) {
        if let Some(json) = self.store.get_setting("rider_profile").ok().flatten() {
            if let Ok(rider) = serde_json::from_str::<RiderProfile>(&json) {
                self.rider = rider;
            }
        }
        if let Some(json) = self.store.get_setting("bike_profile").ok().flatten() {
            if let Ok(bike) = serde_json::from_str::<BikeProfile>(&json) {
                self.bike = bike;
            }
        }
        if let Some(mode) = self.store.get_setting("theme").ok().flatten() {
            self.theme = ThemeMode::from_key(&mode);
        }
        if let Some(raw) = self.store.get_setting("world_prefs").ok().flatten() {
            let parts: Vec<&str> = raw.split('|').collect();
            if parts.len() == 6 {
                use crate::world::{CameraMode, QualityPreset};
                self.world_prefs.quality = QualityPreset::from_key(parts[0]).unwrap_or_default();
                self.world_prefs.camera.mode = CameraMode::from_key(parts[1]).unwrap_or_default();
                self.world_prefs.hour = parts[2].parse::<f32>().unwrap_or(9.4).clamp(0.0, 24.0);
                self.world_prefs.cloud =
                    parts[3].parse::<f32>().unwrap_or(0.22).clamp(0.0, 1.0);
                self.world_prefs.wet = parts[4].parse::<f32>().unwrap_or(0.0).clamp(0.0, 1.0);
                self.world_prefs.adaptive = parts[5] == "1";
            }
        }
        self.palette = Palette::of(self.theme);
    }

    pub fn save_preferences(&mut self) {
        if let Ok(json) = serde_json::to_string(&self.rider) {
            let _ = self.store.set_setting("rider_profile", &json);
        }
        if let Ok(json) = serde_json::to_string(&self.bike) {
            let _ = self.store.set_setting("bike_profile", &json);
        }
        let _ = self.store.set_setting("theme", self.theme.key());
        self.save_world_prefs();
    }

    /// Persist world/graphics settings separately: they change far more often
    /// than the rider profile, and this keeps writes off the drag path.
    pub fn save_world_prefs(&mut self) {
        let world = self.world_prefs;
        let raw = format!(
            "{}|{}|{:.2}|{:.2}|{:.2}|{}",
            world.quality.key(),
            world.camera.mode.key(),
            world.hour,
            world.cloud,
            world.wet,
            if world.adaptive { "1" } else { "0" }
        );
        let _ = self.store.set_setting("world_prefs", &raw);
    }

    pub fn toast(&mut self, text: impl Into<String>, kind: ToastKind) {
        self.toasts.push(Toast {
            text: text.into(),
            kind,
            created_ms: now_ms(),
        });
        if self.toasts.len() > 5 {
            self.toasts.remove(0);
        }
    }

    pub fn reload_activities(&mut self) {
        match self.store.list_activities(500, 0) {
            Ok(list) => {
                self.activities = list;
                self.activity_count = self.store.activity_count().unwrap_or(self.activities.len());
            }
            Err(err) => self.toast(format!("Could not read activities: {err}"), ToastKind::Error),
        }
    }

    pub fn reload_routes(&mut self) {
        match self.store.list_routes() {
            Ok(list) => self.routes = list,
            Err(err) => self.toast(format!("Could not read routes: {err}"), ToastKind::Error),
        }
    }

    pub fn load_activity_details(&mut self, id: i64) {
        match self.store.get_activity(id) {
            Ok(Some(activity)) => {
                self.selected_samples = self.store.load_samples(id).unwrap_or_default();
                self.edit_name = activity.name.clone();
                self.edit_notes = activity.notes.clone();
                self.selected_activity = Some(activity);
            }
            Ok(None) => {
                self.selected_activity = None;
                self.selected_samples.clear();
            }
            Err(err) => self.toast(format!("Could not load activity: {err}"), ToastKind::Error),
        }
    }

    pub fn open_activity(&mut self, id: i64) {
        self.page = Page::Activities;
        self.load_activity_details(id);
    }

    // -----------------------------------------------------------------------
    // Ride lifecycle
    // -----------------------------------------------------------------------

    pub fn has_active_trainer(&self) -> bool {
        self.devices
            .values()
            .any(|d| d.connected && d.kind() == SensorKind::SmartTrainer)
    }

    pub fn trainer_id(&self) -> Option<String> {
        self.devices
            .values()
            .find(|d| d.connected && d.kind() == SensorKind::SmartTrainer)
            .map(|d| d.device.id.clone())
    }

    pub fn connected_source_label(&self) -> String {
        let names: Vec<&str> = self
            .devices
            .values()
            .filter(|d| d.connected)
            .map(|d| d.name())
            .collect();
        match (names.is_empty(), self.sim_enabled) {
            (true, true) => "Simulated telemetry".to_string(),
            (true, false) => "No sensors connected".to_string(),
            (false, true) => format!("{} + simulated indoor trainer", names.join(" + ")),
            (false, false) => names.join(" + "),
        }
    }

    pub fn fresh_device_reading(&self, metric: SensorMetric, now_ms: i64) -> Option<SensorReading> {
        self.devices
            .values()
            .filter(|d| d.connected)
            .filter_map(|d| d.reading(metric))
            .filter(|r| now_ms - r.t_ms <= 5_000)
            .find(|r| r.value.is_finite())
    }

    pub fn start_ride(&mut self, now: i64) {
        if self.session.state.is_active() {
            return;
        }
        if !self.sim_enabled && !self.has_active_trainer() && !self.has_any_power_source() {
            self.toast(
                "Connect a trainer or enable the simulator before starting a ride.",
                ToastKind::Error,
            );
            self.page = Page::Devices;
            return;
        }
        self.session.start(now);
        self.session.route_id = self.current_route.as_ref().map(|(id, _)| *id);
        self.session.route_name = self.current_route.as_ref().map(|(_, r)| r.name.clone());
        let source = self.connected_source_label();
        let sport = if self.sim_enabled {
            Sport::VirtualRide
        } else {
            Sport::Cycling
        };
        match self.store.create_activity(
            &self.session.name,
            sport,
            now,
            &source,
            self.session.route_id,
            self.session.route_name.as_deref(),
        ) {
            Ok(id) => {
                self.recording_id = Some(id);
                self.flushed_samples = 0;
                self.last_flush_ms = now;
                self.last_saved_activity = None;
                if self.sim_enabled {
                    self.simulator.reset(now);
                    self.simulator.set_target_power(self.sim_target_power);
                }
                self.toast(
                    if self.sim_enabled {
                        "Ride started — simulated telemetry is clearly marked."
                    } else {
                        "Ride started. Recording locally."
                    },
                    ToastKind::Success,
                );
            }
            Err(err) => {
                self.session.state = SessionState::Idle;
                self.toast(format!("Could not start recording: {err}"), ToastKind::Error);
            }
        }
    }

    fn has_any_power_source(&self) -> bool {
        self.devices.values().any(|d| {
            d.connected
                && matches!(
                    d.kind(),
                    SensorKind::SmartTrainer | SensorKind::PowerMeter | SensorKind::SpeedCadence
                )
        })
    }

    pub fn pause_ride(&mut self, now: i64) {
        self.session.pause(now);
    }

    pub fn resume_ride(&mut self, now: i64) {
        self.session.resume(now);
    }

    pub fn stop_ride(&mut self, now: i64) {
        if !self.session.state.is_active() {
            return;
        }
        let samples = self.session.finish(now);
        let summary = analytics::summarize(&samples, self.rider.ftp_w);
        if let Some(id) = self.recording_id.take() {
            let name = self.session.name.clone();
            let notes = self.session
                .route_name
                .as_ref()
                .map(|r| format!("Recorded in VELOCITY on route: {r}"))
                .unwrap_or_else(|| "Recorded in VELOCITY.".to_string());
            let _ = self.store.append_samples(id, &samples);
            let result = self.store.finalize_activity(id, now, &summary, &name, &notes);
            match result {
                Ok(()) => {
                    self.last_saved_activity = Some(id);
                    self.toast(
                        format!(
                            "Saved “{}” · {} · TSS {}",
                            name,
                            velocity_core::types::fmt_distance_km(summary.distance_m),
                            summary.tss.map(|t| format!("{t:.0}")).unwrap_or_else(|| "—".into())
                        ),
                        ToastKind::Success,
                    );
                    self.reload_activities();
                    self.load_activity_details(id);
                }
                Err(err) => {
                    self.toast(format!("Could not save ride: {err}"), ToastKind::Error);
                }
            }
        }
        self.flushed_samples = 0;
        self.simulator.set_target_power(0.0);
        if let Some(trainer) = self.trainer_id() {
            self.hub_send(HubCommand::StopTrainer { id: trainer });
        }
    }

    pub fn discard_ride(&mut self, now: i64) {
        let _ = now;
        if let Some(id) = self.recording_id.take() {
            let _ = self.store.delete_activity(id);
        }
        self.session = RideSession::new();
        self.flushed_samples = 0;
        self.simulator.set_target_power(0.0);
        self.toast("Ride discarded. No activity was saved.", ToastKind::Info);
    }

    fn flush_recording(&mut self, now: i64) {
        let Some(id) = self.recording_id else { return };
        let total = self.session.samples.len();
        if total <= self.flushed_samples {
            return;
        }
        let due = now - self.last_flush_ms >= 4_000 || total - self.flushed_samples >= 20;
        if !due {
            return;
        }
        let start = self.flushed_samples;
        let pending: Vec<Sample> = self.session.samples[start..total].to_vec();
        if let Err(err) = self.store.append_samples(id, &pending) {
            self.toast(format!("Autosave failed: {err}"), ToastKind::Error);
            return;
        }
        self.flushed_samples = total;
        self.last_flush_ms = now;
    }

    // -----------------------------------------------------------------------
    // Demo helpers
    // -----------------------------------------------------------------------

    fn seed_demo_ride(&mut self) {
        let Some((activity, mut samples)) = velocity_core::demo::demo_activities().into_iter().next()
        else {
            return;
        };
        let now = now_ms();
        let duration = samples.last().map(|s| s.elapsed_s).unwrap_or(0.0);
        let shift = now - duration as i64 * 1000 - 2_000;
        for s in &mut samples {
            s.t_ms += shift;
        }
        self.session.restore(now - duration as i64 * 1000, duration, samples);
        self.session.resume(now);
        self.session.route_id = self.current_route.as_ref().map(|(id, _)| *id);
        self.session.route_name = self.current_route.as_ref().map(|(_, r)| r.name.clone());
        self.session.name = activity.name.clone();
        self.sim_enabled = true;
        self.sim_target_power = activity.summary.avg_power_w.unwrap_or(190.0);
        self.simulator.set_target_power(self.sim_target_power);
        self.session.live.set_gps(47.9985, 11.3420, now, DataOrigin::Simulated);
        if let Some(last) = self.session.samples.last().cloned() {
            if let Some(v) = last.power_w {
                self.session
                    .live
                    .set(Metric::Power, Reading::new(v, now, true, DataOrigin::Simulated));
            }
            if let Some(v) = last.heart_rate_bpm {
                self.session.live.set(
                    Metric::HeartRate,
                    Reading::new(v, now, true, DataOrigin::Simulated),
                );
            }
            if let Some(v) = last.cadence_rpm {
                self.session
                    .live
                    .set(Metric::Cadence, Reading::new(v, now, true, DataOrigin::Simulated));
            }
            if let Some(v) = last.speed_mps {
                self.session
                    .live
                    .set(Metric::Speed, Reading::new(v, now, true, DataOrigin::Simulated));
            }
        }
    }

    // -----------------------------------------------------------------------
    // BLE + simulator
    // -----------------------------------------------------------------------

    pub fn hub_send(&mut self, command: HubCommand) {
        if let Some(hub) = &self.hub {
            let _ = hub.send(command);
        }
    }

    pub fn connect_device(&mut self, id: &str) {
        let kind = self
            .devices
            .get(id)
            .map(|d| d.kind())
            .unwrap_or(SensorKind::Unknown);
        self.hub_send(HubCommand::Connect {
            id: id.to_string(),
            kind,
        });
        self.toast("Connecting…", ToastKind::Info);
    }

    pub fn disconnect_device(&mut self, id: &str) {
        self.hub_send(HubCommand::Disconnect {
            id: id.to_string(),
        });
    }

    pub fn start_scan(&mut self) {
        self.scanning = true;
        self.hub_send(HubCommand::StartScan);
    }

    pub fn stop_scan(&mut self) {
        self.scanning = false;
        self.hub_send(HubCommand::StopScan);
    }

    fn handle_hub_event(&mut self, event: HubEvent, now: i64) {
        match event {
            HubEvent::Adapter {
                available,
                message,
            } => {
                self.adapter_available = Some(available);
                self.adapter_message = message;
            }
            HubEvent::ScanState { scanning } => {
                self.scanning = scanning;
            }
            HubEvent::Device(device) => {
                let id = device.id.clone();
                self.devices
                    .entry(id)
                    .and_modify(|state| {
                        state.device = device.clone();
                    })
                    .or_insert_with(|| DeviceState::new(device));
            }
            HubEvent::Connected {
                id,
                name,
                kind,
                services,
            } => {
                let device = DiscoveredDevice {
                    id: id.clone(),
                    name: name.clone(),
                    kind,
                    rssi: None,
                    services,
                    connected: true,
                };
                let state = self
                    .devices
                    .entry(id.clone())
                    .or_insert_with(|| DeviceState::new(device.clone()));
                state.device = device;
                state.connected = true;
                state.last_message = "Connected".to_string();
                self.selected_device = Some(id.clone());
                self.trainer_status = format!("{name} connected");
                self.toast(format!("{name} connected"), ToastKind::Success);
                if kind == SensorKind::SmartTrainer {
                    self.sim_enabled = false;
                }
            }
            HubEvent::Disconnected { id, reason } => {
                if let Some(state) = self.devices.get_mut(&id) {
                    state.connected = false;
                    state.control_ready = false;
                    state.last_message = reason.clone().unwrap_or_else(|| "Disconnected".into());
                }
                if let Some(reason) = reason {
                    self.toast(reason, ToastKind::Info);
                }
            }
            HubEvent::Reading { id, reading, .. } => {
                if let Some(state) = self.devices.get_mut(&id) {
                    state.set_reading(reading);
                }
                self.apply_sensor_reading(reading, now);
            }
            HubEvent::Control {
                id,
                opcode,
                result,
                detail,
            } => {
                if let Some(state) = self.devices.get_mut(&id) {
                    state.last_message = detail.clone();
                    if opcode == 0x00 && result == 0x01 {
                        state.control_ready = true;
                    }
                }
                self.trainer_status = detail.clone();
                if result != 0x01 && result != 0x00 {
                    self.trainer_error = Some(detail);
                }
            }
            HubEvent::Status { id, message } => {
                if let Some(state) = self.devices.get_mut(&id) {
                    state.last_message = message.clone();
                }
                self.trainer_status = message.clone();
                self.toast(message, ToastKind::Info);
            }
            HubEvent::Error { id, message } => {
                if let Some(id) = &id {
                    if let Some(state) = self.devices.get_mut(id) {
                        state.last_message = message.clone();
                    }
                }
                self.trainer_error = Some(message.clone());
                self.toast(message, ToastKind::Error);
            }
        }
    }

    fn apply_sensor_reading(&mut self, reading: SensorReading, now: i64) {
        let value = reading.value;
        let origin = if reading.origin == DataOrigin::Unknown {
            DataOrigin::Bluetooth
        } else {
            reading.origin
        };
        let mapped = match reading.metric {
            SensorMetric::Power => Some(Metric::Power),
            SensorMetric::HeartRate => Some(Metric::HeartRate),
            SensorMetric::Cadence => Some(Metric::Cadence),
            SensorMetric::Speed => Some(Metric::Speed),
            SensorMetric::Resistance => None,
        };
        if let Some(metric) = mapped {
            let simulated = origin == DataOrigin::Simulated;
            self.session
                .live
                .set(metric, Reading::new(value, now, simulated, origin));
        }
    }

    fn poll_sensors(&mut self, now: i64) {
        if let Some(hub) = &mut self.hub {
            let events = hub.poll_events();
            for event in events {
                self.handle_hub_event(event, now);
            }
        }
    }

    fn tick_simulator(&mut self, now: i64) {
        if !self.sim_enabled {
            return;
        }
        // Route-driven grade changes the target before ticking.
        if self.route_driven {
            if let Some((_, route)) = &self.current_route {
                let progress = (self.session.live.distance_m / route.stats.distance_m.max(1.0))
                    .clamp(0.0, 1.0);
                let grade = route.grade_at(progress * route.stats.distance_m);
                self.sim_grade_pct = grade;
                self.simulator.set_grade(grade);
            }
        }
        match self.control_mode {
            ControlMode::Erg => self.simulator.set_target_power(self.sim_target_power),
            ControlMode::Grade => {
                self.simulator.set_target_power(190.0);
                self.simulator.set_grade(self.sim_grade_pct);
            }
            ControlMode::Resistance => {
                self.simulator.set_target_power(90.0 + self.sim_resistance_pct * 2.6);
            }
        }
        let readings = self.simulator.tick(now);
        let has_real_hr = self.fresh_device_reading(SensorMetric::HeartRate, now).is_some();
        let has_real_trainer = self.has_active_trainer();

        if !has_real_trainer {
            self.session.live.set(Metric::Power, readings.power(now));
            self.session.live.set(Metric::Speed, readings.speed(now));
            self.session
                .live
                .set(Metric::Cadence, readings.cadence(now));
            self.session.live.set(Metric::Grade, readings.grade(now));
        }
        if !has_real_hr {
            self.session
                .live
                .set(Metric::HeartRate, readings.heart_rate(now));
        }
        if self.route_driven {
            if let Some((_, route)) = &self.current_route {
                let fraction = (self.session.live.distance_m / route.stats.distance_m.max(1.0))
                    .clamp(0.0, 1.0);
                let p = route.position_at(fraction * route.stats.distance_m);
                self.session
                    .live
                    .set_gps(p.lat, p.lon, now, DataOrigin::Simulated);
                self.session.live.set(
                    Metric::Altitude,
                    Reading::new(p.elevation_m, now, true, DataOrigin::Simulated),
                );
                self.session
                    .live
                    .set(Metric::Grade, Reading::new(p.grade_pct, now, true, DataOrigin::Simulated));
            }
        }
    }

    pub(crate) fn send_trainer_control(&mut self, now: i64) {
        let Some(id) = self.trainer_id() else { return };
        if !self.has_active_trainer() {
            return;
        }
        match self.control_mode {
            ControlMode::Erg => {
                if now - self.last_control_ms < 2_000 {
                    return;
                }
                self.last_control_ms = now;
                self.hub_send(HubCommand::SetTargetPower {
                    id: id.clone(),
                    watts: self.sim_target_power,
                });
            }
            ControlMode::Resistance => {
                if now - self.last_control_ms < 2_000 {
                    return;
                }
                self.last_control_ms = now;
                self.hub_send(HubCommand::SetTargetResistance {
                    id: id.clone(),
                    percent: self.sim_resistance_pct,
                });
            }
            ControlMode::Grade => {
                let grade = self.sim_grade_pct;
                let should_send = match self.last_sent_grade {
                    None => true,
                    Some((last, t)) => (last - grade).abs() > 0.35 || now - t > 5_000,
                };
                if should_send {
                    self.last_sent_grade = Some((grade, now));
                    self.hub_send(HubCommand::SetSimulation {
                        id: id.clone(),
                        grade_pct: grade,
                        wind_mps: 0.0,
                        crr: self.bike.crr,
                        cw_kg_per_m: 0.51 * self.bike.cda_m2,
                    });
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Actions used by views
    // -----------------------------------------------------------------------

    pub fn set_theme(&mut self, ctx: &egui::Context, mode: ThemeMode) {
        self.theme = mode;
        self.palette = Palette::of(mode);
        theme::install_style(ctx, mode);
        let _ = self.store.set_setting("theme", mode.key());
    }

    pub fn select_route(&mut self, id: i64) {
        self.selected_route = Some(id);
    }

    pub fn use_route(&mut self, id: i64, and_start: bool) {
        if let Some((_, route)) = self.routes.iter().find(|(rid, _)| *rid == id).cloned() {
            self.current_route = Some((id, route));
            self.selected_route = Some(id);
            if and_start {
                self.page = Page::Ride;
                let now = now_ms();
                self.start_ride(now);
            } else {
                self.toast("Route selected for the next ride.", ToastKind::Success);
            }
        }
    }

    pub fn import_activity(&mut self, path: &str) {
        let path = path.trim();
        if path.is_empty() {
            self.toast("Enter a path to a GPX, TCX or FIT file.", ToastKind::Error);
            return;
        }
        let parsed = match velocity_core::formats::import_activity_file(path) {
            Ok(p) => p,
            Err(velocity_core::formats::ImportError::NoTimestamps) => {
                self.toast(
                    "This GPX has no timestamps. Import it from Routes instead.",
                    ToastKind::Error,
                );
                return;
            }
            Err(err) => {
                self.toast(format!("Import failed: {err}"), ToastKind::Error);
                return;
            }
        };
        let started = parsed
            .samples
            .first()
            .map(|s| s.t_ms)
            .unwrap_or_else(now_ms);
        let summary = analytics::summarize(&parsed.samples, self.rider.ftp_w);
        match self.store.create_activity(
            &parsed.name,
            parsed.sport,
            started,
            &parsed.source,
            None,
            None,
        ) {
            Ok(id) => {
                let _ = self.store.append_samples(id, &parsed.samples);
                let notes = if parsed.warnings.is_empty() {
                    format!("Imported from {path}")
                } else {
                    format!("Imported from {path}. Notes: {}", parsed.warnings.join("; "))
                };
                let _ = self.store.finalize_activity(
                    id,
                    parsed.samples.last().map(|s| s.t_ms).unwrap_or(started),
                    &summary,
                    &parsed.name,
                    &notes,
                );
                self.reload_activities();
                self.load_activity_details(id);
                self.page = Page::Activities;
                self.activity_import_path.clear();
                self.toast(
                    format!("Imported “{}” · {} samples", parsed.name, parsed.samples.len()),
                    ToastKind::Success,
                );
            }
            Err(err) => self.toast(format!("Could not save import: {err}"), ToastKind::Error),
        }
    }

    pub fn import_route(&mut self, path: &str) {
        let path = path.trim();
        if path.is_empty() {
            self.toast("Enter a path to a GPX route file.", ToastKind::Error);
            return;
        }
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(err) => {
                self.toast(format!("Could not read route: {err}"), ToastKind::Error);
                return;
            }
        };
        let fallback = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Imported route");
        match Route::from_gpx(&text, fallback) {
            Ok(route) => match self.store.save_route(&route) {
                Ok(id) => {
                    self.reload_routes();
                    self.selected_route = Some(id);
                    self.current_route = Some((id, route.clone()));
                    self.route_import_path.clear();
                    self.toast(
                        format!(
                            "Imported route “{}” · {}",
                            route.name,
                            velocity_core::types::fmt_distance_km(route.stats.distance_m)
                        ),
                        ToastKind::Success,
                    );
                }
                Err(err) => self.toast(format!("Could not save route: {err}"), ToastKind::Error),
            },
            Err(err) => self.toast(format!("GPX route import failed: {err}"), ToastKind::Error),
        }
    }

    pub fn export_activity(&mut self, id: i64, format: velocity_core::formats::ExportFormat) {
        let Some(activity) = self.activities.iter().find(|a| a.id == id).cloned() else {
            return;
        };
        let samples = self.store.load_samples(id).unwrap_or_default();
        match velocity_core::formats::export_activity(&activity, &samples, format) {
            Ok(bytes) => {
                let safe: String = activity
                    .name
                    .chars()
                    .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
                    .collect();
                let file = self
                    .export_dir
                    .join(format!("{}-{}.{}", safe, local_date(activity.started_at_ms), format.extension()));
                if let Some(parent) = file.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match std::fs::write(&file, bytes) {
                    Ok(()) => self.toast(
                        format!("Exported to {}", file.display()),
                        ToastKind::Success,
                    ),
                    Err(err) => self.toast(format!("Export failed: {err}"), ToastKind::Error),
                }
            }
            Err(err) => self.toast(format!("Export failed: {err}"), ToastKind::Error),
        }
    }

    pub fn export_route(&mut self, id: i64) {
        let Some((_, route)) = self.routes.iter().find(|(rid, _)| *rid == id).cloned() else {
            return;
        };
        let safe: String = route
            .name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
            .collect();
        let file = self.export_dir.join(format!("{safe}.gpx"));
        if let Some(parent) = file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&file, route.to_gpx()) {
            Ok(()) => self.toast(format!("Route exported to {}", file.display()), ToastKind::Success),
            Err(err) => self.toast(format!("Export failed: {err}"), ToastKind::Error),
        }
    }

    pub fn delete_activity(&mut self, id: i64) {
        match self.store.delete_activity(id) {
            Ok(()) => {
                self.reload_activities();
                if self.selected_activity.as_ref().map(|a| a.id) == Some(id) {
                    self.selected_activity = None;
                    self.selected_samples.clear();
                }
                self.toast("Activity deleted", ToastKind::Info);
            }
            Err(err) => self.toast(format!("Delete failed: {err}"), ToastKind::Error),
        }
    }

    pub fn delete_route(&mut self, id: i64) {
        match self.store.delete_route(id) {
            Ok(()) => {
                self.reload_routes();
                if self.selected_route == Some(id) {
                    self.selected_route = self.routes.first().map(|(id, _)| *id);
                }
                if self.current_route.as_ref().map(|(rid, _)| *rid) == Some(id) {
                    self.current_route = None;
                }
                self.toast("Route deleted", ToastKind::Info);
            }
            Err(err) => self.toast(format!("Delete failed: {err}"), ToastKind::Error),
        }
    }

    pub fn save_activity_meta(&mut self, id: i64) {
        let name = self.edit_name.trim().to_string();
        let notes = self.edit_notes.trim().to_string();
        if name.is_empty() {
            self.toast("Activity name cannot be empty.", ToastKind::Error);
            return;
        }
        let sport = self
            .selected_activity
            .as_ref()
            .map(|a| a.sport)
            .unwrap_or(Sport::Cycling);
        match self.store.update_activity_meta(id, &name, &notes, sport) {
            Ok(()) => {
                self.reload_activities();
                self.load_activity_details(id);
                self.toast("Activity updated", ToastKind::Success);
            }
            Err(err) => self.toast(format!("Could not update activity: {err}"), ToastKind::Error),
        }
    }

    pub fn apply_profile(&mut self) {
        self.save_preferences();
        self.rebuild_simulator();
        self.toast("Profile and bike settings saved.", ToastKind::Success);
    }

    pub fn backup_database(&mut self) {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let file = self.export_dir.join(format!("velocity-backup-{stamp}.db"));
        match self.store.backup_to(&file) {
            Ok(()) => self.toast(
                format!("Database backed up to {}", file.display()),
                ToastKind::Success,
            ),
            Err(err) => self.toast(format!("Backup failed: {err}"), ToastKind::Error),
        }
    }

    pub fn seed_demo_data(&mut self) {
        match self.store.seed_demo_if_empty() {
            Ok(true) => {
                self.reload_activities();
                self.toast("Demo activities added.", ToastKind::Success);
            }
            Ok(false) => self.toast(
                "Demo data is only added when the activity list is empty.",
                ToastKind::Info,
            ),
            Err(err) => self.toast(format!("Could not add demo data: {err}"), ToastKind::Error),
        }
    }

    pub fn recovery_resume(&mut self, now: i64) {
        if let Some(recovery) = self.recovery.take() {
            let elapsed = recovery
                .samples
                .last()
                .map(|s| s.elapsed_s)
                .unwrap_or(0.0);
            self.session.restore(
                recovery.activity.started_at_ms,
                elapsed,
                recovery.samples,
            );
            self.recording_id = Some(recovery.activity.id);
            self.flushed_samples = self
                .store
                .load_samples(recovery.activity.id)
                .map(|s| s.len())
                .unwrap_or(0);
            self.session.resume(now);
            self.session.name = recovery.activity.name;
            self.session.route_id = recovery.activity.route_id;
            self.session.route_name = recovery.activity.route_name;
            self.page = Page::Ride;
            self.toast("Interrupted ride restored and paused data resumed.", ToastKind::Success);
        }
    }

    pub fn recovery_finalize(&mut self, now: i64) {
        if let Some(recovery) = self.recovery.take() {
            let samples = if recovery.samples.is_empty() {
                self.store.load_samples(recovery.activity.id).unwrap_or_default()
            } else {
                recovery.samples
            };
            let summary = analytics::summarize(&samples, self.rider.ftp_w);
            match self.store.finalize_activity(
                recovery.activity.id,
                now,
                &summary,
                &recovery.activity.name,
                "Recovered after an unexpected shutdown.",
            ) {
                Ok(()) => {
                    self.reload_activities();
                    self.load_activity_details(recovery.activity.id);
                    self.page = Page::Activities;
                    self.toast("Interrupted ride finalized and saved.", ToastKind::Success);
                }
                Err(err) => self.toast(format!("Could not finalize ride: {err}"), ToastKind::Error),
            }
        }
    }

    pub fn recovery_discard(&mut self) {
        if let Some(recovery) = self.recovery.take() {
            let _ = self.store.delete_activity(recovery.activity.id);
            self.toast("Interrupted ride discarded.", ToastKind::Info);
        }
    }

    // -----------------------------------------------------------------------
    // Wiring for views
    // -----------------------------------------------------------------------

    pub fn week_totals(&self) -> (f64, f64, f32, f32) {
        let cutoff = now_ms() - 7 * 86_400_000;
        let mut distance = 0.0;
        let mut seconds = 0.0;
        let mut tss = 0.0;
        let mut gain = 0.0;
        for a in self.activities.iter().filter(|a| a.started_at_ms >= cutoff) {
            distance += a.summary.distance_m;
            seconds += a.summary.moving_s;
            tss += a.summary.tss.unwrap_or(0.0);
            gain += a.summary.elevation_gain_m;
        }
        (distance, seconds, tss, gain)
    }

    pub fn training_history(&self) -> Vec<analytics::DailyLoad> {
        analytics::training_history(&self.activities, 42)
    }

    pub fn current_route_progress(&self) -> Option<f64> {
        let (_, route) = self.current_route.as_ref()?;
        Some((self.session.live.distance_m / route.stats.distance_m.max(1.0)).clamp(0.0, 1.0))
    }

    pub fn selected_route_data(&self) -> Option<(i64, &Route)> {
        let id = self.selected_route?;
        self.routes
            .iter()
            .find(|(rid, _)| *rid == id)
            .map(|(rid, route)| (*rid, route))
    }

    pub fn connected_count(&self) -> usize {
        self.devices.values().filter(|d| d.connected).count()
    }
}

// ---------------------------------------------------------------------------
// eframe integration
// ---------------------------------------------------------------------------

impl eframe::App for VelocityApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.frame_count += 1;
        let ctx = ui.ctx().clone();
        let now = now_ms();

        self.poll_sensors(now);
        self.tick_simulator(now);
        if self.session.state == SessionState::Running {
            self.session.update(now);
            self.flush_recording(now);
            self.send_trainer_control(now);
        } else if self.session.state == SessionState::Paused {
            self.session.update(now);
        }

        let palette = self.palette;
        let mut action: Option<UiAction> = None;

        // Left rail ---------------------------------------------------------
        egui::Panel::left("rail")
            .resizable(false)
            .exact_size(RAIL_WIDTH)
            .frame(
                Frame::NONE
                    .fill(palette.rail)
                    .inner_margin(Margin::symmetric(16, 18))
                    .stroke(Stroke::new(1.0, palette.line)),
            )
            .show(ui, |ui| {
                action = self.rail(ui);
            });

        // Top bar -----------------------------------------------------------
        egui::Panel::top("topbar")
            .exact_size(66.0)
            .frame(
                Frame::NONE
                    .fill(palette.bg)
                    .inner_margin(Margin::symmetric(28, 12)),
            )
            .show(ui, |ui| {
                let top_action = self.top_bar(ui);
                if top_action.is_some() {
                    action = top_action;
                }
            });

        // Page ---------------------------------------------------------------
        egui::CentralPanel::default()
            .frame(
                Frame::NONE
                    .fill(palette.bg)
                    .inner_margin(Margin {
                        left: 28,
                        right: 28,
                        top: 6,
                        bottom: 24,
                    }),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match self.page {
                            Page::Dashboard => self.dashboard(ui),
                            Page::Ride => self.ride(ui),
                            Page::Activities => self.activities(ui),
                            Page::Routes => self.routes(ui),
                            Page::Devices => self.devices(ui),
                            Page::Settings => self.settings(ui),
                        }
                        ui.add_space(28.0);
                    });
            });

        self.recovery_dialog(&ctx);
        self.delete_dialogs(&ctx);
        self.toasts(&ctx);
        self.handle_action(action, &ctx, now);
        self.screenshot_tick(&ctx);

        let interval = if self.session.state == SessionState::Running
            || self.scanning
            || self.sim_enabled
            || self.screenshot.is_some()
        {
            33
        } else {
            250
        };
        ctx.request_repaint_after(Duration::from_millis(interval));
    }
}

#[derive(Clone, Debug)]
enum UiAction {
    Navigate(Page),
    ToggleTheme,
    StartRide,
    OpenDevices,
    OpenRoute(i64),
    OpenActivity(i64),
}

impl VelocityApp {
    fn rail(&mut self, ui: &mut Ui) -> Option<UiAction> {
        let palette = self.palette;
        let mut action = None;
        ui.horizontal(|ui| {
            theme::logo(ui, 34.0);
            ui.add_space(7.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("VELOCITY")
                        .font(theme::font_display(19.0))
                        .color(palette.ink),
                );
                ui.label(
                    RichText::new("local cycling os")
                        .font(theme::font_semi(9.0))
                        .color(palette.faint),
                );
            });
        });
        ui.add_space(24.0);
        ui.label(
            RichText::new("TRAIN")
                .font(theme::font_semi(9.5))
                .color(palette.faint),
        );
        ui.add_space(4.0);
        for page in [Page::Dashboard, Page::Ride, Page::Activities, Page::Routes] {
            if theme::nav_item(ui, page.icon(), page.label(), self.page == page, &palette).clicked() {
                action = Some(UiAction::Navigate(page));
            }
            ui.add_space(2.0);
        }
        ui.add_space(18.0);
        ui.label(
            RichText::new("SYSTEM")
                .font(theme::font_semi(9.5))
                .color(palette.faint),
        );
        ui.add_space(4.0);
        for page in [Page::Devices, Page::Settings] {
            if theme::nav_item(ui, page.icon(), page.label(), self.page == page, &palette).clicked() {
                action = Some(UiAction::Navigate(page));
            }
            ui.add_space(2.0);
        }

        // Bottom status card.
        let bottom_height = 132.0;
        let y = ui.max_rect().bottom() - bottom_height;
        if y > ui.cursor().top() {
            ui.add_space((y - ui.cursor().top()).max(0.0));
        }
        let card = theme::soft_card(&palette);
        card.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let color = match self.adapter_available {
                    Some(true) => palette.positive,
                    Some(false) => palette.danger,
                    None => palette.warning,
                };
                theme::dot(ui, color, 7.0);
                ui.label(
                    RichText::new(if self.connected_count() > 0 {
                        format!("{} device(s) connected", self.connected_count())
                    } else {
                        "Bluetooth".to_string()
                    })
                    .font(theme::font_semi(11.5))
                    .color(palette.ink),
                );
            });
            ui.add_space(3.0);
            let message = if self.connected_count() > 0 {
                self.devices
                    .values()
                    .find(|d| d.connected)
                    .map(|d| d.name().to_string())
                    .unwrap_or_else(|| self.adapter_message.clone())
            } else {
                self.adapter_message.clone()
            };
            ui.label(
                RichText::new(truncate(&message, 34))
                    .font(theme::font_body(10.5))
                    .color(palette.muted),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if self.sim_enabled {
                    let _ = theme::pill(
                        ui,
                        "SIMULATOR ON",
                        palette.accent_ink,
                        palette.accent,
                    );
                } else {
                    let _ = theme::pill(
                        ui,
                        "SENSOR MODE",
                        palette.muted,
                        palette.surface_sunken,
                    );
                }
            });
            ui.add_space(5.0);
            ui.label(
                RichText::new(format!("VELOCITY {}", env!("CARGO_PKG_VERSION")))
                    .font(theme::font_mono(9.0))
                    .color(palette.faint),
            );
        });
        action
    }

    fn top_bar(&mut self, ui: &mut Ui) -> Option<UiAction> {
        let palette = self.palette;
        let mut action = None;
        ui.horizontal(|ui| {
            let crumb = match self.page {
                Page::Dashboard => "TRAIN  /  OVERVIEW",
                Page::Ride => "TRAIN  /  LIVE RIDE",
                Page::Activities => "TRAIN  /  ACTIVITIES",
                Page::Routes => "TRAIN  /  ROUTES",
                Page::Devices => "SYSTEM  /  DEVICES",
                Page::Settings => "SYSTEM  /  SETTINGS",
            };
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(crumb)
                        .font(theme::font_semi(9.5))
                        .color(palette.faint),
                );
                ui.label(
                    RichText::new(self.page.label())
                        .font(theme::font_semi(15.0))
                        .color(palette.ink),
                );
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if theme::icon_button(
                    ui,
                    if self.theme.is_dark() {
                        Icon::Sun
                    } else {
                        Icon::Moon
                    },
                    &palette,
                    36.0,
                    false,
                )
                .clicked()
                {
                    action = Some(UiAction::ToggleTheme);
                }
                ui.add_space(4.0);
                let status = if self.session.state.is_active() {
                    format!("RIDE {}", fmt_duration(self.session.elapsed_s(now_ms())))
                } else if self.connected_count() > 0 {
                    format!("{} CONNECTED", self.connected_count())
                } else {
                    "NO SENSORS".to_string()
                };
                if self.session.state.is_active() {
                    let _ = theme::pill(ui, &status, palette.accent_ink, palette.accent);
                } else {
                    let dark = self.connected_count() == 0;
                    let _ = theme::pill(
                        ui,
                        &status,
                        if dark { palette.muted } else { palette.positive },
                        if dark { palette.surface_sunken } else { palette.positive_soft },
                    );
                }
                ui.add_space(10.0);
                ui.label(
                    RichText::new(local_time(now_ms()))
                        .font(theme::font_mono(11.5))
                        .color(palette.muted),
                );
                ui.add_space(12.0);
                if self.session.state.is_active() {
                    if theme::accent_button(ui, Icon::Ride, "Back to ride", &palette).clicked() {
                        action = Some(UiAction::Navigate(Page::Ride));
                    }
                } else if theme::accent_button(ui, Icon::Play, "Start ride", &palette).clicked() {
                    action = Some(UiAction::StartRide);
                }
            });
        });
        action
    }

    fn handle_action(&mut self, action: Option<UiAction>, ctx: &egui::Context, now: i64) {
        match action {
            Some(UiAction::Navigate(page)) => self.page = page,
            Some(UiAction::ToggleTheme) => {
                let mode = self.theme.toggled();
                self.set_theme(ctx, mode);
            }
            Some(UiAction::StartRide) => {
                self.page = Page::Ride;
                self.start_ride(now);
            }
            Some(UiAction::OpenDevices) => self.page = Page::Devices,
            Some(UiAction::OpenRoute(id)) => {
                self.selected_route = Some(id);
                self.page = Page::Routes;
            }
            Some(UiAction::OpenActivity(id)) => self.open_activity(id),
            None => {}
        }
    }

    fn recovery_dialog(&mut self, ctx: &egui::Context) {
        let Some(recovery) = self.recovery.clone() else {
            return;
        };
        let palette = self.palette;
        let now = now_ms();
        let mut resume = false;
        let mut finalize = false;
        let mut discard = false;
        egui::Window::new("recover_ride")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .frame(
                theme::card(&palette)
                    .inner_margin(Margin::symmetric(26, 24))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 10],
                        blur: 40,
                        spread: 2,
                        color: palette.shadow,
                    }),
            )
            .show(ctx, |ui| {
                ui.set_width(430.0);
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
                    ui.painter()
                        .rect_filled(rect, CornerRadius::same(12), palette.warning_soft);
                    theme::paint_icon(ui, rect.shrink(10.0), Icon::Refresh, palette.warning);
                    ui.add_space(10.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("Unfinished ride found")
                                .font(theme::font_display(20.0))
                                .color(palette.ink),
                        );
                        ui.label(
                            RichText::new("VELOCITY closed while recording. Nothing was lost.")
                                .font(theme::font_body(12.5))
                                .color(palette.muted),
                        );
                    });
                });
                ui.add_space(16.0);
                let card = theme::soft_card(&palette);
                card.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    theme::stat_line(ui, &palette, "Activity", &recovery.activity.name);
                    theme::stat_line(
                        ui,
                        &palette,
                        "Started",
                        &velocity_core::types::local_datetime(recovery.activity.started_at_ms),
                    );
                    theme::stat_line(
                        ui,
                        &palette,
                        "Recovered samples",
                        &format!("{}", recovery.samples.len()),
                    );
                    theme::stat_line(
                        ui,
                        &palette,
                        "Distance so far",
                        &velocity_core::types::fmt_distance_km(
                            recovery
                                .samples
                                .last()
                                .map(|s| s.distance_m)
                                .unwrap_or(0.0),
                        ),
                    );
                });
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if theme::accent_button(ui, Icon::Play, "Resume ride", &palette).clicked() {
                        resume = true;
                    }
                    if theme::ghost_button(ui, Some(Icon::Check), "Save & finish", &palette).clicked()
                    {
                        finalize = true;
                    }
                    if theme::ghost_button(ui, Some(Icon::Trash), "Discard", &palette).clicked() {
                        discard = true;
                    }
                });
                let _ = now;
            });
        if resume {
            self.recovery_resume(now);
        } else if finalize {
            self.recovery_finalize(now);
        } else if discard {
            self.recovery_discard();
        }
    }

    fn delete_dialogs(&mut self, ctx: &egui::Context) {
        let palette = self.palette;
        if let Some(id) = self.pending_delete_activity {
            let name = self
                .activities
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "this activity".to_string());
            let mut confirm = false;
            let mut cancel = false;
            egui::Window::new("delete_activity")
                .title_bar(false)
                .resizable(false)
                .collapsible(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(theme::card(&palette).inner_margin(Margin::symmetric(24, 22)))
                .show(ctx, |ui| {
                    ui.set_width(360.0);
                    ui.label(
                        RichText::new("Delete activity?")
                            .font(theme::font_display(20.0))
                            .color(palette.ink),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "“{name}” and all its samples will be removed from this computer."
                        ))
                        .font(theme::font_body(12.5))
                        .color(palette.muted),
                    );
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Delete")
                                        .font(theme::font_semi(13.0))
                                        .color(Color32::WHITE),
                                )
                                .fill(palette.danger)
                                .stroke(Stroke::NONE)
                                .corner_radius(CornerRadius::same(10)),
                            )
                            .clicked()
                        {
                            confirm = true;
                        }
                        if theme::ghost_button(ui, None, "Cancel", &palette).clicked() {
                            cancel = true;
                        }
                    });
                });
            if confirm {
                self.pending_delete_activity = None;
                self.delete_activity(id);
            } else if cancel {
                self.pending_delete_activity = None;
            }
        }

        if self.pending_discard {
            let mut confirm = false;
            let mut cancel = false;
            egui::Window::new("discard_ride")
                .title_bar(false)
                .resizable(false)
                .collapsible(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(theme::card(&palette).inner_margin(Margin::symmetric(24, 22)))
                .show(ctx, |ui| {
                    ui.set_width(380.0);
                    ui.label(
                        RichText::new("Discard this ride?")
                            .font(theme::font_display(20.0))
                            .color(palette.ink),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "The recorded samples will be deleted. This cannot be undone.",
                        )
                        .font(theme::font_body(12.5))
                        .color(palette.muted),
                    );
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Discard")
                                        .font(theme::font_semi(13.0))
                                        .color(Color32::WHITE),
                                )
                                .fill(palette.danger)
                                .stroke(Stroke::NONE)
                                .corner_radius(CornerRadius::same(10)),
                            )
                            .clicked()
                        {
                            confirm = true;
                        }
                        if theme::ghost_button(ui, None, "Keep recording", &palette).clicked() {
                            cancel = true;
                        }
                    });
                });
            if confirm {
                self.pending_discard = false;
                self.discard_ride(now_ms());
                self.page = Page::Dashboard;
            } else if cancel {
                self.pending_discard = false;
            }
        }

        if let Some(id) = self.pending_delete_route {
            let name = self
                .routes
                .iter()
                .find(|(rid, _)| *rid == id)
                .map(|(_, r)| r.name.clone())
                .unwrap_or_else(|| "this route".to_string());
            let mut confirm = false;
            let mut cancel = false;
            egui::Window::new("delete_route")
                .title_bar(false)
                .resizable(false)
                .collapsible(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(theme::card(&palette).inner_margin(Margin::symmetric(24, 22)))
                .show(ctx, |ui| {
                    ui.set_width(360.0);
                    ui.label(
                        RichText::new("Delete route?")
                            .font(theme::font_display(20.0))
                            .color(palette.ink),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("“{name}” will be removed from this computer."))
                            .font(theme::font_body(12.5))
                            .color(palette.muted),
                    );
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Delete")
                                        .font(theme::font_semi(13.0))
                                        .color(Color32::WHITE),
                                )
                                .fill(palette.danger)
                                .stroke(Stroke::NONE)
                                .corner_radius(CornerRadius::same(10)),
                            )
                            .clicked()
                        {
                            confirm = true;
                        }
                        if theme::ghost_button(ui, None, "Cancel", &palette).clicked() {
                            cancel = true;
                        }
                    });
                });
            if confirm {
                self.pending_delete_route = None;
                self.delete_route(id);
            } else if cancel {
                self.pending_delete_route = None;
            }
        }
    }

    fn toasts(&mut self, ctx: &egui::Context) {
        let now = now_ms();
        self.toasts.retain(|t| now - t.created_ms < 6_000);
        if self.toasts.is_empty() {
            return;
        }
        let palette = self.palette;
        let toasts = self.toasts.clone();
        egui::Area::new(Id::new("toasts"))
            .order(Order::Foreground)
            .anchor(Align2::RIGHT_BOTTOM, vec2(-26.0, -24.0))
            .show(ctx, |ui| {
                for toast in toasts.iter() {
                    let (fg, bg, icon) = match toast.kind {
                        ToastKind::Info => (palette.ink, palette.surface, Icon::Info),
                        ToastKind::Success => (palette.positive, palette.surface, Icon::Check),
                        ToastKind::Error => (palette.danger, palette.surface, Icon::Info),
                    };
                    theme::card(&palette)
                        .inner_margin(Margin::symmetric(16, 12))
                        .show(ui, |ui| {
                            ui.set_max_width(420.0);
                            ui.horizontal(|ui| {
                                let (rect, _) =
                                    ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
                                ui.painter().rect_filled(
                                    rect,
                                    CornerRadius::same(8),
                                    if matches!(toast.kind, ToastKind::Error) {
                                        palette.danger_soft
                                    } else if matches!(toast.kind, ToastKind::Success) {
                                        palette.positive_soft
                                    } else {
                                        palette.surface_alt
                                    },
                                );
                                theme::paint_icon(ui, rect.shrink(6.0), icon, fg);
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(&toast.text)
                                        .font(theme::font_body(12.5))
                                        .color(palette.ink_soft),
                                );
                            });
                        });
                    ui.add_space(6.0);
                    let _ = bg;
                }
            });
    }

    fn screenshot_tick(&mut self, ctx: &egui::Context) {
        let Some(plan) = &mut self.screenshot else {
            return;
        };
        plan.frame += 1;
        if plan.done.load(Ordering::Relaxed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            self.screenshot = None;
            return;
        }
        let waited = plan.started.elapsed().as_millis() as u64 >= plan.delay_ms;
        if plan.frame >= 10 && waited && !plan.requested {
            plan.requested = true;
            let path = plan.path.clone();
            let done = Arc::clone(&plan.done);
            ctx.request_screenshot(move |image| {
                if let Err(err) = write_png(&path, &image) {
                    eprintln!("screenshot failed: {err}");
                }
                done.store(true, Ordering::Relaxed);
            });
            ctx.request_repaint();
        }
        if plan.frame > 600 + (plan.delay_ms / 8) as u32 {
            eprintln!("screenshot timed out");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            self.screenshot = None;
        }
    }
}

fn write_png(path: &std::path::Path, image: &egui::ColorImage) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let [width, height] = image.size;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for pixel in &image.pixels {
        rgba.extend_from_slice(&pixel.to_srgba_unmultiplied());
    }
    let file = std::fs::File::create(path)?;
    let writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&rgba)?;
    writer.finish()?;
    Ok(())
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
