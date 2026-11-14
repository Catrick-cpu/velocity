//! Manual hardware check: scan for BLE devices and print the normalized events.
//!
//! ```sh
//! cargo run -p velocity-sensors --example scan
//! ```
//!
//! Requires the user to be in the `bluetooth` group on Linux. No root needed.

use std::time::{Duration, Instant};
use velocity_sensors::{BleHub, DiscoveredDevice, HubCommand, HubEvent};

fn main() {
    let mut hub = BleHub::spawn();
    let _ = hub.send(HubCommand::StartScan);
    let start = Instant::now();
    let timeout = Duration::from_secs(10);
    let mut devices: Vec<DiscoveredDevice> = Vec::new();
    let mut events = 0usize;

    while start.elapsed() < timeout {
        for event in hub.poll_events() {
            events += 1;
            match event {
                HubEvent::Adapter { available, message } => {
                    println!("adapter available={available}: {message}");
                }
                HubEvent::ScanState { scanning } => println!("scanning={scanning}"),
                HubEvent::Device(device) => {
                    println!(
                        "  {:24} {:16} rssi={:>4} id={}",
                        device.name,
                        device.kind.label(),
                        device
                            .rssi
                            .map(|r| r.to_string())
                            .unwrap_or_else(|| "-".into()),
                        device.id
                    );
                    if !devices.iter().any(|d| d.id == device.id) {
                        devices.push(device);
                    }
                }
                other => println!("{other:?}"),
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let _ = hub.send(HubCommand::StopScan);
    let _ = hub.send(HubCommand::Shutdown);
    println!("{} events, {} devices", events, devices.len());
    if events == 0 {
        eprintln!("No Bluetooth events received. Check that BlueZ is running and the adapter is powered.");
        std::process::exit(2);
    }
}
