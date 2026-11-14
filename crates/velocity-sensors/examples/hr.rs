//! Manual hardware check: connect to the first heart-rate device found and
//! print live readings.
//!
//! ```sh
//! cargo run -p velocity-sensors --example hr
//! ```
//!
//! On a Garmin watch, enable **Broadcast Heart Rate** first. No root needed.

use std::time::{Duration, Instant};
use velocity_sensors::{BleHub, HubCommand, HubEvent, SensorKind};

fn main() {
    let mut hub = BleHub::spawn();
    let _ = hub.send(HubCommand::StartScan);
    let start = Instant::now();
    let mut target: Option<String> = None;
    let mut connected = false;
    let mut readings = 0usize;

    while start.elapsed() < Duration::from_secs(25) {
        for event in hub.poll_events() {
            match event {
                HubEvent::Device(device)
                    if target.is_none() && device.kind == SensorKind::HeartRate =>
                {
                    println!("found {} ({})", device.name, device.id);
                    target = Some(device.id.clone());
                    let _ = hub.send(HubCommand::StopScan);
                    let _ = hub.send(HubCommand::Connect {
                        id: device.id,
                        kind: SensorKind::HeartRate,
                    });
                }
                HubEvent::Connected { name, kind, .. } => {
                    println!("connected to {name} ({})", kind.label());
                    connected = true;
                }
                HubEvent::Reading { id, reading, .. } => {
                    readings += 1;
                    println!(
                        "{:>6.1} {} ({}) at t={} id={}",
                        reading.value,
                        reading.metric.unit(),
                        reading.metric.label(),
                        reading.t_ms,
                        id
                    );
                }
                HubEvent::Status { id, message } => println!("status {id}: {message}"),
                HubEvent::Disconnected { id, reason } => {
                    println!("disconnected {id}: {reason:?}");
                    break;
                }
                HubEvent::Error { id, message } => println!("error {id:?}: {message}"),
                _ => {}
            }
        }
        if readings >= 10 {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let _ = hub.send(HubCommand::Shutdown);
    if !connected {
        eprintln!("No heart-rate device connected.");
        std::process::exit(2);
    }
    if readings == 0 {
        eprintln!("Connected but no readings. Enable Broadcast Heart Rate on the watch.");
        std::process::exit(3);
    }
    println!("{readings} readings received");
}
