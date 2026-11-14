//! Device manager: Bluetooth discovery, trainer control and simulator controls.

use crate::app::{ControlMode, VelocityApp};
use crate::theme::{self, Icon, Palette};
use crate::views::{fmt_opt_int, reading_badge, titled_card};
use eframe::egui::{self, Align, Layout, RichText, Ui};
use velocity_core::types::now_ms;
use velocity_sensors::{HubCommand, SensorKind, SensorMetric, SensorReading};

impl VelocityApp {
    pub(crate) fn devices(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        let now = now_ms();
        let mut toggle_scan = false;
        let mut connect: Option<String> = None;
        let mut disconnect: Option<String> = None;
        let mut apply_trainer = false;
        let mut request_control = false;
        let mut sim_target = self.sim_target_power;
        let mut sim_grade = self.sim_grade_pct;
        let mut sim_resistance = self.sim_resistance_pct;
        let mut sim_enabled = self.sim_enabled;
        let mut mode = self.control_mode;
        let mut route_driven = self.route_driven;

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("SYSTEM · DEVICES")
                        .font(theme::font_semi(10.0))
                        .color(palette.faint),
                );
                ui.add_space(3.0);
                ui.label(
                    RichText::new("Devices & trainer")
                        .font(theme::font_display(34.0))
                        .color(palette.ink),
                );
                ui.add_space(3.0);
                ui.label(
                    RichText::new(
                        "Bluetooth LE sensors connect directly — no dongle, no vendor cloud, no account.",
                    )
                    .font(theme::font_body(13.0))
                    .color(palette.muted),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let count = self.connected_count();
                let _ = theme::pill(
                    ui,
                    &format!("{count} CONNECTED"),
                    if count > 0 {
                        palette.positive
                    } else {
                        palette.muted
                    },
                    if count > 0 {
                        palette.positive_soft
                    } else {
                        palette.surface_sunken
                    },
                );
            });
        });
        ui.add_space(16.0);

        // Adapter status -----------------------------------------------------
        theme::card(&palette).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::Vec2::splat(42.0), egui::Sense::hover());
                let color = match self.adapter_available {
                    Some(true) => palette.positive_soft,
                    Some(false) => palette.warning_soft,
                    None => palette.blue_soft,
                };
                ui.painter()
                    .rect_filled(rect, egui::CornerRadius::same(12), color);
                theme::paint_icon(
                    ui,
                    rect.shrink(11.0),
                    if self.offline {
                        Icon::Lock
                    } else {
                        Icon::Bluetooth
                    },
                    match self.adapter_available {
                        Some(true) => palette.positive,
                        Some(false) => palette.warning,
                        None => palette.blue,
                    },
                );
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(if self.offline {
                            "Bluetooth disabled for this run"
                        } else if self.scanning {
                            "Scanning for Bluetooth LE devices…"
                        } else {
                            "Bluetooth LE"
                        })
                        .font(theme::font_semi(14.0))
                        .color(palette.ink),
                    );
                    ui.label(
                        RichText::new(&self.adapter_message)
                            .font(theme::font_body(11.0))
                            .color(palette.muted),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if !self.offline {
                        if self.scanning {
                            if theme::ghost_button(
                                ui,
                                Some(Icon::Refresh),
                                "Stop scan",
                                &palette,
                            )
                            .clicked()
                            {
                                toggle_scan = true;
                            }
                        } else if theme::accent_button(
                            ui,
                            Icon::Search,
                            "Scan for devices",
                            &palette,
                        )
                        .clicked()
                        {
                            toggle_scan = true;
                        }
                    }
                    if self.scanning {
                        ui.add_space(8.0);
                        let _ = theme::pill(
                            ui,
                            "SCANNING",
                            palette.blue,
                            palette.blue_soft,
                        );
                    }
                });
            });
        });
        ui.add_space(14.0);

        let devices: Vec<DeviceRow> = self
            .devices
            .values()
            .map(|d| DeviceRow {
                id: d.device.id.clone(),
                name: d.device.name.clone(),
                kind: d.kind(),
                rssi: d.device.rssi,
                services: d.device.services.len(),
                connected: d.connected,
                control_ready: d.control_ready,
                readings: [
                    d.reading(SensorMetric::Power),
                    d.reading(SensorMetric::HeartRate),
                    d.reading(SensorMetric::Cadence),
                    d.reading(SensorMetric::Speed),
                    d.reading(SensorMetric::Resistance),
                ],
                last_message: d.last_message.clone(),
            })
            .collect();
        let trainer = devices
            .iter()
            .find(|d| d.connected && d.kind == SensorKind::SmartTrainer)
            .cloned();
        let trainer_status = self.trainer_status.clone();
        let trainer_error = self.trainer_error.clone();

        ui.columns(2, |cols| {
            titled_card(
                &mut cols[0],
                &palette,
                "Discovered devices",
                Some("Nearby Bluetooth LE trainers and sensors"),
                |ui| {
                    if devices.is_empty() {
                        theme::empty_state(
                            ui,
                            &palette,
                            Icon::Bluetooth,
                            if self.scanning {
                                "Looking for devices…"
                            } else {
                                "No devices discovered"
                            },
                            "Put the KICKR in pairing mode and wake the heart-rate strap.",
                        );
                    } else {
                        for device in &devices {
                            theme::soft_card(&palette).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::Vec2::splat(36.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(
                                        rect,
                                        egui::CornerRadius::same(10),
                                        if device.connected {
                                            palette.positive_soft
                                        } else {
                                            palette.surface
                                        },
                                    );
                                    theme::paint_icon(
                                        ui,
                                        rect.shrink(9.0),
                                        match device.kind {
                                            SensorKind::HeartRate => Icon::Heart,
                                            SensorKind::SmartTrainer => Icon::Bolt,
                                            _ => Icon::Bluetooth,
                                        },
                                        if device.connected {
                                            palette.positive
                                        } else {
                                            palette.muted
                                        },
                                    );
                                    ui.add_space(4.0);
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new(&device.name)
                                                    .font(theme::font_semi(13.0))
                                                    .color(palette.ink),
                                            );
                                            let _ = theme::pill(
                                                ui,
                                                device.kind.short(),
                                                palette.muted,
                                                palette.surface_sunken,
                                            );
                                            if device.connected && device.control_ready {
                                                let _ = theme::pill(
                                                    ui,
                                                    "CONTROL",
                                                    palette.positive,
                                                    palette.positive_soft,
                                                );
                                            }
                                        });
                                        ui.label(
                                            RichText::new(format!(
                                                "{} · {} services{}{}",
                                                device.kind.label(),
                                                device.services,
                                                device
                                                    .rssi
                                                    .map(|r| format!(" · {r} dBm"))
                                                    .unwrap_or_default(),
                                                if device.last_message.is_empty() {
                                                    String::new()
                                                } else {
                                                    format!(" · {}", device.last_message)
                                                }
                                            ))
                                            .font(theme::font_body(9.5))
                                            .color(palette.faint),
                                        );
                                    });
                                    ui.with_layout(
                                        Layout::right_to_left(Align::Center),
                                        |ui| {
                                            if device.connected {
                                                if theme::ghost_button(
                                                    ui,
                                                    None,
                                                    "Disconnect",
                                                    &palette,
                                                )
                                                .clicked()
                                                {
                                                    disconnect = Some(device.id.clone());
                                                }
                                                let _ = theme::pill(
                                                    ui,
                                                    "CONNECTED",
                                                    palette.positive,
                                                    palette.positive_soft,
                                                );
                                            } else if theme::primary_button(
                                                ui,
                                                "Connect",
                                                &palette,
                                            )
                                            .clicked()
                                            {
                                                connect = Some(device.id.clone());
                                            }
                                        },
                                    );
                                });
                            });
                            ui.add_space(6.0);
                        }
                    }
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "VELOCITY talks standard FTMS and Cycling Power to trainers, and the standard Heart Rate service to straps. Wahoo's proprietary protocol is used only as a fallback.",
                        )
                        .font(theme::font_body(9.5))
                        .color(palette.faint),
                    );
                },
            );

            let mut right = |ui: &mut Ui| {
                titled_card(
                    ui,
                    &palette,
                    "Trainer control",
                    Some("Changes take effect immediately when connected"),
                    |ui| {
                        match &trainer {
                            Some(device) => {
                                ui.horizontal(|ui| {
                                    theme::dot(ui, palette.positive, 7.0);
                                    ui.label(
                                        RichText::new(&device.name)
                                            .font(theme::font_semi(12.5))
                                            .color(palette.ink),
                                    );
                                    ui.with_layout(
                                        Layout::right_to_left(Align::Center),
                                        |ui| {
                                            let _ = theme::pill(
                                                ui,
                                                if device.control_ready {
                                                    "CONTROL READY"
                                                } else {
                                                    "NO CONTROL YET"
                                                },
                                                if device.control_ready {
                                                    palette.positive
                                                } else {
                                                    palette.warning
                                                },
                                                if device.control_ready {
                                                    palette.positive_soft
                                                } else {
                                                    palette.warning_soft
                                                },
                                            );
                                        },
                                    );
                                });
                            }
                            None => {
                                ui.horizontal(|ui| {
                                    theme::dot(ui, palette.blue, 7.0);
                                    ui.label(
                                        RichText::new("Simulated trainer")
                                            .font(theme::font_semi(12.5))
                                            .color(palette.ink),
                                    );
                                    ui.with_layout(
                                        Layout::right_to_left(Align::Center),
                                        |ui| {
                                            let _ = theme::pill(
                                                ui,
                                                "SIMULATOR",
                                                palette.accent_ink,
                                                palette.accent,
                                            );
                                        },
                                    );
                                });
                            }
                        }
                        ui.add_space(10.0);
                        let width = ui.available_width().min(360.0);
                        if let Some(index) = theme::segmented(
                            ui,
                            &palette,
                            &["ERG", "Grade", "Resistance"],
                            match mode {
                                ControlMode::Erg => 0,
                                ControlMode::Grade => 1,
                                ControlMode::Resistance => 2,
                            },
                            width,
                        ) {
                            mode = match index {
                                0 => ControlMode::Erg,
                                1 => ControlMode::Grade,
                                _ => ControlMode::Resistance,
                            };
                        }
                        ui.add_space(8.0);
                        match mode {
                            ControlMode::Erg => {
                                ui.add(
                                    egui::Slider::new(&mut sim_target, 40.0..=600.0)
                                        .suffix(" W")
                                        .text("Target power"),
                                );
                            }
                            ControlMode::Grade => {
                                ui.add(
                                    egui::Slider::new(&mut sim_grade, -12.0..=18.0)
                                        .suffix(" %")
                                        .text("Gradient"),
                                );
                                ui.add(
                                    egui::Slider::new(&mut sim_resistance, 0.0..=100.0)
                                        .suffix(" %")
                                        .text("Base resistance"),
                                );
                            }
                            ControlMode::Resistance => {
                                ui.add(
                                    egui::Slider::new(&mut sim_resistance, 0.0..=100.0)
                                        .suffix(" %")
                                        .text("Resistance"),
                                );
                            }
                        }
                        let _ = crate::views::accent_switch(
                            ui,
                            &palette,
                            &mut route_driven,
                            "Follow route gradient",
                        );
                        let _ =
                            crate::views::accent_switch(ui, &palette, &mut sim_enabled, "Simulated telemetry");
                        if !trainer_status.is_empty() {
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(&trainer_status)
                                    .font(theme::font_body(10.0))
                                    .color(palette.faint),
                            );
                        }
                        if let Some(err) = &trainer_error {
                            ui.label(
                                RichText::new(err)
                                    .font(theme::font_body(10.0))
                                    .color(palette.danger),
                            );
                        }
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if theme::accent_button(ui, Icon::Bolt, "Apply now", &palette).clicked() {
                                apply_trainer = true;
                            }
                            if trainer.is_some()
                                && theme::ghost_button(
                                    ui,
                                    Some(Icon::Lock),
                                    "Request control",
                                    &palette,
                                )
                                .clicked()
                            {
                                request_control = true;
                            }
                        });
                        if trainer.is_none() {
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(
                                    "Without a connected trainer these controls drive the deterministic simulator. Values stay labelled SIM.",
                                )
                                .font(theme::font_body(9.5))
                                .color(palette.faint),
                            );
                        }
                    },
                );
                ui.add_space(14.0);
                titled_card(
                    ui,
                    &palette,
                    "Simulator",
                    Some("Deterministic, plausible, always labelled"),
                    |ui| {
                        ui.horizontal(|ui| {
                            let _ = theme::pill(
                                ui,
                                if sim_enabled { "ON" } else { "OFF" },
                                if sim_enabled {
                                    palette.accent_ink
                                } else {
                                    palette.muted
                                },
                                if sim_enabled {
                                    palette.accent
                                } else {
                                    palette.surface_sunken
                                },
                            );
                            ui.label(
                                RichText::new(format!(
                                    "Target {} W · grade {:.1}%",
                                    sim_target, sim_grade
                                ))
                                .font(theme::font_mono(10.5))
                                .color(palette.muted),
                            );
                        });
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(
                                "The simulator reuses the same physics model as virtual speed, so power, speed and heart rate respond plausibly. Every channel is tagged as simulated in the database.",
                            )
                            .font(theme::font_body(10.0))
                            .color(palette.faint),
                        );
                    },
                );
            };
            right(&mut cols[1]);
        });
        ui.add_space(14.0);

        // Connected sensor readout ------------------------------------------
        titled_card(
            ui,
            &palette,
            "Live sensor readout",
            Some("Timestamped at receipt; stale values are never silently recorded"),
            |ui| {
                let connected: Vec<&DeviceRow> =
                    devices.iter().filter(|d| d.connected).collect();
                if connected.is_empty() {
                    ui.label(
                        RichText::new(
                            "No connected sensors. The simulator is available from the trainer control panel.",
                        )
                        .font(theme::font_body(11.0))
                        .color(palette.muted),
                    );
                } else {
                    for device in connected {
                        ui.horizontal(|ui| {
                            theme::dot(ui, palette.positive, 6.0);
                            ui.label(
                                RichText::new(&device.name)
                                    .font(theme::font_semi(12.0))
                                    .color(palette.ink),
                            );
                            let _ = theme::pill(
                                ui,
                                device.kind.short(),
                                palette.muted,
                                palette.surface_sunken,
                            );
                        });
                        ui.add_space(4.0);
                        let mut any = false;
                        for (metric, reading) in [
                            (SensorMetric::Power, device.readings[0]),
                            (SensorMetric::HeartRate, device.readings[1]),
                            (SensorMetric::Cadence, device.readings[2]),
                            (SensorMetric::Speed, device.readings[3]),
                            (SensorMetric::Resistance, device.readings[4]),
                        ] {
                            let Some(reading) = reading else { continue };
                            any = true;
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
                                                "{} {} · {} ms ago",
                                                device_value(metric, reading),
                                                metric.unit(),
                                                reading.age_ms(now)
                                            ))
                                            .font(theme::font_mono(10.5))
                                            .color(palette.ink_soft),
                                        );
                                        if let Some(badge) =
                                            reading_badge(Some(reading.to_reading()), now)
                                        {
                                            let is_sim = badge == "SIM";
                                            let is_stale = badge == "STALE";
                                            let _ = theme::pill(
                                                ui,
                                                &badge,
                                                if is_stale {
                                                    palette.warning
                                                } else if is_sim {
                                                    palette.accent_ink
                                                } else {
                                                    palette.muted
                                                },
                                                if is_stale {
                                                    palette.warning_soft
                                                } else if is_sim {
                                                    palette.accent
                                                } else {
                                                    palette.surface_sunken
                                                },
                                            );
                                        }
                                    },
                                );
                            });
                        }
                        if !any {
                            ui.label(
                                RichText::new("Waiting for the first packet…")
                                    .font(theme::font_body(10.0))
                                    .color(palette.faint),
                            );
                        }
                        ui.add_space(6.0);
                    }
                }
            },
        );
        ui.add_space(14.0);

        // Hardware notes ------------------------------------------------------
        ui.columns(2, |cols| {
            titled_card(
                &mut cols[0],
                &palette,
                "Wahoo KICKR",
                None,
                |ui| {
                    for (step, text) in [
                        ("1", "Power the KICKR and wake it by turning the flywheel."),
                        ("2", "Make sure no phone or head unit is holding the trainer."),
                        ("3", "Click Scan, select the trainer and connect."),
                        ("4", "VELOCITY requests FTMS control automatically and confirms it in the trainer panel."),
                    ] {
                        ui.horizontal(|ui| {
                            let _ = theme::pill(
                                ui,
                                step,
                                palette.blue,
                                palette.blue_soft,
                            );
                            ui.label(
                                RichText::new(text)
                                    .font(theme::font_body(10.5))
                                    .color(palette.muted),
                            );
                        });
                        ui.add_space(3.0);
                    }
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "Supported: FTMS ERG, simulated grade, resistance, Cycling Power. Wahoo proprietary control is the fallback if FTMS is unavailable.",
                        )
                        .font(theme::font_body(9.5))
                        .color(palette.faint),
                    );
                },
            );
            titled_card(
                &mut cols[1],
                &palette,
                "Garmin Forerunner 245",
                None,
                |ui| {
                    for (step, text) in [
                        ("1", "On the watch: Settings → Sensors & Accessories → Broadcast Heart Rate."),
                        ("2", "Start broadcasting and keep the watch on the wrist."),
                        ("3", "Click Scan in VELOCITY and connect the Forerunner."),
                        ("4", "Heart rate appears at roughly 1 Hz and is recorded with provenance."),
                    ] {
                        ui.horizontal(|ui| {
                            let _ = theme::pill(
                                ui,
                                step,
                                palette.blue,
                                palette.blue_soft,
                            );
                            ui.label(
                                RichText::new(text)
                                    .font(theme::font_body(10.5))
                                    .color(palette.muted),
                            );
                        });
                        ui.add_space(3.0);
                    }
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "The Forerunner 245 broadcasts the standard BLE Heart Rate service. No ANT+ stick is required.",
                        )
                        .font(theme::font_body(9.5))
                        .color(palette.faint),
                    );
                },
            );
        });

        // Apply local edits ------------------------------------------------
        self.sim_target_power = sim_target;
        self.sim_grade_pct = sim_grade;
        self.sim_resistance_pct = sim_resistance;
        self.sim_enabled = sim_enabled;
        self.control_mode = mode;
        self.route_driven = route_driven;
        self.simulator.set_target_power(sim_target);
        self.simulator.set_grade(sim_grade);

        if toggle_scan {
            if self.scanning {
                self.stop_scan();
            } else {
                self.start_scan();
            }
        }
        if let Some(id) = connect {
            self.connect_device(&id);
        }
        if let Some(id) = disconnect {
            self.disconnect_device(&id);
        }
        if apply_trainer {
            self.last_control_ms = 0;
            self.last_sent_grade = None;
            self.send_trainer_control(now);
        }
        if request_control {
            if let Some(id) = self.trainer_id() {
                self.hub_send(HubCommand::RequestControl { id });
                self.toast("Requested trainer control.", crate::app::ToastKind::Info);
            }
        }
    }
}

#[derive(Clone)]
struct DeviceRow {
    id: String,
    name: String,
    kind: SensorKind,
    rssi: Option<i16>,
    services: usize,
    connected: bool,
    control_ready: bool,
    readings: [Option<SensorReading>; 5],
    last_message: String,
}

fn device_value(metric: SensorMetric, reading: SensorReading) -> String {
    match metric {
        SensorMetric::Speed => format!("{:.1}", reading.value * 3.6),
        SensorMetric::Power | SensorMetric::HeartRate | SensorMetric::Cadence => {
            fmt_opt_int(Some(reading.value))
        }
        SensorMetric::Resistance => format!("{:.0}", reading.value),
    }
}
