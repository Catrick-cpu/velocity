//! Overview dashboard: weekly totals, training load, current route and recent
//! activities. Designed as the first screen after launch.

use crate::app::VelocityApp;
use crate::charts;
use crate::theme::{self, Icon, Palette};
use crate::views::{Page, metric_tile, stat_cell};
use eframe::egui::{self, Align, Layout, RichText, Ui};
use velocity_core::telemetry::SessionState;
use velocity_core::types::{
    fmt_distance_km, fmt_duration, fmt_duration_compact, local_date, now_ms,
};

impl VelocityApp {
    pub(crate) fn dashboard(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        let now = now_ms();
        let name = self.rider.name.clone();
        let active = self.session.state.is_active();
        let mut start = false;
        let mut open_devices = false;
        let mut open_ride = false;

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(format!("TODAY · {}", local_date(now)))
                        .font(theme::font_semi(10.0))
                        .color(palette.faint),
                );
                ui.add_space(3.0);
                ui.label(
                    RichText::new(format!("{}, {}.", greeting(), name))
                        .font(theme::font_display(34.0))
                        .color(palette.ink),
                );
                ui.add_space(3.0);
                ui.label(
                    RichText::new(
                        "Your whole training world, local-first. Nothing leaves this computer.",
                    )
                    .font(theme::font_body(13.0))
                    .color(palette.muted),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if active {
                    if theme::accent_button(ui, Icon::Ride, "Open live ride", &palette).clicked() {
                        open_ride = true;
                    }
                } else if theme::accent_button(ui, Icon::Play, "Start a ride", &palette).clicked() {
                    start = true;
                }
                ui.add_space(8.0);
                if theme::ghost_button(ui, Some(Icon::Bluetooth), "Devices", &palette).clicked() {
                    open_devices = true;
                }
            });
        });
        ui.add_space(18.0);

        if active {
            active_ride_banner(self, ui, &palette, now);
            ui.add_space(14.0);
        } else if self.connected_count() == 0 && !self.sim_enabled {
            theme::soft_card(&palette).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    theme::dot(ui, palette.warning, 8.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("No sensors connected yet")
                                .font(theme::font_semi(13.0))
                                .color(palette.ink),
                        );
                        ui.label(
                            RichText::new(
                                "Connect your Wahoo KICKR or Garmin heart-rate strap, or enable the simulator to explore VELOCITY.",
                            )
                            .font(theme::font_body(11.5))
                            .color(palette.muted),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if theme::ghost_button(ui, Some(Icon::Bluetooth), "Connect devices", &palette)
                            .clicked()
                        {
                            open_devices = true;
                        }
                    });
                });
            });
            ui.add_space(14.0);
        }

        // Weekly totals -----------------------------------------------------
        let (distance, seconds, tss, gain) = self.week_totals();
        let rides = self
            .activities
            .iter()
            .filter(|a| a.started_at_ms >= now - 7 * 86_400_000)
            .count();
        let tss_text = if tss > 0.0 {
            format!("{tss:.0}")
        } else {
            "—".to_string()
        };
        ui.columns(4, |cols| {
            metric_tile(
                &mut cols[0],
                &palette,
                Icon::Routes,
                "Distance · 7 days",
                &format!("{:.1}", distance / 1000.0),
                "km",
                Some(&format!("{rides} rides")),
                false,
            );
            metric_tile(
                &mut cols[1],
                &palette,
                Icon::Timer,
                "Moving time",
                &fmt_duration_compact(seconds),
                "",
                None,
                false,
            );
            metric_tile(
                &mut cols[2],
                &palette,
                Icon::Bolt,
                "Training load",
                &tss_text,
                "TSS",
                Some("7-day"),
                true,
            );
            metric_tile(
                &mut cols[3],
                &palette,
                Icon::Mountain,
                "Climbing",
                &format!("{gain:.0}"),
                "m",
                None,
                false,
            );
        });
        ui.add_space(16.0);

        // Training load + current route -------------------------------------
        let history = self.training_history();
        let current_route = self.current_route.clone();
        let progress = self.current_route_progress();
        ui.columns(2, |cols| {
            training_card(&mut cols[0], &palette, &history);
            route_card(
                &mut cols[1],
                &palette,
                current_route.as_ref().map(|(_, r)| r),
                progress,
            );
        });
        ui.add_space(16.0);
        self.recent_activities_card(ui, &palette);

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("LOCAL-FIRST")
                    .font(theme::font_semi(9.0))
                    .color(palette.faint),
            );
            theme::dot(ui, palette.positive, 5.0);
            ui.label(
                RichText::new("SQLite on this computer")
                    .font(theme::font_mono(9.5))
                    .color(palette.faint),
            );
            ui.add_space(10.0);
            ui.label(
                RichText::new("No account, no cloud, no telemetry.")
                    .font(theme::font_body(10.5))
                    .color(palette.faint),
            );
        });

        if open_devices {
            self.page = Page::Devices;
        }
        if open_ride {
            self.page = Page::Ride;
        }
        if start {
            self.page = Page::Ride;
            self.start_ride(now);
        }
    }

    fn recent_activities_card(&mut self, ui: &mut Ui, palette: &Palette) {
        let mut open: Option<i64> = None;
        let recent: Vec<_> = self.activities.iter().take(5).cloned().collect();
        crate::views::titled_card(
            ui,
            palette,
            "Recent activities",
            Some("Everything is recorded at 1 Hz and stored locally"),
            |ui| {
                if recent.is_empty() {
                    theme::empty_state(
                        ui,
                        palette,
                        Icon::Activities,
                        "No rides yet",
                        "Start a ride or import a FIT, GPX or TCX file.",
                    );
                } else {
                    for activity in &recent {
                        let response = theme::list_row(ui, palette, false, 56.0, |ui, palette| {
                            ui.horizontal(|ui| {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::Vec2::splat(34.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(
                                    rect,
                                    egui::CornerRadius::same(10),
                                    palette.surface_alt,
                                );
                                theme::paint_icon(
                                    ui,
                                    rect.shrink(9.0),
                                    if matches!(
                                        activity.sport,
                                        velocity_core::types::Sport::MountainBike
                                            | velocity_core::types::Sport::Gravel
                                    ) {
                                        Icon::Mountain
                                    } else {
                                        Icon::Ride
                                    },
                                    palette.muted,
                                );
                                ui.add_space(4.0);
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(&activity.name)
                                            .font(theme::font_semi(13.5))
                                            .color(palette.ink),
                                    );
                                    ui.label(
                                        RichText::new(format!(
                                            "{} · {} · {}",
                                            local_date(activity.started_at_ms),
                                            activity.sport.label(),
                                            activity.source
                                        ))
                                        .font(theme::font_body(10.5))
                                        .color(palette.faint),
                                    );
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(
                                        RichText::new(
                                            activity
                                                .summary
                                                .tss
                                                .map(|t| format!("{t:.0} TSS"))
                                                .unwrap_or_else(|| "— TSS".into()),
                                        )
                                        .font(theme::font_mono(10.5))
                                        .color(palette.muted),
                                    );
                                    ui.add_space(12.0);
                                    ui.label(
                                        RichText::new(fmt_duration(
                                            activity.summary.moving_s.max(
                                                activity.summary.duration_s,
                                            ),
                                        ))
                                        .font(theme::font_mono(11.5))
                                        .color(palette.ink_soft),
                                    );
                                    ui.add_space(12.0);
                                    ui.label(
                                        RichText::new(fmt_distance_km(
                                            activity.summary.distance_m,
                                        ))
                                        .font(theme::font_mono(11.5))
                                        .color(palette.ink_soft),
                                    );
                                });
                            });
                        });
                        if response.clicked() {
                            open = Some(activity.id);
                        }
                        ui.add_space(2.0);
                    }
                }
            },
        );
        if let Some(id) = open {
            self.open_activity(id);
        }
    }
}

fn greeting() -> &'static str {
    use chrono::Timelike;
    match chrono::Local::now().hour() {
        5..=11 => "Good morning",
        12..=17 => "Good afternoon",
        18..=22 => "Good evening",
        _ => "Late-night training",
    }
}

fn active_ride_banner(app: &VelocityApp, ui: &mut Ui, palette: &Palette, now: i64) {
    let session = &app.session;
    let paused = session.state == SessionState::Paused;
    egui::Frame::new()
        .fill(palette.accent_soft)
        .stroke(egui::Stroke::new(1.0, theme::Palette::alpha(palette.accent, 130)))
        .corner_radius(egui::CornerRadius::same(16))
        .inner_margin(egui::Margin::symmetric(18, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(
                    egui::Vec2::splat(36.0),
                    egui::Sense::hover(),
                );
                ui.painter()
                    .rect_filled(rect, egui::CornerRadius::same(10), palette.accent);
                theme::paint_icon(ui, rect.shrink(9.0), Icon::Ride, palette.accent_ink);
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(if paused {
                            "Ride paused"
                        } else {
                            "Ride in progress"
                        })
                        .font(theme::font_semi(14.0))
                        .color(palette.ink),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{} · {} · {}",
                            session.name,
                            fmt_duration(session.live.elapsed_s),
                            fmt_distance_km(session.live.distance_m)
                        ))
                        .font(theme::font_body(11.5))
                        .color(palette.ink_soft),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let _ = theme::pill(
                        ui,
                        if session.live.sim_flags() != 0 {
                            "SIMULATED TELEMETRY"
                        } else {
                            "SENSOR TELEMETRY"
                        },
                        if session.live.sim_flags() != 0 {
                            palette.accent_ink
                        } else {
                            palette.positive
                        },
                        if session.live.sim_flags() != 0 {
                            palette.accent
                        } else {
                            palette.positive_soft
                        },
                    );
                    ui.add_space(8.0);
                    let _ = now;
                });
            });
        });
}

fn training_card(ui: &mut Ui, palette: &Palette, history: &[velocity_core::analytics::DailyLoad]) {
    crate::views::titled_card(
        ui,
        palette,
        "Training load",
        Some("Fitness, fatigue and form over the last 42 days"),
        |ui| {
            let last = history.last().cloned().unwrap_or_default();
            let has_data = history.iter().any(|d| d.tss > 0.0);
            ui.columns(3, |cols| {
                stat_cell(&mut cols[0], palette, "Fitness · CTL", &format!("{:.0}", last.ctl), "");
                stat_cell(&mut cols[1], palette, "Fatigue · ATL", &format!("{:.0}", last.atl), "");
                stat_cell(
                    &mut cols[2],
                    palette,
                    "Form · TSB",
                    &format!("{:+.0}", last.tsb),
                    "",
                );
            });
            ui.add_space(12.0);
            let recent = &history[history.len().saturating_sub(14)..];
            charts::week_bars(ui, palette, recent, 104.0);
            ui.add_space(8.0);
            ui.label(
                RichText::new("DAILY TSS · LAST 14 DAYS")
                    .font(theme::font_semi(8.5))
                    .color(palette.faint),
            );
            if has_data {
                ui.add_space(6.0);
                charts::load_plot(ui, palette, history, 92.0);
            } else {
                ui.add_space(6.0);
                charts::chart_placeholder(
                    ui,
                    palette,
                    "No training history yet — record or import a ride",
                    92.0,
                );
            }
        },
    );
}

fn route_card(
    ui: &mut Ui,
    palette: &Palette,
    route: Option<&velocity_core::route::Route>,
    progress: Option<f64>,
) {
    crate::views::titled_card(
        ui,
        palette,
        "Current route",
        Some("Route-driven trainer resistance and a live position marker"),
        |ui| {
            match route {
                Some(route) => {
                    charts::route_map(ui, palette, route, progress, 188.0);
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(&route.name)
                                    .font(theme::font_semi(13.5))
                                    .color(palette.ink),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "{} · {:.0} m gain · {} difficulty",
                                    fmt_distance_km(route.stats.distance_m),
                                    route.stats.elevation_gain_m,
                                    route.stats.difficulty()
                                ))
                                .font(theme::font_body(10.5))
                                .color(palette.faint),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if progress.is_some() {
                                let _ = theme::pill(
                                    ui,
                                    "RIDE IN PROGRESS",
                                    palette.accent_ink,
                                    palette.accent,
                                );
                            }
                        });
                    });
                }
                None => {
                    theme::empty_state(
                        ui,
                        palette,
                        Icon::Routes,
                        "No route selected",
                        "Import a GPX track to ride a real-world course.",
                    );
                }
            }
        },
    );
}
