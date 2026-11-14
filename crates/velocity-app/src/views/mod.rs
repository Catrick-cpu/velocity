//! Full-page views. Each page is an inherent `impl VelocityApp` block so pages
//! can read live app state and trigger cross-cutting actions without a
//! framework of message types.

pub mod activities;
pub mod dashboard;
pub mod devices;
pub mod ride;
pub mod routes;
pub mod settings;
pub mod world;

use crate::theme::{self, Icon, Palette};
use eframe::egui::{self, Color32, RichText, Ui};
use velocity_core::types::Reading;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Dashboard,
    Ride,
    Activities,
    Routes,
    Devices,
    Settings,
}

impl Page {
    pub fn label(self) -> &'static str {
        match self {
            Self::Dashboard => "Overview",
            Self::Ride => "Live Ride",
            Self::Activities => "Activities",
            Self::Routes => "Routes",
            Self::Devices => "Devices",
            Self::Settings => "Settings",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Self::Dashboard => Icon::Dashboard,
            Self::Ride => Icon::Ride,
            Self::Activities => Icon::Activities,
            Self::Routes => Icon::Routes,
            Self::Devices => Icon::Devices,
            Self::Settings => Icon::Settings,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Dashboard => "dashboard",
            Self::Ride => "ride",
            Self::Activities => "activities",
            Self::Routes => "routes",
            Self::Devices => "devices",
            Self::Settings => "settings",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_lowercase().as_str() {
            "dashboard" | "overview" | "home" => Some(Self::Dashboard),
            "ride" | "live" => Some(Self::Ride),
            "activities" | "activity" | "history" => Some(Self::Activities),
            "routes" | "route" => Some(Self::Routes),
            "devices" | "sensors" | "bluetooth" => Some(Self::Devices),
            "settings" | "preferences" => Some(Self::Settings),
            _ => None,
        }
    }

    pub fn all() -> [Self; 6] {
        [
            Self::Dashboard,
            Self::Ride,
            Self::Activities,
            Self::Routes,
            Self::Devices,
            Self::Settings,
        ]
    }
}

/// Convert a stored speed reading (m/s) to km/h for display.
pub(crate) fn speed_kmh(reading: Option<Reading>) -> Option<f32> {
    reading.map(|r| r.value * 3.6)
}

pub(crate) fn fmt_opt(value: Option<f32>, decimals: usize) -> String {
    match value {
        Some(v) => format!("{v:.decimals$}"),
        None => "—".to_string(),
    }
}

pub(crate) fn fmt_opt_int(value: Option<f32>) -> String {
    match value {
        Some(v) => format!("{v:.0}"),
        None => "—".to_string(),
    }
}

pub(crate) fn fmt_speed(reading: Option<Reading>) -> String {
    match speed_kmh(reading) {
        Some(v) => format!("{v:.1}"),
        None => "—".to_string(),
    }
}

/// SIM / STALE / origin badge for a live reading.
pub(crate) fn reading_badge(reading: Option<Reading>, now: i64) -> Option<String> {
    let reading = reading?;
    if reading.simulated {
        Some("SIM".to_string())
    } else if reading.is_stale(now, 5_000) {
        Some("STALE".to_string())
    } else {
        Some(reading.origin.label().to_ascii_uppercase())
    }
}

pub(crate) fn metric_tile(
    ui: &mut Ui,
    palette: &Palette,
    icon: Icon,
    label: &str,
    value: &str,
    unit: &str,
    badge: Option<&str>,
    accent: bool,
) {
    let fill = if accent {
        palette.accent_soft
    } else {
        palette.surface
    };
    let stroke = if accent {
        theme::Palette::alpha(palette.accent, 110)
    } else {
        palette.line
    };
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(egui::CornerRadius::same(16))
        .inner_margin(egui::Margin::symmetric(16, 14))
        .shadow(palette.card_shadow())
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(92.0);
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::Vec2::splat(18.0), egui::Sense::hover());
                theme::paint_icon(
                    ui,
                    rect,
                    icon,
                    if accent { palette.ink } else { palette.faint },
                );
                ui.add_space(2.0);
                ui.label(
                    RichText::new(label.to_ascii_uppercase())
                        .font(theme::font_semi(9.5))
                        .color(palette.faint),
                );
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(value)
                        .font(theme::font_mono(if accent { 29.0 } else { 27.0 }))
                        .color(palette.ink),
                );
                ui.add_space(2.0);
                ui.label(
                    RichText::new(unit)
                        .font(theme::font_semi(10.5))
                        .color(palette.muted),
                );
            });
            if let Some(badge) = badge {
                ui.add_space(3.0);
                let (fg, bg) = match badge {
                    "SIM" => (palette.accent_ink, palette.accent),
                    "STALE" => (palette.warning, palette.warning_soft),
                    "—" => (palette.muted, palette.surface_sunken),
                    _ => (palette.muted, palette.surface_sunken),
                };
                let _ = theme::pill(ui, badge, fg, bg);
            }
        });
}

/// A framed card with a heading row and optional right-aligned action text.
pub(crate) fn titled_card(
    ui: &mut Ui,
    palette: &Palette,
    title: &str,
    subtitle: Option<&str>,
    add: impl FnOnce(&mut Ui),
) {
    theme::card(palette).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(title)
                        .font(theme::font_display(17.0))
                        .color(palette.ink),
                );
                if let Some(subtitle) = subtitle {
                    ui.label(
                        RichText::new(subtitle)
                            .font(theme::font_body(11.5))
                            .color(palette.muted),
                    );
                }
            });
        });
        ui.add_space(10.0);
        add(ui);
    });
}

/// One key/value cell used in the detail panels.
pub(crate) fn stat_cell(ui: &mut Ui, palette: &Palette, label: &str, value: &str, unit: &str) {
    ui.vertical(|ui| {
        ui.label(
            RichText::new(label.to_ascii_uppercase())
                .font(theme::font_semi(9.0))
                .color(palette.faint),
        );
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(value)
                    .font(theme::font_mono(19.0))
                    .color(palette.ink),
            );
            ui.label(
                RichText::new(unit)
                    .font(theme::font_semi(9.5))
                    .color(palette.muted),
            );
        });
    });
}

pub(crate) fn state_pill(ui: &mut Ui, palette: &Palette, text: &str, kind: ToastKind) {
    let (fg, bg) = match kind {
        ToastKind::Success => (palette.accent_ink, palette.accent),
        ToastKind::Info => (palette.blue, palette.blue_soft),
        ToastKind::Error => (palette.danger, palette.danger_soft),
    };
    let _ = theme::pill(ui, text, fg, bg);
}

pub(crate) use crate::app::ToastKind;

/// Small diagnostics row used on Devices and Settings.
pub(crate) fn info_row(ui: &mut Ui, palette: &Palette, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .font(theme::font_body(12.0))
                .color(palette.muted),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(value)
                    .font(theme::font_mono(11.5))
                    .color(palette.ink_soft),
            );
        });
    });
}

pub(crate) fn accent_switch(ui: &mut Ui, palette: &Palette, on: &mut bool, label: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let (rect, response) =
            ui.allocate_exact_size(egui::Vec2::new(38.0, 22.0), egui::Sense::click());
        if response.clicked() {
            *on = !*on;
            changed = true;
        }
        let t = if *on { 1.0 } else { 0.0 };
        let bg = if *on {
            palette.accent
        } else {
            palette.surface_sunken
        };
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(11), bg);
        let knob_x = rect.left() + 11.0 + t * 16.0;
        ui.painter().circle_filled(
            egui::pos2(knob_x, rect.center().y),
            8.0,
            if *on { palette.accent_ink } else { palette.muted },
        );
        ui.label(
            RichText::new(label)
                .font(theme::font_body(12.0))
                .color(palette.ink_soft),
        );
        response.on_hover_cursor(egui::CursorIcon::PointingHand);
    });
    changed
}

/// Colour chosen for a given provenance label.
pub(crate) fn provenance_color(palette: &Palette, label: &str) -> Color32 {
    match label {
        "SIM" | "SIMULATION" => palette.accent_ink,
        "STALE" => palette.warning,
        "BLUETOOTH" | "GPS" => palette.positive,
        "IMPORT" => palette.blue,
        _ => palette.muted,
    }
}

/// Shared "no sensors yet" hint.
pub(crate) fn sensor_hint(ui: &mut Ui, palette: &Palette) {
    theme::soft_card(palette).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            theme::dot(ui, palette.warning, 7.0);
            ui.label(
                RichText::new(
                    "No live sensors yet. Enable simulated telemetry to explore, or connect your trainer and heart-rate strap.",
                )
                .font(theme::font_body(11.5))
                .color(palette.muted),
            );
        });
    });
}
