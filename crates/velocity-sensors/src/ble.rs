//! Bluetooth Low Energy hub.
//!
//! The UI never touches `btleplug` directly. [`BleHub`] owns a background
//! Tokio runtime and exchanges plain messages with the app:
//!
//! ```text
//! UI thread ──HubCommand──▶ BLE thread ──HubEvent──▶ UI thread
//! ```
//!
//! This keeps the 60 fps egui loop independent from adapter timeouts and
//! reconnection backoff. Events are polled with [`BleHub::poll_events`].

use crate::ftms::{self, RevTracker};
use crate::{SensorMetric, SensorReading};
use btleplug::api::{
    Central, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use velocity_core::types::{now_ms, DataOrigin};

const RECONNECT_BACKOFF_MS: i64 = 5_000;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensorKind {
    SmartTrainer,
    HeartRate,
    PowerMeter,
    SpeedCadence,
    #[default]
    Unknown,
}

impl SensorKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::SmartTrainer => "Smart trainer",
            Self::HeartRate => "Heart rate",
            Self::PowerMeter => "Power meter",
            Self::SpeedCadence => "Speed / cadence",
            Self::Unknown => "Unknown device",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Self::SmartTrainer => "TRAINER",
            Self::HeartRate => "HR",
            Self::PowerMeter => "POWER",
            Self::SpeedCadence => "SPD/CAD",
            Self::Unknown => "BLE",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiscoveredDevice {
    pub id: String,
    pub name: String,
    pub kind: SensorKind,
    pub rssi: Option<i16>,
    pub services: Vec<String>,
    pub connected: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum HubEvent {
    /// Adapter availability and a human-readable diagnostic.
    Adapter { available: bool, message: String },
    ScanState { scanning: bool },
    Device(DiscoveredDevice),
    Connected {
        id: String,
        name: String,
        kind: SensorKind,
        services: Vec<String>,
    },
    Disconnected {
        id: String,
        reason: Option<String>,
    },
    Reading {
        id: String,
        kind: SensorKind,
        reading: SensorReading,
    },
    Control {
        id: String,
        opcode: u8,
        result: u8,
        detail: String,
    },
    Status {
        id: String,
        message: String,
    },
    Error {
        id: Option<String>,
        message: String,
    },
}

#[derive(Clone, Debug)]
pub enum HubCommand {
    StartScan,
    StopScan,
    Connect { id: String, kind: SensorKind },
    Disconnect { id: String },
    RequestControl { id: String },
    SetTargetPower { id: String, watts: f32 },
    SetTargetResistance { id: String, percent: f32 },
    SetGrade { id: String, grade_pct: f32 },
    SetSimulation {
        id: String,
        grade_pct: f32,
        wind_mps: f32,
        crr: f32,
        cw_kg_per_m: f32,
    },
    SetTargetCadence { id: String, rpm: f32 },
    StartTrainer { id: String },
    StopTrainer { id: String },
    Shutdown,
}

/// Handle to the Bluetooth background service.
pub struct BleHub {
    commands: UnboundedSender<HubCommand>,
    events: UnboundedReceiver<HubEvent>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl BleHub {
    pub fn spawn() -> Self {
        let (commands, cmd_rx) = unbounded_channel();
        let (event_tx, events) = unbounded_channel();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("velocity-ble")
            .build();
        let thread = match runtime {
            Ok(rt) => std::thread::Builder::new()
                .name("velocity-ble-rt".to_string())
                .spawn(move || {
                    rt.block_on(run_hub(cmd_rx, event_tx));
                })
                .ok(),
            Err(err) => {
                let _ = event_tx.send(HubEvent::Error {
                    id: None,
                    message: format!("could not start Bluetooth runtime: {err}"),
                });
                None
            }
        };
        Self {
            commands,
            events,
            thread,
        }
    }

    /// Enqueue a command. Returns `false` if the BLE service has stopped.
    pub fn send(&self, command: HubCommand) -> bool {
        self.commands.send(command).is_ok()
    }

    /// Drain pending events. Designed to be called once per UI frame.
    pub fn poll_events(&mut self) -> Vec<HubEvent> {
        let mut out = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            out.push(event);
            if out.len() >= 512 {
                break;
            }
        }
        out
    }

    pub fn is_running(&self) -> bool {
        self.thread.as_ref().map(|t| !t.is_finished()).unwrap_or(false)
    }

    pub fn shutdown(&mut self) {
        let _ = self.commands.send(HubCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            // Do not block the UI on a slow adapter; the thread owns its runtime.
            drop(thread);
        }
    }
}

impl Drop for BleHub {
    fn drop(&mut self) {
        let _ = self.commands.send(HubCommand::Shutdown);
    }
}

struct Connection {
    peripheral: Peripheral,
    control: Option<Characteristic>,
    last_reading_ms: Arc<AtomicI64>,
    stale_reported: bool,
}

fn infer_kind(name: &str, services: &[uuid::Uuid]) -> SensorKind {
    if services.contains(&ftms::FTMS_SERVICE)
        || services.contains(&ftms::WAHOO_SERVICE)
        || services.contains(&ftms::CPS_SERVICE)
    {
        return SensorKind::SmartTrainer;
    }
    if services.contains(&ftms::HR_SERVICE) {
        return SensorKind::HeartRate;
    }
    if services.contains(&ftms::CSC_SERVICE) {
        return SensorKind::SpeedCadence;
    }
    let lower = name.to_ascii_lowercase();
    for needle in [
        "tickr",
        "hrm",
        "forerunner",
        "fenix",
        "polar",
        "h10",
        "h9",
        "heart",
        "hr-",
        "hr ",
    ] {
        if lower.contains(needle) {
            return SensorKind::HeartRate;
        }
    }
    for needle in [
        "kickr", "wahoo", "neo", "tacx", "elite", "saris", "hammer", "direto", "trainer", "zwift",
    ] {
        if lower.contains(needle) {
            return SensorKind::SmartTrainer;
        }
    }
    if lower.contains("powermeter") || lower.contains("power meter") || lower.contains("assioma") {
        return SensorKind::PowerMeter;
    }
    SensorKind::Unknown
}

fn device_name(peripheral: &Peripheral, props: Option<&btleplug::api::PeripheralProperties>) -> String {
    props
        .and_then(|p| p.local_name.clone().or_else(|| p.advertisement_name.clone()))
        .unwrap_or_else(|| format!("BLE device {}", peripheral.id()))
}

async fn run_hub(mut cmd_rx: UnboundedReceiver<HubCommand>, events: UnboundedSender<HubEvent>) {
    let manager = match Manager::new().await {
        Ok(m) => m,
        Err(err) => {
            let _ = events.send(HubEvent::Adapter {
                available: false,
                message: format!("Bluetooth is unavailable: {err}"),
            });
            while let Some(command) = cmd_rx.recv().await {
                match command {
                    HubCommand::Shutdown => break,
                    HubCommand::StartScan => {
                        let _ = events.send(HubEvent::Error {
                            id: None,
                            message: "Cannot scan: no Bluetooth adapter.".to_string(),
                        });
                    }
                    _ => {}
                }
            }
            return;
        }
    };
    let adapter = match manager.adapters().await {
        Ok(adapters) if !adapters.is_empty() => adapters.into_iter().next().unwrap(),
        Ok(_) => {
            let _ = events.send(HubEvent::Adapter {
                available: false,
                message: "No Bluetooth adapter found. Connect a USB BLE dongle or enable Bluetooth."
                    .to_string(),
            });
            return;
        }
        Err(err) => {
            let _ = events.send(HubEvent::Adapter {
                available: false,
                message: format!("Could not list Bluetooth adapters: {err}"),
            });
            return;
        }
    };
    let adapter_info = adapter.adapter_info().await.unwrap_or_default();
    let _ = events.send(HubEvent::Adapter {
        available: true,
        message: if adapter_info.is_empty() {
            "Bluetooth adapter ready".to_string()
        } else {
            adapter_info
        },
    });

    let mut scanning = false;
    let mut seen: HashMap<String, (i16, String, i64)> = HashMap::new();
    let mut connections: HashMap<String, Connection> = HashMap::new();
    let mut wanted: HashMap<String, SensorKind> = HashMap::new();
    let mut last_attempt: HashMap<String, i64> = HashMap::new();
    let mut ticker = tokio::time::interval(Duration::from_millis(900));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            command = cmd_rx.recv() => {
                match command {
                    Some(HubCommand::Shutdown) | None => break,
                    Some(command) => {
                        let start_scan = matches!(&command, HubCommand::StartScan);
                        let stop_scan = matches!(&command, HubCommand::StopScan);
                        handle_command(&adapter, command, &events, &mut connections, &mut wanted, &mut last_attempt).await;
                        if start_scan {
                            scanning = true;
                        }
                        if stop_scan {
                            scanning = false;
                        }
                    }
                }
            }
            _ = ticker.tick() => {
                if scanning || !wanted.is_empty() || !connections.is_empty() {
                    refresh(
                        &adapter,
                        &events,
                        scanning,
                        &mut seen,
                        &mut connections,
                        &mut wanted,
                        &mut last_attempt,
                    ).await;
                }
            }
        }
    }

    for (_, conn) in connections {
        let _ = conn.peripheral.disconnect().await;
    }
    if scanning {
        let _ = adapter.stop_scan().await;
    }
}

async fn handle_command(
    adapter: &Adapter,
    command: HubCommand,
    events: &UnboundedSender<HubEvent>,
    connections: &mut HashMap<String, Connection>,
    wanted: &mut HashMap<String, SensorKind>,
    last_attempt: &mut HashMap<String, i64>,
) {
    match command {
        HubCommand::StartScan => match adapter.start_scan(ScanFilter::default()).await {
            Ok(()) => {
                let _ = events.send(HubEvent::ScanState { scanning: true });
            }
            Err(err) => send_error(events, None, format!("Could not start scanning: {err}")),
        },
        HubCommand::StopScan => {
            let _ = adapter.stop_scan().await;
            let _ = events.send(HubEvent::ScanState { scanning: false });
        }
        HubCommand::Connect { id, kind } => {
            wanted.insert(id.clone(), kind);
            connect_device(adapter, &id, kind, events, connections).await;
        }
        HubCommand::Disconnect { id } => {
            wanted.remove(&id);
            last_attempt.remove(&id);
            if let Some(conn) = connections.remove(&id) {
                let _ = conn.peripheral.disconnect().await;
                let _ = events.send(HubEvent::Disconnected {
                    id,
                    reason: Some("Disconnected by user".to_string()),
                });
            }
        }
        HubCommand::RequestControl { id } => {
            write_control(events, connections, &id, ftms::cmds_request_control()).await;
        }
        HubCommand::SetTargetPower { id, watts } => {
            write_control(
                events,
                connections,
                &id,
                ftms::cmds_set_target_power(watts),
            )
            .await;
        }
        HubCommand::SetTargetResistance { id, percent } => {
            write_control(
                events,
                connections,
                &id,
                ftms::cmds_set_target_resistance(percent),
            )
            .await;
        }
        HubCommand::SetGrade { id, grade_pct } => {
            write_control(
                events,
                connections,
                &id,
                ftms::cmds_set_simulation(grade_pct, 0.0, 0.005, 0.51),
            )
            .await;
        }
        HubCommand::SetSimulation {
            id,
            grade_pct,
            wind_mps,
            crr,
            cw_kg_per_m,
        } => {
            write_control(
                events,
                connections,
                &id,
                ftms::cmds_set_simulation(grade_pct, wind_mps, crr, cw_kg_per_m),
            )
            .await;
        }
        HubCommand::SetTargetCadence { id, rpm } => {
            write_control(
                events,
                connections,
                &id,
                ftms::cmds_set_target_cadence(rpm),
            )
            .await;
        }
        HubCommand::StartTrainer { id } => {
            write_control(events, connections, &id, ftms::cmds_start_resume()).await;
        }
        HubCommand::StopTrainer { id } => {
            write_control(events, connections, &id, ftms::cmds_stop()).await;
        }
        HubCommand::Shutdown => {}
    }
}

async fn write_control(
    events: &UnboundedSender<HubEvent>,
    connections: &HashMap<String, Connection>,
    id: &str,
    payload: Vec<u8>,
) {
    let Some(conn) = connections.get(id) else {
        send_error(events, Some(id.to_string()), "Device is not connected.".to_string());
        return;
    };
    let Some(control) = &conn.control else {
        send_error(
            events,
            Some(id.to_string()),
            "This device does not expose an FTMS control point; it may be read-only or use a proprietary protocol."
                .to_string(),
        );
        return;
    };
    match conn
        .peripheral
        .write(control, &payload, WriteType::WithResponse)
        .await
    {
        Ok(()) => {
            let _ = events.send(HubEvent::Control {
                id: id.to_string(),
                opcode: payload[0],
                result: 0x00,
                detail: "Command sent, waiting for trainer acknowledgement".to_string(),
            });
        }
        Err(err) => send_error(events, Some(id.to_string()), format!("Trainer write failed: {err}")),
    }
}

async fn refresh(
    adapter: &Adapter,
    events: &UnboundedSender<HubEvent>,
    scanning: bool,
    seen: &mut HashMap<String, (i16, String, i64)>,
    connections: &mut HashMap<String, Connection>,
    wanted: &mut HashMap<String, SensorKind>,
    last_attempt: &mut HashMap<String, i64>,
) {
    let peripherals = match adapter.peripherals().await {
        Ok(p) => p,
        Err(err) => {
            send_error(events, None, format!("Could not read peripherals: {err}"));
            return;
        }
    };
    let now = now_ms();

    if scanning {
        for peripheral in &peripherals {
            let props = peripheral.properties().await.ok().flatten();
            let name = device_name(peripheral, props.as_ref());
            let rssi = props.as_ref().and_then(|p| p.rssi).unwrap_or(-127);
            let services: Vec<uuid::Uuid> =
                props.as_ref().map(|p| p.services.clone()).unwrap_or_default();
            let kind = infer_kind(&name, &services);
            let id = peripheral.id().to_string();
            let changed = match seen.get(&id) {
                None => true,
                Some((last_rssi, last_name, last_emit)) => {
                    (*last_rssi - rssi).abs() >= 6
                        || last_name != &name
                        || now - last_emit >= 15_000
                }
            };
            if changed {
                seen.insert(id.clone(), (rssi, name.clone(), now));
                let connected = connections.contains_key(&id);
                let _ = events.send(HubEvent::Device(DiscoveredDevice {
                    id,
                    name,
                    kind,
                    rssi: (rssi > -127).then_some(rssi),
                    services: services.iter().map(|s| s.to_string()).collect(),
                    connected,
                }));
            }
        }
    }

    // Detect dropped links.
    let mut dropped = Vec::new();
    for (id, conn) in connections.iter() {
        match conn.peripheral.is_connected().await {
            Ok(true) => {}
            _ => dropped.push(id.clone()),
        }
    }
    for id in dropped {
        if let Some(conn) = connections.remove(&id) {
            let _ = conn.peripheral.disconnect().await;
            let _ = events.send(HubEvent::Disconnected {
                id: id.clone(),
                reason: Some("Connection lost".to_string()),
            });
            if wanted.contains_key(&id) {
                last_attempt.insert(id, now);
            }
        }
    }

    // Reconnect wanted devices.
    let pending: Vec<(String, SensorKind)> = wanted
        .iter()
        .filter(|(id, _)| !connections.contains_key(*id))
        .filter(|(id, _)| {
            now - last_attempt.get(*id).copied().unwrap_or(0) >= RECONNECT_BACKOFF_MS
        })
        .map(|(id, kind)| (id.clone(), *kind))
        .collect();
    for (id, kind) in pending {
        last_attempt.insert(id.clone(), now);
        connect_device(adapter, &id, kind, events, connections).await;
    }

    // One stale notice per connection, not a flood.
    for conn in connections.values_mut() {
        let last = conn.last_reading_ms.load(Ordering::Relaxed);
        if last > 0 {
            if !conn.stale_reported && now - last >= 8_000 {
                conn.stale_reported = true;
                let _ = events.send(HubEvent::Status {
                    id: conn.peripheral.id().to_string(),
                    message: "No data received for 8 s — check the device is awake.".to_string(),
                });
            } else if now - last < 8_000 {
                conn.stale_reported = false;
            }
        }
    }
}

async fn connect_device(
    adapter: &Adapter,
    id: &str,
    requested_kind: SensorKind,
    events: &UnboundedSender<HubEvent>,
    connections: &mut HashMap<String, Connection>,
) {
    if connections.contains_key(id) {
        return;
    }
    let peripheral = match adapter.peripherals().await {
        Ok(peripherals) => peripherals
            .into_iter()
            .find(|p| p.id().to_string() == id),
        Err(_) => None,
    };
    let Some(peripheral) = peripheral else {
        return;
    };

    if let Err(err) = peripheral.connect_with_timeout(CONNECT_TIMEOUT).await {
        send_error(
            events,
            Some(id.to_string()),
            format!("Could not connect: {err}"),
        );
        return;
    }
    if let Err(err) = peripheral.discover_services_with_timeout(DISCOVERY_TIMEOUT).await {
        let _ = peripheral.disconnect().await;
        send_error(
            events,
            Some(id.to_string()),
            format!("Could not discover services: {err}"),
        );
        return;
    }

    let props = peripheral.properties().await.ok().flatten();
    let name = device_name(&peripheral, props.as_ref());
    let services: Vec<uuid::Uuid> = props.as_ref().map(|p| p.services.clone()).unwrap_or_default();
    let kind = if requested_kind == SensorKind::Unknown {
        infer_kind(&name, &services)
    } else {
        requested_kind
    };

    let chars: Vec<Characteristic> = peripheral.characteristics().into_iter().collect();
    let mut subscribed = 0usize;
    for ch in &chars {
        if matches!(
            ch.uuid,
            ftms::FTMS_INDOOR_BIKE_DATA
                | ftms::FTMS_CONTROL_POINT
                | ftms::FTMS_STATUS
                | ftms::CPS_MEASUREMENT
                | ftms::HR_MEASUREMENT
                | ftms::CSC_MEASUREMENT
        ) {
            match peripheral.subscribe(ch).await {
                Ok(()) => subscribed += 1,
                Err(err) => log::warn!("subscribe {} failed: {err}", ch.uuid),
            }
        }
    }
    if subscribed == 0 {
        let _ = events.send(HubEvent::Status {
            id: id.to_string(),
            message: "Connected, but no supported telemetry characteristics were found.".to_string(),
        });
    }

    let control = chars
        .iter()
        .find(|c| c.uuid == ftms::FTMS_CONTROL_POINT)
        .cloned();

    // Subscribe before streaming so the first notifications are not lost.
    let last_reading_ms = Arc::new(AtomicI64::new(0));
    match peripheral.notifications().await {
        Ok(mut stream) => {
            let id = id.to_string();
            let event_tx = events.clone();
            let heartbeat = Arc::clone(&last_reading_ms);
            let mut tracker = RevTracker::default();
            tokio::spawn(async move {
                while let Some(notification) = stream.next().await {
                    let t = now_ms();
                    heartbeat.store(t, Ordering::Relaxed);
                    handle_notification(&event_tx, &id, kind, &mut tracker, &notification, t);
                }
            });
        }
        Err(err) => {
            let _ = peripheral.disconnect().await;
            send_error(
                events,
                Some(id.to_string()),
                format!("Could not open notification stream: {err}"),
            );
            return;
        }
    }

    // FTMS requires explicit control ownership; ask immediately. Some trainers
    // only grant it after their data notifications start, which is the case now.
    if let Some(control) = &control {
        if let Err(err) = peripheral
            .write(control, &ftms::cmds_request_control(), WriteType::WithResponse)
            .await
        {
            let _ = events.send(HubEvent::Status {
                id: id.to_string(),
                message: format!("Requested trainer control, write failed: {err}"),
            });
        }
    }

    let _ = events.send(HubEvent::Connected {
        id: id.to_string(),
        name: name.clone(),
        kind,
        services: services.iter().map(|s| s.to_string()).collect(),
    });
    connections.insert(
        id.to_string(),
        Connection {
            peripheral,
            control,
            last_reading_ms,
            stale_reported: false,
        },
    );
}

fn handle_notification(
    events: &UnboundedSender<HubEvent>,
    id: &str,
    kind: SensorKind,
    tracker: &mut RevTracker,
    notification: &btleplug::api::ValueNotification,
    t_ms: i64,
) {
    match notification.uuid {
        ftms::FTMS_INDOOR_BIKE_DATA => {
            if let Some(data) = ftms::parse_indoor_bike_data(&notification.value) {
                if let Some(v) = data.speed_mps {
                    emit(events, id, kind, SensorMetric::Speed, v, t_ms);
                }
                if let Some(v) = data.cadence_rpm {
                    emit(events, id, kind, SensorMetric::Cadence, v, t_ms);
                }
                if let Some(v) = data.power_w {
                    emit(events, id, kind, SensorMetric::Power, v, t_ms);
                }
                if let Some(v) = data.heart_rate_bpm {
                    emit(events, id, kind, SensorMetric::HeartRate, v, t_ms);
                }
                if let Some(v) = data.resistance_level {
                    emit(events, id, kind, SensorMetric::Resistance, v, t_ms);
                }
            }
        }
        ftms::FTMS_CONTROL_POINT => {
            if let Some(resp) = ftms::parse_control_response(&notification.value) {
                let detail = if resp.ok() {
                    format!("{} acknowledged", resp.opcode_label())
                } else {
                    format!("{}: {}", resp.opcode_label(), resp.result_label())
                };
                let _ = events.send(HubEvent::Control {
                    id: id.to_string(),
                    opcode: resp.request_opcode,
                    result: resp.result,
                    detail,
                });
            }
        }
        ftms::FTMS_STATUS => {
            let code = notification.value.first().copied().unwrap_or(0);
            let message = match code {
                0x01 => "Trainer reset".to_string(),
                0x02 => "Stopped by user".to_string(),
                0x03 => "Stopped by safety key".to_string(),
                0x04 => "Started by user".to_string(),
                0x06 => "Target incline changed".to_string(),
                0x07 => "Target resistance changed".to_string(),
                0x08 => "Target power changed".to_string(),
                0x12 => "Simulation parameters changed".to_string(),
                0xFF => "Trainer control permission lost".to_string(),
                other => format!("Trainer status 0x{other:02X}"),
            };
            let _ = events.send(HubEvent::Status {
                id: id.to_string(),
                message,
            });
        }
        ftms::HR_MEASUREMENT => {
            if let Some(data) = ftms::parse_heart_rate(&notification.value) {
                emit(
                    events,
                    id,
                    kind,
                    SensorMetric::HeartRate,
                    data.bpm,
                    t_ms,
                );
            }
        }
        ftms::CPS_MEASUREMENT => {
            if let Some(data) = ftms::parse_cycling_power(&notification.value) {
                if let Some(v) = data.power_w {
                    emit(events, id, kind, SensorMetric::Power, v, t_ms);
                }
                if let Some(v) = tracker.cadence_rpm(data.crank) {
                    emit(events, id, kind, SensorMetric::Cadence, v, t_ms);
                }
                if let Some(v) = tracker.speed_mps(data.wheel) {
                    emit(events, id, kind, SensorMetric::Speed, v, t_ms);
                }
            }
        }
        ftms::CSC_MEASUREMENT => {
            if let Some(data) = ftms::parse_csc(&notification.value) {
                if let Some(v) = tracker.speed_mps(data.wheel) {
                    emit(events, id, kind, SensorMetric::Speed, v, t_ms);
                }
                if let Some(v) = tracker.cadence_rpm(data.crank) {
                    emit(events, id, kind, SensorMetric::Cadence, v, t_ms);
                }
            }
        }
        _ => {}
    }
}

fn emit(
    events: &UnboundedSender<HubEvent>,
    id: &str,
    kind: SensorKind,
    metric: SensorMetric,
    value: f32,
    t_ms: i64,
) {
    let _ = events.send(HubEvent::Reading {
        id: id.to_string(),
        kind,
        reading: SensorReading {
            metric,
            value,
            t_ms,
            origin: DataOrigin::Bluetooth,
        },
    });
}

fn send_error(events: &UnboundedSender<HubEvent>, id: Option<String>, message: String) {
    let _ = events.send(HubEvent::Error { id, message });
}

#[cfg(test)]
mod tests {
    use super::*;
    use btleplug::api::bleuuid::uuid_from_u16;

    #[test]
    fn infers_sensor_kinds_from_services_and_names() {
        let hr = vec![uuid_from_u16(0x180D)];
        assert_eq!(infer_kind("Wahoo TICKR", &hr), SensorKind::HeartRate);
        assert_eq!(infer_kind("Forerunner 245", &[]), SensorKind::HeartRate);
        let ftms_services = vec![uuid_from_u16(0x1826)];
        assert_eq!(infer_kind("KICKR 1234", &ftms_services), SensorKind::SmartTrainer);
        assert_eq!(infer_kind("Wahoo KICKR", &[]), SensorKind::SmartTrainer);
        assert_eq!(infer_kind("Random", &[]), SensorKind::Unknown);
    }

    #[test]
    fn event_queue_drains_without_async() {
        let (tx, rx) = unbounded_channel();
        tx.send(HubEvent::Status {
            id: "x".into(),
            message: "hello".into(),
        })
        .unwrap();
        drop(tx);
        let mut hub = BleHub {
            commands: unbounded_channel().0,
            events: rx,
            thread: None,
        };
        let events = hub.poll_events();
        assert_eq!(events.len(), 1);
    }
}
