//! Settings: rider and bike profiles, appearance, local data and about.

use crate::app::VelocityApp;
use crate::theme::{self, Icon, Palette, ThemeMode};
use crate::views::titled_card;
use eframe::egui::{self, Align, Layout, RichText, Ui};
use velocity_core::types::Sex;

impl VelocityApp {
    pub(crate) fn settings(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        let mut save_profile = false;
        let mut backup = false;
        let mut seed_demo = false;
        let mut new_theme: Option<ThemeMode> = None;
        let ctx = ui.ctx().clone();

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("SYSTEM · SETTINGS")
                        .font(theme::font_semi(10.0))
                        .color(palette.faint),
                );
                ui.add_space(3.0);
                ui.label(
                    RichText::new("Settings")
                        .font(theme::font_display(34.0))
                        .color(palette.ink),
                );
                ui.add_space(3.0);
                ui.label(
                    RichText::new(
                        "Your profile drives power zones, TSS and the physics model. Everything is stored locally.",
                    )
                    .font(theme::font_body(13.0))
                    .color(palette.muted),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let _ = theme::pill(
                    ui,
                    &format!("VELOCITY {}", env!("CARGO_PKG_VERSION")),
                    palette.muted,
                    palette.surface_sunken,
                );
            });
        });
        ui.add_space(16.0);

        ui.columns(2, |cols| {
            titled_card(
                &mut cols[0],
                &palette,
                "Rider profile",
                Some("Used for zones, calorie estimates and simulation"),
                |ui| {
                    field(ui, &palette, "Name");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.rider.name)
                            .desired_width(ui.available_width()),
                    );
                    ui.add_space(8.0);
                    ui.columns(3, |c| {
                        field(&mut c[0], &palette, "Weight");
                        c[0].add(
                            egui::DragValue::new(&mut self.rider.weight_kg)
                                .range(30.0..=200.0)
                                .speed(0.5)
                                .suffix(" kg"),
                        );
                        field(&mut c[1], &palette, "Height");
                        c[1].add(
                            egui::DragValue::new(&mut self.rider.height_cm)
                                .range(120.0..=230.0)
                                .speed(0.5)
                                .suffix(" cm"),
                        );
                        field(&mut c[2], &palette, "Age");
                        c[2].add(
                            egui::DragValue::new(&mut self.rider.age)
                                .range(10..=100)
                                .speed(1.0),
                        );
                    });
                    ui.add_space(8.0);
                    field(ui, &palette, "Sex");
                    egui::ComboBox::from_id_salt("sex")
                        .width(ui.available_width())
                        .selected_text(match self.rider.sex {
                            Sex::Unspecified => "Prefer not to say",
                            Sex::Female => "Female",
                            Sex::Male => "Male",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.rider.sex,
                                Sex::Unspecified,
                                "Prefer not to say",
                            );
                            ui.selectable_value(&mut self.rider.sex, Sex::Female, "Female");
                            ui.selectable_value(&mut self.rider.sex, Sex::Male, "Male");
                        });
                    ui.add_space(8.0);
                    ui.columns(3, |c| {
                        field(&mut c[0], &palette, "FTP");
                        c[0].add(
                            egui::DragValue::new(&mut self.rider.ftp_w)
                                .range(50.0..=600.0)
                                .speed(1.0)
                                .suffix(" W"),
                        );
                        field(&mut c[1], &palette, "Max HR");
                        c[1].add(
                            egui::DragValue::new(&mut self.rider.max_hr_bpm)
                                .range(100.0..=230.0)
                                .speed(1.0)
                                .suffix(" bpm"),
                        );
                        field(&mut c[2], &palette, "Resting HR");
                        c[2].add(
                            egui::DragValue::new(&mut self.rider.resting_hr_bpm)
                                .range(30.0..=120.0)
                                .speed(1.0)
                                .suffix(" bpm"),
                        );
                    });
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(format!(
                            "Power zones: {} · {} · {} · {} · {} · {} W",
                            self.rider.power_zone_bounds()[0] as i32,
                            self.rider.power_zone_bounds()[1] as i32,
                            self.rider.power_zone_bounds()[2] as i32,
                            self.rider.power_zone_bounds()[3] as i32,
                            self.rider.power_zone_bounds()[4] as i32,
                            self.rider.power_zone_bounds()[5] as i32
                        ))
                        .font(theme::font_mono(9.5))
                        .color(palette.faint),
                    );
                    ui.add_space(10.0);
                    if theme::primary_button(ui, "Save profile", &palette).clicked() {
                        save_profile = true;
                    }
                },
            );

            titled_card(
                &mut cols[1],
                &palette,
                "Bike & physics",
                Some("Resistance model used by simulation and virtual speed"),
                |ui| {
                    field(ui, &palette, "Bike name");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.bike.name)
                            .desired_width(ui.available_width()),
                    );
                    ui.add_space(8.0);
                    ui.columns(2, |c| {
                        field(&mut c[0], &palette, "Bike mass");
                        c[0].add(
                            egui::DragValue::new(&mut self.bike.mass_kg)
                                .range(4.0..=30.0)
                                .speed(0.1)
                                .suffix(" kg"),
                        );
                        field(&mut c[1], &palette, "Wheel circumference");
                        c[1].add(
                            egui::DragValue::new(&mut self.bike.wheel_circumference_mm)
                                .range(1500.0..=2500.0)
                                .speed(1.0)
                                .suffix(" mm"),
                        );
                    });
                    ui.add_space(8.0);
                    ui.columns(2, |c| {
                        field(&mut c[0], &palette, "Rolling resistance · Crr");
                        c[0].add(
                            egui::DragValue::new(&mut self.bike.crr)
                                .range(0.002..=0.02)
                                .speed(0.0005)
                                .max_decimals(4),
                        );
                        field(&mut c[1], &palette, "Frontal area · CdA");
                        c[1].add(
                            egui::DragValue::new(&mut self.bike.cda_m2)
                                .range(0.20..=0.60)
                                .speed(0.005)
                                .max_decimals(3)
                                .suffix(" m²"),
                        );
                    });
                    ui.add_space(10.0);
                    theme::soft_card(&palette).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new(
                                "Crr 0.004 = smooth asphalt · 0.008 = rough road. CdA 0.36 ≈ hoods, 0.25 ≈ drops. The same physics drives virtual speed and simulated trainer feel.",
                            )
                            .font(theme::font_body(10.0))
                            .color(palette.muted),
                        );
                    });
                    ui.add_space(10.0);
                    if theme::ghost_button(ui, Some(Icon::Check), "Apply to simulator", &palette)
                        .clicked()
                    {
                        save_profile = true;
                    }
                },
            );
        });
        ui.add_space(14.0);

        ui.columns(2, |cols| {
            titled_card(
                &mut cols[0],
                &palette,
                "Appearance",
                Some("Light is the default. Both themes are tuned by hand."),
                |ui| {
                    let current = if self.theme.is_dark() { 1 } else { 0 };
                    if let Some(index) = theme::segmented(
                        ui,
                        &palette,
                        &["Light", "Dark"],
                        current,
                        260.0,
                    ) {
                        new_theme = Some(if index == 1 {
                            ThemeMode::Dark
                        } else {
                            ThemeMode::Light
                        });
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        for (name, mode) in [
                            ("Light", ThemeMode::Light),
                            ("Dark", ThemeMode::Dark),
                        ] {
                            let p = Palette::of(mode);
                            egui::Frame::new()
                                .fill(p.bg)
                                .stroke(egui::Stroke::new(
                                    1.0,
                                    if self.theme == mode {
                                        p.accent
                                    } else {
                                        p.line
                                    },
                                ))
                                .corner_radius(egui::CornerRadius::same(12))
                                .inner_margin(egui::Margin::symmetric(14, 10))
                                .show(ui, |ui| {
                                    ui.set_min_width(110.0);
                                    ui.label(
                                        RichText::new(name)
                                            .font(theme::font_semi(11.0))
                                            .color(p.ink),
                                    );
                                    ui.add_space(4.0);
                                    ui.horizontal(|ui| {
                                        theme::dot(ui, p.accent, 9.0);
                                        theme::dot(ui, p.blue, 9.0);
                                        theme::dot(ui, p.positive, 9.0);
                                    });
                                });
                            ui.add_space(8.0);
                        }
                    });
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(
                            "VELOCITY follows the system theme when first launched and remembers your choice.",
                        )
                        .font(theme::font_body(10.0))
                        .color(palette.faint),
                    );
                },
            );

            titled_card(
                &mut cols[1],
                &palette,
                "Local data",
                Some("SQLite · no cloud · no telemetry"),
                |ui| {
                    crate::views::info_row(
                        ui,
                        &palette,
                        "Database",
                        &self.db_path.display().to_string(),
                    );
                    crate::views::info_row(
                        ui,
                        &palette,
                        "Size",
                        &format!(
                            "{:.1} MB",
                            self.store.database_size_bytes() as f64 / (1024.0 * 1024.0)
                        ),
                    );
                    crate::views::info_row(
                        ui,
                        &palette,
                        "Exports",
                        &self.export_dir.display().to_string(),
                    );
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if theme::ghost_button(ui, Some(Icon::Download), "Backup database", &palette)
                            .clicked()
                        {
                            backup = true;
                        }
                        if theme::ghost_button(ui, Some(Icon::Plus), "Add demo rides", &palette)
                            .clicked()
                        {
                            seed_demo = true;
                        }
                    });
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(
                            "Exports are written to your Documents folder. Demo data is only added when the activity list is empty.",
                        )
                        .font(theme::font_body(10.0))
                        .color(palette.faint),
                    );
                },
            );
        });
        ui.add_space(14.0);

        titled_card(
            ui,
            &palette,
            "About VELOCITY",
            Some("A local-first cycling computer, trainer and training log"),
            |ui| {
                ui.columns(2, |cols| {
                    ui_about_column(
                        &mut cols[0],
                        &palette,
                        "OPEN SOURCE · MIT",
                        &[
                            ("Version", env!("CARGO_PKG_VERSION")),
                            ("License", "MIT"),
                            ("Stack", "Rust · egui · SQLite · btleplug"),
                            ("Graphics", "OpenGL GLOW · Intel HD 520 friendly"),
                        ],
                    );
                    ui_about_column(
                        &mut cols[1],
                        &palette,
                        "HONEST BY DESIGN",
                        &[
                            ("Simulated data", "Always marked SIM"),
                            ("Telemetry", "None"),
                            ("Accounts", "Not required"),
                            ("Data location", "This computer only"),
                        ],
                    );
                });
                ui.add_space(10.0);
                theme::divider(ui, &palette);
                ui.add_space(10.0);
                ui.label(
                    RichText::new(
                        "VELOCITY is an independent project. It is not affiliated with Wahoo, Garmin, Strava or MyWhoosh. Device names are used only to describe compatibility. Route maps are drawn from the imported GPX geometry; no external tile service is contacted during normal use.",
                    )
                    .font(theme::font_body(10.0))
                    .color(palette.muted),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Built to be owned by the rider.")
                            .font(theme::font_semi(10.5))
                            .color(palette.ink_soft),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let _ = theme::pill(
                            ui,
                            "NO ROOT REQUIRED",
                            palette.positive,
                            palette.positive_soft,
                        );
                    });
                });
            },
        );

        if let Some(mode) = new_theme {
            self.set_theme(&ctx, mode);
        }
        if save_profile {
            self.apply_profile();
        }
        if backup {
            self.backup_database();
        }
        if seed_demo {
            self.seed_demo_data();
        }
    }
}

fn field(ui: &mut Ui, palette: &Palette, label: &str) {
    ui.label(
        RichText::new(label.to_ascii_uppercase())
            .font(theme::font_semi(8.5))
            .color(palette.faint),
    );
}

fn ui_about_column(
    ui: &mut Ui,
    palette: &Palette,
    heading: &str,
    rows: &[(&str, &str)],
) {
    ui.label(
        RichText::new(heading)
            .font(theme::font_semi(9.5))
            .color(palette.faint),
    );
    ui.add_space(4.0);
    for (label, value) in rows {
        crate::views::info_row(ui, palette, label, value);
    }
}
