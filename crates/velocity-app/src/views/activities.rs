//! Activity history: searchable list, detail analytics and format export.

use crate::app::VelocityApp;
use crate::charts::{self, Series};
use crate::theme::{self, Icon, Palette};
use crate::views::{fmt_opt, fmt_opt_int, titled_card};
use eframe::egui::{self, Align, Layout, RichText, Ui};
use velocity_core::analytics;
use velocity_core::formats::ExportFormat;
use velocity_core::types::{
    Activity, Sample, fmt_distance_km, fmt_duration, local_date, local_datetime,
};

impl VelocityApp {
    pub(crate) fn activities(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        let mut open: Option<i64> = None;
        let mut import = false;
        let mut delete: Option<i64> = None;
        let mut export: Option<(i64, ExportFormat)> = None;
        let mut save_meta = false;

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("TRAIN · ACTIVITIES")
                        .font(theme::font_semi(10.0))
                        .color(palette.faint),
                );
                ui.add_space(3.0);
                ui.label(
                    RichText::new("Activity history")
                        .font(theme::font_display(34.0))
                        .color(palette.ink),
                );
                ui.add_space(3.0);
                ui.label(
                    RichText::new(
                        "Every ride, recorded locally with full-resolution samples. Import and export FIT, GPX and TCX.",
                    )
                    .font(theme::font_body(13.0))
                    .color(palette.muted),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let _ = theme::pill(
                    ui,
                    &format!("{} ACTIVITIES", self.activity_count),
                    palette.muted,
                    palette.surface_sunken,
                );
            });
        });
        ui.add_space(16.0);

        // Import strip -------------------------------------------------------
        theme::soft_card(&palette).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::Vec2::splat(30.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, egui::CornerRadius::same(9), palette.surface);
                theme::paint_icon(ui, rect.shrink(7.0), Icon::Download, palette.muted);
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("Import an activity")
                            .font(theme::font_semi(12.5))
                            .color(palette.ink),
                    );
                    ui.label(
                        RichText::new("FIT, GPX or TCX · path on this computer")
                            .font(theme::font_body(10.5))
                            .color(palette.faint),
                    );
                });
                ui.add_space(10.0);
                ui.add_sized(
                    [ui.available_width().min(430.0) - 110.0, 32.0],
                    egui::TextEdit::singleline(&mut self.activity_import_path)
                        .hint_text("/home/you/Downloads/ride.fit"),
                );
                if theme::ghost_button(ui, Some(Icon::Upload), "Import", &palette).clicked() {
                    import = true;
                }
            });
        });
        ui.add_space(14.0);

        let selected_id = self.selected_activity.as_ref().map(|a| a.id);
        let query = self.activity_search.to_ascii_lowercase();
        let activities: Vec<Activity> = self
            .activities
            .iter()
            .filter(|a| {
                query.is_empty()
                    || a.name.to_ascii_lowercase().contains(&query)
                    || a.source.to_ascii_lowercase().contains(&query)
                    || local_date(a.started_at_ms).contains(&query)
                    || a.sport.label().to_ascii_lowercase().contains(&query)
            })
            .cloned()
            .collect();
        let activity = self.selected_activity.clone();
        let samples = self.selected_samples.clone();
        let ftp = self.rider.ftp_w;

        ui.columns(2, |cols| {
            titled_card(
                &mut cols[0],
                &palette,
                "All activities",
                Some("Click a row to open its analysis"),
                |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.activity_search)
                            .hint_text("Search name, date, sport or source…")
                            .desired_width(ui.available_width()),
                    );
                    ui.add_space(8.0);
                    if activities.is_empty() {
                        theme::empty_state(
                            ui,
                            &palette,
                            Icon::Activities,
                            if self.activities.is_empty() {
                                "No activities yet"
                            } else {
                                "No matching activities"
                            },
                            "Record a ride or import a FIT/GPX/TCX file.",
                        );
                    } else {
                        for activity in &activities {
                            let selected = selected_id == Some(activity.id);
                            let response = theme::list_row(
                                ui,
                                &palette,
                                selected,
                                54.0,
                                |ui, palette| {
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(
                                                RichText::new(
                                                    local_date(activity.started_at_ms)
                                                        .get(5..)
                                                        .unwrap_or("—"),
                                                )
                                                .font(theme::font_mono(10.0))
                                                .color(palette.faint),
                                            );
                                            ui.label(
                                                RichText::new(
                                                    activity
                                                        .summary
                                                        .tss
                                                        .map(|t| format!("{t:.0}"))
                                                        .unwrap_or_else(|| "—".into()),
                                                )
                                                .font(theme::font_mono(15.0))
                                                .color(palette.ink_soft),
                                            );
                                        });
                                        ui.add_space(8.0);
                                        ui.vertical(|ui| {
                                            ui.label(
                                                RichText::new(&activity.name)
                                                    .font(theme::font_semi(12.5))
                                                    .color(palette.ink),
                                            );
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} · {}",
                                                    activity.sport.label(),
                                                    activity.source
                                                ))
                                                .font(theme::font_body(9.5))
                                                .color(palette.faint),
                                            );
                                        });
                                        ui.with_layout(
                                            Layout::right_to_left(Align::Center),
                                            |ui| {
                                                ui.label(
                                                    RichText::new(fmt_distance_km(
                                                        activity.summary.distance_m,
                                                    ))
                                                    .font(theme::font_mono(11.0))
                                                    .color(palette.ink_soft),
                                                );
                                                ui.add_space(8.0);
                                                ui.label(
                                                    RichText::new(fmt_duration(
                                                        activity.summary.moving_s.max(
                                                            activity.summary.duration_s,
                                                        ),
                                                    ))
                                                    .font(theme::font_mono(10.5))
                                                    .color(palette.muted),
                                                );
                                            },
                                        );
                                    });
                                },
                            );
                            if response.clicked() {
                                open = Some(activity.id);
                            }
                            ui.add_space(1.0);
                        }
                    }
                },
            );

            titled_card(
                &mut cols[1],
                &palette,
                "Activity detail",
                Some("Power, heart rate, zones and best efforts"),
                |ui| match &activity {
                    None => {
                        theme::empty_state(
                            ui,
                            &palette,
                            Icon::TrendUp,
                            "Select an activity",
                            "Your analysis will appear here.",
                        );
                    }
                    Some(activity) => {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(&activity.name)
                                        .font(theme::font_display(24.0))
                                        .color(palette.ink),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "{} · {} · {}",
                                        local_datetime(activity.started_at_ms),
                                        activity.sport.label(),
                                        activity.source
                                    ))
                                    .font(theme::font_body(10.5))
                                    .color(palette.muted),
                                );
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let _ = theme::pill(
                                    ui,
                                    if activity.is_finished() {
                                        "SAVED"
                                    } else {
                                        "IN PROGRESS"
                                    },
                                    if activity.is_finished() {
                                        palette.positive
                                    } else {
                                        palette.warning
                                    },
                                    if activity.is_finished() {
                                        palette.positive_soft
                                    } else {
                                        palette.warning_soft
                                    },
                                );
                            });
                        });
                        ui.add_space(10.0);
                        ui.columns(4, |stats| {
                            crate::views::stat_cell(
                                &mut stats[0],
                                &palette,
                                "Distance",
                                &format!("{:.1}", activity.summary.distance_m / 1000.0),
                                "km",
                            );
                            crate::views::stat_cell(
                                &mut stats[1],
                                &palette,
                                "Moving",
                                &fmt_duration(activity.summary.moving_s),
                                "",
                            );
                            crate::views::stat_cell(
                                &mut stats[2],
                                &palette,
                                "Avg power",
                                &fmt_opt_int(activity.summary.avg_power_w),
                                "W",
                            );
                            crate::views::stat_cell(
                                &mut stats[3],
                                &palette,
                                "TSS",
                                &fmt_opt_int(activity.summary.tss),
                                "",
                            );
                        });
                        ui.add_space(6.0);
                        ui.columns(4, |stats| {
                            crate::views::stat_cell(
                                &mut stats[0],
                                &palette,
                                "Avg HR",
                                &fmt_opt_int(activity.summary.avg_heart_rate_bpm),
                                "bpm",
                            );
                            crate::views::stat_cell(
                                &mut stats[1],
                                &palette,
                                "Climbing",
                                &format!("{:.0}", activity.summary.elevation_gain_m),
                                "m",
                            );
                            crate::views::stat_cell(
                                &mut stats[2],
                                &palette,
                                "Norm. power",
                                &fmt_opt_int(activity.summary.normalized_power_w),
                                "W",
                            );
                            crate::views::stat_cell(
                                &mut stats[3],
                                &palette,
                                "Intensity",
                                &fmt_opt(activity.summary.intensity_factor, 2),
                                "IF",
                            );
                        });
                        ui.add_space(10.0);
                        theme::divider(ui, &palette);
                        ui.add_space(8.0);

                        let power_points: Vec<[f64; 2]> = samples
                            .iter()
                            .filter_map(|s| {
                                s.power_w.map(|p| [s.elapsed_s / 60.0, p as f64])
                            })
                            .collect();
                        if power_points.len() >= 2 {
                            let series = [Series::new("Power", power_points, palette.blue)];
                            charts::line_plot(
                                ui,
                                "activity_power",
                                128.0,
                                &series,
                                "minutes",
                                "watts",
                                &palette,
                                false,
                            );
                        } else {
                            charts::chart_placeholder(
                                ui,
                                &palette,
                                "No power channel in this activity",
                                96.0,
                            );
                        }
                        ui.add_space(6.0);
                        let hr_points: Vec<[f64; 2]> = samples
                            .iter()
                            .filter_map(|s| {
                                s.heart_rate_bpm
                                    .map(|h| [s.elapsed_s / 60.0, h as f64])
                            })
                            .collect();
                        if hr_points.len() >= 2 {
                            let series = [Series::new("Heart rate", hr_points, palette.danger)];
                            charts::line_plot(
                                ui,
                                "activity_hr",
                                102.0,
                                &series,
                                "minutes",
                                "bpm",
                                &palette,
                                false,
                            );
                        }
                        ui.add_space(8.0);

                        // Zones and best efforts.
                        let buckets = zone_buckets(&samples, ftp);
                        let fractions: [f32; 6] =
                            std::array::from_fn(|i| buckets[i].fraction);
                        ui.label(
                            RichText::new("POWER ZONES")
                                .font(theme::font_semi(9.0))
                                .color(palette.faint),
                        );
                        ui.add_space(4.0);
                        theme::zone_bar(ui, &palette, &fractions, 10.0);
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            for (i, bucket) in buckets.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    theme::dot(ui, palette.zone(i + 1), 6.0);
                                    ui.label(
                                        RichText::new(format!(
                                            "Z{} {:.0}%",
                                            i + 1,
                                            bucket.fraction * 100.0
                                        ))
                                        .font(theme::font_mono(9.0))
                                        .color(palette.muted),
                                    );
                                });
                                ui.add_space(4.0);
                            }
                        });
                        ui.add_space(8.0);
                        let efforts = analytics::best_efforts(
                            &samples,
                            &[1, 5, 30, 60, 300, 600, 1200],
                        );
                        if !efforts.is_empty() {
                            ui.label(
                                RichText::new("BEST EFFORTS")
                                    .font(theme::font_semi(9.0))
                                    .color(palette.faint),
                            );
                            ui.add_space(4.0);
                            ui.horizontal_wrapped(|ui| {
                                for effort in &efforts {
                                    egui::Frame::new()
                                        .fill(palette.surface_alt)
                                        .corner_radius(egui::CornerRadius::same(8))
                                        .inner_margin(egui::Margin::symmetric(8, 4))
                                        .show(ui, |ui| {
                                            ui.vertical(|ui| {
                                                ui.label(
                                                    RichText::new(analytics::window_label(
                                                        effort.window_s,
                                                    ))
                                                    .font(theme::font_semi(8.5))
                                                    .color(palette.faint),
                                                );
                                                ui.label(
                                                    RichText::new(format!(
                                                        "{:.0} W",
                                                        effort.power_w
                                                    ))
                                                    .font(theme::font_mono(10.5))
                                                    .color(palette.ink),
                                                );
                                            });
                                        });
                                    ui.add_space(3.0);
                                }
                            });
                        }
                        ui.add_space(10.0);
                        theme::divider(ui, &palette);
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("NAME")
                                .font(theme::font_semi(8.5))
                                .color(palette.faint),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.edit_name)
                                .desired_width(ui.available_width()),
                        );
                        ui.add_space(4.0);
                        ui.add(
                            egui::TextEdit::multiline(&mut self.edit_notes)
                                .desired_rows(2)
                                .desired_width(ui.available_width())
                                .hint_text("Notes about this ride…"),
                        );
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if theme::primary_button(ui, "Save details", &palette).clicked() {
                                save_meta = true;
                            }
                            if theme::ghost_button(ui, Some(Icon::Download), "FIT", &palette)
                                .clicked()
                            {
                                export = Some((activity.id, ExportFormat::Fit));
                            }
                            if theme::ghost_button(ui, Some(Icon::Download), "GPX", &palette)
                                .clicked()
                            {
                                export = Some((activity.id, ExportFormat::Gpx));
                            }
                            if theme::ghost_button(ui, Some(Icon::Download), "TCX", &palette)
                                .clicked()
                            {
                                export = Some((activity.id, ExportFormat::Tcx));
                            }
                            if theme::ghost_button(ui, Some(Icon::Trash), "Delete", &palette)
                                .clicked()
                            {
                                delete = Some(activity.id);
                            }
                        });
                    }
                },
            );
        });

        if let Some(id) = open {
            self.load_activity_details(id);
        }
        if import {
            let path = self.activity_import_path.clone();
            self.import_activity(&path);
        }
        if save_meta {
            if let Some(activity) = activity.as_ref() {
                self.save_activity_meta(activity.id);
            }
        }
        if let Some((id, format)) = export {
            self.export_activity(id, format);
        }
        if let Some(id) = delete {
            self.pending_delete_activity = Some(id);
        }
    }
}

fn zone_buckets(
    samples: &[Sample],
    ftp: f32,
) -> [analytics::ZoneBucket; 6] {
    let rider = velocity_core::types::RiderProfile {
        ftp_w: ftp.max(1.0),
        ..Default::default()
    };
    let bounds = rider.power_zone_bounds();
    analytics::zone_distribution(
        samples.windows(2).filter_map(|w| {
            let power = w[0].power_w?;
            let dt = w[1].elapsed_s - w[0].elapsed_s;
            Some((power, dt))
        }),
        &bounds,
    )
}
