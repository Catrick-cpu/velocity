//! VELOCITY core: domain model, physics, routes, analytics, telemetry and
//! local persistence. This crate is UI- and platform-independent.

pub mod analytics;
pub mod db;
pub mod demo;
pub mod formats;
pub mod physics;
pub mod route;
pub mod telemetry;
pub mod types;

pub use types::{APP_NAME, APP_VERSION};
