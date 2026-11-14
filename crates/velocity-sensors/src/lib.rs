//! Sensor integration for VELOCITY.
//!
//! Two independent sources feed the app through the same small data model:
//!
//! - [`sim::Simulator`] — deterministic, clearly marked simulated telemetry.
//! - [`ble::BleHub`] — real Bluetooth LE trainers and heart-rate straps
//!   (FTMS, Cycling Power, Heart Rate, CSC).
//!
//! Protocol codecs live in [`ftms`] and are pure functions so they can be
//! tested byte-for-byte without hardware.

pub mod ble;
pub mod ftms;
pub mod sim;

pub use ble::{BleHub, DiscoveredDevice, HubCommand, HubEvent, SensorKind};
pub use ftms::RevTracker;
pub use sim::{SimReadings, Simulator, SimulatorConfig};

use serde::{Deserialize, Serialize};
use velocity_core::types::DataOrigin;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensorMetric {
    Power,
    HeartRate,
    Cadence,
    Speed,
    Resistance,
}

impl SensorMetric {
    pub fn label(self) -> &'static str {
        match self {
            Self::Power => "Power",
            Self::HeartRate => "Heart rate",
            Self::Cadence => "Cadence",
            Self::Speed => "Speed",
            Self::Resistance => "Resistance",
        }
    }

    pub fn unit(self) -> &'static str {
        match self {
            Self::Power => "W",
            Self::HeartRate => "bpm",
            Self::Cadence => "rpm",
            Self::Speed => "m/s",
            Self::Resistance => "%",
        }
    }
}

/// One channel value from a device, already timestamped at receipt.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SensorReading {
    pub metric: SensorMetric,
    pub value: f32,
    pub t_ms: i64,
    pub origin: DataOrigin,
}

impl SensorReading {
    /// Milliseconds since this reading was received, never negative.
    pub fn age_ms(&self, now_ms: i64) -> i64 {
        (now_ms - self.t_ms).max(0)
    }

    /// Convert to the shared core reading type used by the UI and telemetry.
    pub fn to_reading(self) -> velocity_core::types::Reading {
        velocity_core::types::Reading::new(
            self.value,
            self.t_ms,
            self.origin == DataOrigin::Simulated,
            self.origin,
        )
    }
}
