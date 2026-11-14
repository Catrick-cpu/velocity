//! Live ride: setup, recording controls, big telemetry, trainer control and the
//! route-driven virtual course view.

use crate::app::{ControlMode, VelocityApp};
use crate::charts::{self, Series};
use crate::theme::{self, Icon, Palette};
use crate::views::{
    Page, ToastKind, fmt_opt, fmt_opt_int, fmt_speed, metric_tile, reading_badge, state_pill,
    titled_card,
};
use eframe::egui::{self, Align, Layout, RichText, Ui};
use velocity_core::telemetry::{Metric, SessionState};
use velocity_core::types::{fmt_distance_km, fmt_duration, now_ms};
use velocity_sensors::{HubCommand, SensorKind, SensorMetric};

impl VelocityApp {
    pub(crate) fn ride(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        let now = now_ms();
        let active = self.session.state.is_active();
        let mut open_devices = false;

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("TRAIN · LIVE RIDE")
                        .font(theme::font_semi(10.0))
                        .color(palette.faint),
                );
                ui.add_space(3.0);
                ui.label(
                    RichText::new(if active {
                        "Ride control center"
                    } else {
                        "Set up your ride"
                    })
                    .font(theme::font_display(34.0))
                    .color(palette.ink),
                );
                ui.add_space(3.0);
                ui.label(
                    RichText::new(if active {
                        "Samples are written to SQLite every few seconds. A crash never loses the whole ride."
                    } else {
                        "Pick a route, choose how the trainer responds, then start recording."
                    })
                    .font(theme::font_body(13.0))
                    .color(palette.muted),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if active {
                    if self.session.state == SessionState::Paused {
                        state_pill(ui, &palette, "PAUSED", ToastKind::Info);
                    } else {
                        state_pill(ui, &palette, "RECORDING", ToastKind::Success);
                    }
                } else if self.connected_count() > 0 {
                    state_pill(ui, &palette, "SENSORS READY", ToastKind::Success);
                } else if self.sim_enabled {
                    state_pill(ui, &palette, "SIMULATOR READY", ToastKind::Info);
                } else {
                    state_pill(ui, &palette, "NOT READY", ToastKind::Error);
                }
            });
        });
        ui.add_space(18.0);

        // The virtual world is the centrepiece: always visible, whether setting
        // up a ride or already rolling.
        self.world_panel(ui, &palette);
        ui.add_space(16.0);

        if active {
            self.live_ride(ui, &palette, now);
        } else {
            self.ride_setup(ui, &palette, now, &mut open_devices);
        }

        if open_devices {
            self.page = Page::Devices;
        }
    }

    fn ride_setup(
        &mut self,
        ui: &mut Ui,
        palette: &Palette,
        now: i64,
        open_devices: &mut bool,
    ) {
        let mut target = self.sim_target_power;
        let mut grade = self.sim_grade_pct;
        let mut resistance = self.sim_resistance_pct;
        let mut sim_enabled = self.sim_enabled;
        let mut route_driven = self.route_driven;
        let mut mode_idx = match self.control_mode {
            ControlMode::Erg => 0,
            ControlMode::Grade => 1,
            ControlMode::Resistance => 2,
        };
        let mut chosen_route: Option<(i64, velocity_core::route::Route)> = None;
        let mut start = false;
        let routes = self.routes.clone();
        let current_route_id = self.current_route.as_ref().map(|(id, _)| *id);
        let current_route_name = self
            .current_route
            .as_ref()
            .map(|(_, r)| r.name.clone())
            .unwrap_or_else(|| "Free ride — no route".to_string());
        let connected = self.connected_count();

        ui.columns(2, |cols| {
            titled_card(
                &mut cols[0],
                palette,
                "Ride setup",
                Some("Everything below can be changed while riding"),
                |ui| {
                    ui.label(
                        RichText::new("ROUTE")
                            .font(theme::font_semi(9.0))
                            .color(palette.faint),
                    );
                    egui::ComboBox::from_id_salt("ride_route")
                        .width(ui.available_width())
                        .selected_text(&current_route_name)
                        .show_ui(ui, |ui| {
                            for (id, route) in &routes {
                                let selected = current_route_id == Some(*id);
                                if ui
                                    .selectable_label(
                                        selected,
                                        format!(
                                            "{} · {}",
                                            route.name,
                                            fmt_distance_km(route.stats.distance_m)
                                        ),
                                    )
                                    .clicked()
                                {
                                    chosen_route = Some((*id, route.clone()));
                                }
                            }
                        });
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("TRAINER RESPONSE")
                            .font(theme::font_semi(9.0))
                            .color(palette.faint),
                    );
                    let width = ui.available_width().min(360.0);
                    if let Some(index) = theme::segmented(
                        ui,
                        palette,
                        &["ERG power", "Sim grade", "Resistance"],
                        mode_idx,
                        width,
                    ) {
                        mode_idx = index;
                    }
                    ui.add_space(10.0);
                    match mode_idx {
                        0 => {
                            ui.add(
                                egui::Slider::new(&mut target, 40.0..=600.0)
                                    .suffix(" W")
                                    .text("Target power"),
                            );
                            ui.label(
                                RichText::new(
                                    "ERG holds this power by changing resistance automatically.",
                                )
                                .font(theme::font_body(10.5))
                                .color(palette.faint),
                            );
                        }
                        1 => {
                            ui.add(
                                egui::Slider::new(&mut grade, -12.0..=18.0)
                                    .suffix(" %")
                                    .text("Gradient"),
                            );
                            ui.add(
                                egui::Slider::new(&mut resistance, 0.0..=100.0)
                                    .suffix(" %")
                                    .text("Base resistance"),
                            );
                            ui.label(
                                RichText::new(
                                    "The trainer feels the gradient. Use route-driven mode to follow the course.",
                                )
                                .font(theme::font_body(10.5))
                                .color(palette.faint),
                            );
                        }
                        _ => {
                            ui.add(
                                egui::Slider::new(&mut resistance, 0.0..=100.0)
                                    .suffix(" %")
                                    .text("Resistance"),
                            );
                            ui.label(
                                RichText::new(
                                    "Manual resistance is useful for workouts that are not power-based.",
                                )
                                .font(theme::font_body(10.5))
                                .color(palette.faint),
                            );
                        }
                    }
                    ui.add_space(10.0);
                    if crate::views::accent_switch(
                        ui,
                        palette,
                        &mut route_driven,
                        "Follow route gradient automatically",
                    ) {}
                    ui.add_space(4.0);
                    if crate::views::accent_switch(ui, palette, &mut sim_enabled, "Simulated telemetry")
                    {
                        if sim_enabled {
                            target = target.max(120.0);
                        }
                    }
                    ui.add_space(6.0);
                    if sim_enabled {
                        ui.label(
                            RichText::new(
                                "Simulation is deterministic and clearly marked SIM in every chart and sample.",
                            )
                            .font(theme::font_body(10.5))
                            .color(palette.muted),
                        );
                    }
                    ui.add_space(14.0);
                    if theme::accent_button(ui, Icon::Play, "Start recording", palette).clicked() {
                        start = true;
                    }
                },
            );

            titled_card(
                &mut cols[1],
                palette,
                "Sensors",
                Some("Bluetooth LE · trainer, power, heart rate, speed/cadence"),
                |ui| {
                    if connected == 0 {
                        theme::empty_state(
                            ui,
                            palette,
                            Icon::Bluetooth,
                            "No devices connected",
                            "Power on your KICKR and wake the Garmin strap.",
                        );
                        if theme::ghost_button(
                            ui,
                            Some(Icon::Bluetooth),
                            "Open device manager",
                            palette,
                        )
                        .clicked()
                        {
                            *open_devices = true;
                        }
                    } else {
                        let devices: Vec<_> = self
                            .devices
                            .values()
                            .filter(|d| d.connected)
                            .map(|d| {
                                (
                                    d.name().to_string(),
                                    d.kind(),
                                    [
                                        d.reading(SensorMetric::Power),
                                        d.reading(SensorMetric::HeartRate),
                                        d.reading(SensorMetric::Cadence),
                                        d.reading(SensorMetric::Speed),
                                    ],
                                )
                            })
                            .collect();
                        for (name, kind, readings) in devices {
                            ui.horizontal(|ui| {
                                theme::dot(ui, palette.positive, 7.0);
                                ui.label(
                                    RichText::new(name)
                                        .font(theme::font_semi(13.0))
                                        .color(palette.ink),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let _ = theme::pill(
                                        ui,
                                        kind.short(),
                                        palette.positive,
                                        palette.positive_soft,
                                    );
                                });
                            });
                            let mut shown = 0;
                            for (metric, reading) in [
                                (SensorMetric::Power, readings[0]),
                                (SensorMetric::HeartRate, readings[1]),
                                (SensorMetric::Cadence, readings[2]),
                                (SensorMetric::Speed, readings[3]),
                            ] {
                                let Some(reading) = reading else { continue };
                                shown += 1;
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(metric.label())
                                            .font(theme::font_body(11.0))
                                            .color(palette.muted),
                                    );
                                    ui.with_layout(
                                        Layout::right_to_left(Align::Center),
                                        |ui| {
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} {}",
                                                    sensor_value(metric, reading.value),
                                                    if metric == SensorMetric::Speed {
                                                        "km/h"
                                                    } else {
                                                        metric.unit()
                                                    }
                                                ))
                                                .font(theme::font_mono(11.0))
                                                .color(palette.ink_soft),
                                            );
                                        },
                                    );
                                });
                            }
                            if shown == 0 {
                                ui.label(
                                    RichText::new("Waiting for the first packet…")
                                        .font(theme::font_body(10.5))
                                        .color(palette.faint),
                                );
                            }
                            ui.add_space(6.0);
                            theme::divider(ui, palette);
                            ui.add_space(6.0);
                        }
                        if theme::ghost_button(ui, Some(Icon::Bluetooth), "Manage devices", palette)
                            .clicked()
                        {
                            *open_devices = true;
                        }
                    }
                    ui.add_space(10.0);
                    let (fk, fv, fi, fc) = match self.adapter_available {
                        Some(true) => ("Bluetooth ready", "", Icon::Bluetooth, palette.positive),
                        Some(false) => ("Bluetooth unavailable", "", Icon::Lock, palette.warning),
                        None => ("Starting Bluetooth…", "", Icon::Refresh, palette.warning),
                    };
                    ui.horizontal(|ui| {
                        theme::dot(ui, fc, 6.0);
                        ui.label(
                            RichText::new(format!("{fk} {fv}"))
                                .font(theme::font_body(10.5))
                                .color(palette.muted),
                        );
                        theme::paint_icon(
                            ui,
                            egui::Rect::from_center_size(
                                egui::pos2(ui.cursor().left() + 6.0, ui.cursor().center().y),
                                egui::Vec2::splat(12.0),
                            ),
                            fi,
                            fc,
                        );
                    });
                },
            );
        });

        self.sim_target_power = target;
        self.sim_grade_pct = grade;
        self.sim_resistance_pct = resistance;
        self.sim_enabled = sim_enabled;
        self.route_driven = route_driven;
        self.control_mode = match mode_idx {
            0 => ControlMode::Erg,
            1 => ControlMode::Grade,
            _ => ControlMode::Resistance,
        };
        if let Some((id, route)) = chosen_route {
            self.current_route = Some((id, route));
            self.selected_route = Some(id);
        }
        self.simulator.set_target_power(target);
        self.simulator.set_grade(grade);

        if start {
            self.start_ride(now);
        }
    }

    fn live_ride(&mut self, ui: &mut Ui, palette: &Palette, now: i64) {
        let mut pause = false;
        let mut resume = false;
        let mut stop = false;
        let mut discard = false;
        let mut open_activities = false;

        // Recording bar ------------------------------------------------------
        theme::card(palette).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::Vec2::splat(42.0), egui::Sense::hover());
                let running = self.session.state == SessionState::Running;
                ui.painter().rect_filled(
                    rect,
                    egui::CornerRadius::same(12),
                    if running {
                        palette.accent
                    } else {
                        palette.warning_soft
                    },
                );
                theme::paint_icon(
                    ui,
                    rect.shrink(11.0),
                    if running { Icon::Ride } else { Icon::Pause },
                    if running {
                        palette.accent_ink
                    } else {
                        palette.warning
                    },
                );
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(&self.session.name)
                            .font(theme::font_semi(15.0))
                            .color(palette.ink),
                    );
                    ui.label(
                        RichText::new(format!(
                            "Started {} · {} samples · autosave active",
                            velocity_core::types::local_datetime(
                                self.session.started_at_ms.unwrap_or(now)
                            ),
                            self.session.samples.len()
                        ))
                        .font(theme::font_body(10.5))
                        .color(palette.faint),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if theme::ghost_button(ui, Some(Icon::Trash), "Discard", palette).clicked() {
                        discard = true;
                    }
                    ui.add_space(4.0);
                    if theme::ghost_button(ui, Some(Icon::Stop), "Stop & save", palette).clicked() {
                        stop = true;
                    }
                    ui.add_space(4.0);
                    if self.session.state == SessionState::Paused {
                        if theme::accent_button(ui, Icon::Play, "Resume", palette).clicked() {
                            resume = true;
                        }
                    } else if theme::accent_button(ui, Icon::Pause, "Pause", palette).clicked() {
                        pause = true;
                    }
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(fmt_duration(self.session.live.elapsed_s))
                            .font(theme::font_mono(30.0))
                            .color(palette.ink),
                    );
                });
            });
        });
        ui.add_space(12.0);

        // Live metrics -------------------------------------------------------
        let live = &self.session.live;
        let power = live.current_power(now);
        let hr = live.current_hr(now);
        let cadence = live.current_cadence(now);
        let speed = live.current_speed(now);
        let grade = live.current_grade(now);
        let altitude = live.altitude;
        let sim = live.sim_flags() != 0;
        ui.columns(6, |cols| {
            metric_tile(
                &mut cols[0],
                palette,
                Icon::Bolt,
                "Power",
                &fmt_opt_int(power.map(|r| r.value)),
                "W",
                reading_badge(power, now).as_deref(),
                true,
            );
            metric_tile(
                &mut cols[1],
                palette,
                Icon::Heart,
                "Heart rate",
                &fmt_opt_int(hr.map(|r| r.value)),
                "bpm",
                reading_badge(hr, now).as_deref(),
                false,
            );
            metric_tile(
                &mut cols[2],
                palette,
                Icon::Refresh,
                "Cadence",
                &fmt_opt_int(cadence.map(|r| r.value)),
                "rpm",
                reading_badge(cadence, now).as_deref(),
                false,
            );
            metric_tile(
                &mut cols[3],
                palette,
                Icon::Ride,
                "Speed",
                &fmt_speed(speed),
                "km/h",
                reading_badge(speed, now).as_deref(),
                false,
            );
            metric_tile(
                &mut cols[4],
                palette,
                Icon::Mountain,
                "Gradient",
                &fmt_opt(grade.map(|r| r.value), 1),
                "%",
                reading_badge(grade, now).as_deref(),
                false,
            );
            metric_tile(
                &mut cols[5],
                palette,
                Icon::Map,
                "Distance",
                &format!("{:.2}", self.session.live.distance_m / 1000.0),
                "km",
                if sim { Some("SIM") } else { None },
                false,
            );
        });
        ui.add_space(14.0);

        let avg_power = live.avg_power();
        let max_power = live.max_power();
        let avg_hr = live.avg_heart_rate();
        let avg_speed = live.avg_speed().map(|v| v * 3.6);
        let recent_power = live.recent_avg_power();
        let current_route = self.current_route.clone();
        let progress = self.current_route_progress();
        let samples: Vec<velocity_core::types::Sample> = self.session.samples.clone();
        let route_driven = self.route_driven;
        let control_mode = self.control_mode;
        let target = self.sim_target_power;
        let grade_value = self.sim_grade_pct;
        let resistance = self.sim_resistance_pct;
        let trainer = self
            .devices
            .values()
            .find(|d| d.connected && d.kind() == SensorKind::SmartTrainer)
            .map(|d| (d.name().to_string(), d.control_ready));
        let trainer_status = self.trainer_status.clone();
        let trainer_error = self.trainer_error.clone();
        let _ = altitude;

        ui.columns(2, |cols| {
            titled_card(
                &mut cols[0],
                palette,
                "Live power",
                Some("Recorded at 1 Hz · latest 30-second average"),
                |ui| {
                    let mut points: Vec<[f64; 2]> = samples
                        .iter()
                        .filter_map(|s| s.power_w.map(|p| [s.elapsed_s, p as f64]))
                        .collect();
                    if let Some(p) = power {
                        points.push([self.session.live.elapsed_s, p.value as f64]);
                    }
                    if points.len() >= 2 {
                        let series = [Series::new("Power", points, palette.blue).width(2.0)];
                        charts::line_plot(
                            ui,
                            "live_power",
                            190.0,
                            &series,
                            "elapsed (s)",
                            "watts",
                            palette,
                            false,
                        );
                    } else {
                        charts::chart_placeholder(
                            ui,
                            palette,
                            "Waiting for the first power samples",
                            190.0,
                        );
                    }
                    ui.add_space(8.0);
                    ui.columns(4, |stats| {
                        crate::views::stat_cell(
                            &mut stats[0],
                            palette,
                            "Avg power",
                            &fmt_opt_int(avg_power),
                            "W",
                        );
                        crate::views::stat_cell(
                            &mut stats[1],
                            palette,
                            "30 s",
                            &fmt_opt_int(recent_power),
                            "W",
                        );
                        crate::views::stat_cell(
                            &mut stats[2],
                            palette,
                            "Max",
                            &fmt_opt_int(max_power),
                            "W",
                        );
                        crate::views::stat_cell(
                            &mut stats[3],
                            palette,
                            "Avg HR",
                            &fmt_opt_int(avg_hr),
                            "bpm",
                        );
                    });
                },
            );

            titled_card(
                &mut cols[1],
                palette,
                "Trainer control",
                Some("ERG, gradient or manual resistance"),
                |ui| {
                    if let Some((name, ready)) = &trainer {
                        ui.horizontal(|ui| {
                            theme::dot(
                                ui,
                                if *ready { palette.positive } else { palette.warning },
                                7.0,
                            );
                            ui.label(
                                RichText::new(name)
                                    .font(theme::font_semi(12.5))
                                    .color(palette.ink),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let _ = theme::pill(
                                    ui,
                                    if *ready { "CONTROL READY" } else { "REQUESTING CONTROL" },
                                    if *ready { palette.positive } else { palette.warning },
                                    if *ready {
                                        palette.positive_soft
                                    } else {
                                        palette.warning_soft
                                    },
                                );
                            });
                        });
                    } else {
                        ui.horizontal(|ui| {
                            theme::dot(ui, palette.blue, 7.0);
                            ui.label(
                                RichText::new("Simulated trainer")
                                    .font(theme::font_semi(12.5))
                                    .color(palette.ink),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let _ = theme::pill(
                                    ui,
                                    "SIM",
                                    palette.accent_ink,
                                    palette.accent,
                                );
                            });
                        });
                    }
                    ui.add_space(8.0);
                    let width = ui.available_width().min(340.0);
                    if let Some(index) = theme::segmented(
                        ui,
                        palette,
                        &["ERG", "Grade", "Resistance"],
                        match control_mode {
                            ControlMode::Erg => 0,
                            ControlMode::Grade => 1,
                            ControlMode::Resistance => 2,
                        },
                        width,
                    ) {
                        self.control_mode = match index {
                            0 => ControlMode::Erg,
                            1 => ControlMode::Grade,
                            _ => ControlMode::Resistance,
                        };
                        self.last_sent_grade = None;
                    }
                    ui.add_space(8.0);
                    match self.control_mode {
                        ControlMode::Erg => {
                            let mut value = target;
                            if ui
                                .add(
                                    egui::Slider::new(&mut value, 40.0..=600.0)
                                        .suffix(" W")
                                        .text("Target power"),
                                )
                                .changed()
                            {
                                self.sim_target_power = value;
                                self.simulator.set_target_power(value);
                            }
                        }
                        ControlMode::Grade => {
                            let mut value = grade_value;
                            if ui
                                .add(
                                    egui::Slider::new(&mut value, -12.0..=18.0)
                                        .suffix(" %")
                                        .text("Gradient"),
                                )
                                .changed()
                            {
                                self.sim_grade_pct = value;
                                self.simulator.set_grade(value);
                                self.last_sent_grade = None;
                            }
                        }
                        ControlMode::Resistance => {
                            let mut value = resistance;
                            if ui
                                .add(
                                    egui::Slider::new(&mut value, 0.0..=100.0)
                                        .suffix(" %")
                                        .text("Resistance"),
                                )
                                .changed()
                            {
                                self.sim_resistance_pct = value;
                                self.last_control_ms = 0;
                            }
                        }
                    }
                    if crate::views::accent_switch(
                        ui,
                        palette,
                        &mut self.route_driven,
                        "Follow route gradient",
                    ) {
                        self.last_sent_grade = None;
                    }
                    let _ = route_driven;
                    ui.add_space(6.0);
                    if !trainer_status.is_empty() {
                        ui.label(
                            RichText::new(&trainer_status)
                                .font(theme::font_body(10.5))
                                .color(palette.faint),
                        );
                    }
                    if let Some(err) = &trainer_error {
                        ui.label(
                            RichText::new(err)
                                .font(theme::font_body(10.5))
                                .color(palette.danger),
                        );
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if theme::ghost_button(ui, None, "Apply now", palette).clicked() {
                            self.last_control_ms = 0;
                            self.last_sent_grade = None;
                            self.send_trainer_control(now);
                        }
                        if trainer.is_some()
                            && theme::ghost_button(ui, Some(Icon::Lock), "Request control", palette)
                                .clicked()
                        {
                            if let Some(id) = self.trainer_id() {
                                self.hub_send(HubCommand::RequestControl { id });
                            }
                        }
                    });
                },
            );
        });
        ui.add_space(14.0);

        ui.columns(2, |cols| {
            titled_card(
                &mut cols[0],
                palette,
                "Course",
                Some("Live position and remaining elevation"),
                |ui| {
                    match &current_route {
                        Some((_, route)) => {
                            charts::route_map(ui, palette, route, progress, 214.0);
                            ui.add_space(8.0);
                            charts::elevation_plot(
                                ui,
                                "ride_elevation",
                                palette,
                                route,
                                108.0,
                                progress,
                            );
                            let fraction = progress.unwrap_or(0.0);
                            ui.add_space(6.0);
                            theme::progress_bar(
                                ui,
                                palette,
                                fraction as f32,
                                palette.accent,
                                8.0,
                            );
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                crate::views::stat_cell(
                                    ui,
                                    palette,
                                    "Route",
                                    &format!("{:.0}", fraction * 100.0),
                                    "%",
                                );
                                ui.add_space(16.0);
                                crate::views::stat_cell(
                                    ui,
                                    palette,
                                    "Remaining",
                                    &format!(
                                        "{:.1}",
                                        (route.stats.distance_m
                                            - self.session.live.distance_m)
                                            .max(0.0)
                                            / 1000.0
                                    ),
                                    "km",
                                );
                                ui.add_space(16.0);
                                crate::views::stat_cell(
                                    ui,
                                    palette,
                                    "Grade",
                                    &fmt_opt(grade.map(|r| r.value), 1),
                                    "%",
                                );
                            });
                        }
                        None => {
                            theme::empty_state(
                                ui,
                                palette,
                                Icon::Routes,
                                "Free ride",
                                "Select a GPX route in ride setup to follow a course.",
                            );
                        }
                    }
                },
            );

            titled_card(
                &mut cols[1],
                palette,
                "Recording",
                Some("Honest telemetry, with provenance on every channel"),
                |ui| {
                    ui.columns(2, |row| {
                        crate::views::stat_cell(
                            &mut row[0],
                            palette,
                            "Distance",
                            &format!("{:.2}", self.session.live.distance_m / 1000.0),
                            "km",
                        );
                        crate::views::stat_cell(
                            &mut row[1],
                            palette,
                            "Moving time",
                            &fmt_duration(self.session.live.elapsed_s),
                            "",
                        );
                    });
                    ui.add_space(12.0);
                    ui.columns(2, |row| {
                        crate::views::stat_cell(
                            &mut row[0],
                            palette,
                            "Avg speed",
                            &fmt_opt(avg_speed, 1),
                            "km/h",
                        );
                        crate::views::stat_cell(
                            &mut row[1],
                            palette,
                            "Elevation gain",
                            &format!("{:.0}", self.session.live.elevation_gain_m),
                            "m",
                        );
                    });
                    ui.add_space(10.0);
                    theme::divider(ui, palette);
                    ui.add_space(10.0);
                    let flags = self.session.live.sim_flags();
                    if flags != 0 {
                        ui.horizontal(|ui| {
                            let _ = theme::pill(
                                ui,
                                "SIMULATED TELEMETRY",
                                palette.accent_ink,
                                palette.accent,
                            );
                            ui.label(
                                RichText::new(
                                    "These values come from the deterministic simulator, not sensors.",
                                )
                                .font(theme::font_body(10.5))
                                .color(palette.muted),
                            );
                        });
                    } else {
                        ui.horizontal(|ui| {
                            let _ = theme::pill(
                                ui,
                                "LIVE SENSOR DATA",
                                palette.positive,
                                palette.positive_soft,
                            );
                            ui.label(
                                RichText::new("Values are timestamped at receipt from Bluetooth LE.")
                                    .font(theme::font_body(10.5))
                                    .color(palette.muted),
                            );
                        });
                    }
                    ui.add_space(10.0);
                    let mut channels = Vec::new();
                    for (metric, reading) in [
                        (Metric::Power, power),
                        (Metric::HeartRate, hr),
                        (Metric::Cadence, cadence),
                        (Metric::Speed, speed),
                    ] {
                        channels.push((metric, reading));
                    }
                    for (metric, reading) in channels {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(metric.label())
                                    .font(theme::font_body(11.0))
                                    .color(palette.muted),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if let Some(reading) = reading {
                                    let value = if metric == Metric::Speed {
                                        reading.value * 3.6
                                    } else {
                                        reading.value
                                    };
                                    ui.label(
                                        RichText::new(format!(
                                            "{} · {} ms ago",
                                            fmt_opt(Some(value), 1),
                                            reading.age_ms(now)
                                        ))
                                        .font(theme::font_mono(10.5))
                                        .color(palette.ink_soft),
                                    );
                                    if let Some(badge) = reading.badge(now) {
                                        let _ = theme::pill(
                                            ui,
                                            badge,
                                            if badge == "SIM" {
                                                palette.accent_ink
                                            } else {
                                                palette.warning
                                            },
                                            if badge == "SIM" {
                                                palette.accent
                                            } else {
                                                palette.warning_soft
                                            },
                                        );
                                    }
                                } else {
                                    ui.label(
                                        RichText::new("unavailable")
                                            .font(theme::font_mono(10.5))
                                            .color(palette.faint),
                                    );
                                }
                            });
                        });
                    }
                    ui.add_space(12.0);
                    if self.last_saved_activity.is_some() {
                        if theme::primary_button(ui, "View saved activity", palette).clicked() {
                            open_activities = true;
                        }
                    }
                },
            );
        });

        if pause {
            self.pause_ride(now);
        }
        if resume {
            self.resume_ride(now);
        }
        if stop {
            self.stop_ride(now);
            self.page = Page::Activities;
        }
        if discard {
            self.pending_discard = true;
        }
        if open_activities {
            self.page = Page::Activities;
        }
    }
}

fn sensor_value(metric: SensorMetric, value: f32) -> String {
    if metric == SensorMetric::Speed {
        format!("{:.1}", value * 3.6)
    } else if metric == SensorMetric::Resistance {
        format!("{value:.0}")
    } else {
        format!("{value:.0}")
    }
}
