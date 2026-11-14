//! Route library: GPX import, course profile and route selection for rides.

use crate::app::VelocityApp;
use crate::charts;
use crate::theme::{self, Icon, Palette};
use crate::views::{Page, titled_card};
use eframe::egui::{self, Align, Layout, RichText, Ui};
use velocity_core::route::Route;
use velocity_core::types::{fmt_distance_km, local_date, now_ms};

impl VelocityApp {
    pub(crate) fn routes(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        let mut open: Option<i64> = None;
        let mut import = false;
        let mut use_for_ride: Option<i64> = None;
        let mut export: Option<i64> = None;
        let mut delete: Option<i64> = None;

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("TRAIN · ROUTES")
                        .font(theme::font_semi(10.0))
                        .color(palette.faint),
                );
                ui.add_space(3.0);
                ui.label(
                    RichText::new("Route library")
                        .font(theme::font_display(34.0))
                        .color(palette.ink),
                );
                ui.add_space(3.0);
                ui.label(
                    RichText::new(
                        "Import real GPX tracks. Elevation profiles drive both the visual course and the trainer's simulated gradient.",
                    )
                    .font(theme::font_body(13.0))
                    .color(palette.muted),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let _ = theme::pill(
                    ui,
                    &format!("{} ROUTES", self.routes.len()),
                    palette.muted,
                    palette.surface_sunken,
                );
            });
        });
        ui.add_space(16.0);

        theme::soft_card(&palette).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::Vec2::splat(30.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, egui::CornerRadius::same(9), palette.surface);
                theme::paint_icon(ui, rect.shrink(7.0), Icon::Routes, palette.muted);
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("Import a GPX route")
                            .font(theme::font_semi(12.5))
                            .color(palette.ink),
                    );
                    ui.label(
                        RichText::new("Track points, elevation and gradients")
                            .font(theme::font_body(10.5))
                            .color(palette.faint),
                    );
                });
                ui.add_space(10.0);
                ui.add_sized(
                    [ui.available_width().min(430.0) - 110.0, 32.0],
                    egui::TextEdit::singleline(&mut self.route_import_path)
                        .hint_text("/home/you/routes/loop.gpx"),
                );
                if theme::ghost_button(ui, Some(Icon::Upload), "Import", &palette).clicked() {
                    import = true;
                }
            });
        });
        ui.add_space(14.0);

        let query = self.route_search.to_ascii_lowercase();
        let routes: Vec<(i64, Route)> = self
            .routes
            .iter()
            .filter(|(_, r)| {
                query.is_empty() || r.name.to_ascii_lowercase().contains(&query)
            })
            .cloned()
            .collect();
        let selected = self.selected_route;
        let detail = self.selected_route_data().map(|(id, r)| (id, r.clone()));
        let current = self.current_route.as_ref().map(|(id, _)| *id);

        ui.columns(2, |cols| {
            titled_card(
                &mut cols[0],
                &palette,
                "All routes",
                Some("Select a route to inspect its course"),
                |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.route_search)
                            .hint_text("Search routes…")
                            .desired_width(ui.available_width()),
                    );
                    ui.add_space(8.0);
                    if routes.is_empty() {
                        theme::empty_state(
                            ui,
                            &palette,
                            Icon::Routes,
                            if self.routes.is_empty() {
                                "No routes yet"
                            } else {
                                "No matching routes"
                            },
                            "Import a GPX track to get started.",
                        );
                    } else {
                        for (id, route) in &routes {
                            let selected_row = selected == Some(*id);
                            let response = theme::list_row(
                                ui,
                                &palette,
                                selected_row,
                                62.0,
                                |ui, palette| {
                                    ui.horizontal(|ui| {
                                        let (rect, _) = ui.allocate_exact_size(
                                            egui::Vec2::splat(36.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(
                                            rect,
                                            egui::CornerRadius::same(10),
                                            if selected_row {
                                                palette.surface
                                            } else {
                                                palette.surface_alt
                                            },
                                        );
                                        theme::paint_icon(
                                            ui,
                                            rect.shrink(9.0),
                                            Icon::Map,
                                            palette.blue,
                                        );
                                        ui.add_space(4.0);
                                        ui.vertical(|ui| {
                                            ui.label(
                                                RichText::new(&route.name)
                                                    .font(theme::font_semi(12.5))
                                                    .color(palette.ink),
                                            );
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} · {:.0} m gain · {}",
                                                    fmt_distance_km(
                                                        route.stats.distance_m
                                                    ),
                                                    route.stats.elevation_gain_m,
                                                    route.stats.difficulty()
                                                ))
                                                .font(theme::font_body(9.5))
                                                .color(palette.faint),
                                            );
                                        });
                                        ui.with_layout(
                                            Layout::right_to_left(Align::Center),
                                            |ui| {
                                                if current == Some(*id) {
                                                    let _ = theme::pill(
                                                        ui,
                                                        "ACTIVE",
                                                        palette.accent_ink,
                                                        palette.accent,
                                                    );
                                                }
                                            },
                                        );
                                    });
                                },
                            );
                            if response.clicked() {
                                open = Some(*id);
                            }
                            ui.add_space(1.0);
                        }
                    }
                },
            );

            titled_card(
                &mut cols[1],
                &palette,
                "Route detail",
                Some("Course geometry, profile and gradient"),
                |ui| match &detail {
                    None => {
                        theme::empty_state(
                            ui,
                            &palette,
                            Icon::Map,
                            "Select a route",
                            "The course map and elevation profile will appear here.",
                        );
                    }
                    Some((id, route)) => {
                        charts::route_map(ui, &palette, route, None, 236.0);
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(&route.name)
                                        .font(theme::font_display(22.0))
                                        .color(palette.ink),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "{} points · {} difficulty",
                                        route.points.len(),
                                        route.stats.difficulty()
                                    ))
                                    .font(theme::font_body(10.5))
                                    .color(palette.muted),
                                );
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if route.is_demo {
                                    let _ = theme::pill(
                                        ui,
                                        "DEMO GEOMETRY",
                                        palette.warning,
                                        palette.warning_soft,
                                    );
                                }
                                if current == Some(*id) {
                                    let _ = theme::pill(
                                        ui,
                                        "ACTIVE ROUTE",
                                        palette.accent_ink,
                                        palette.accent,
                                    );
                                }
                            });
                        });
                        ui.add_space(10.0);
                        ui.columns(4, |stats| {
                            crate::views::stat_cell(
                                &mut stats[0],
                                &palette,
                                "Distance",
                                &format!("{:.1}", route.stats.distance_m / 1000.0),
                                "km",
                            );
                            crate::views::stat_cell(
                                &mut stats[1],
                                &palette,
                                "Climbing",
                                &format!("{:.0}", route.stats.elevation_gain_m),
                                "m",
                            );
                            crate::views::stat_cell(
                                &mut stats[2],
                                &palette,
                                "Avg grade",
                                &format!("{:.1}", route.stats.avg_grade_pct),
                                "%",
                            );
                            crate::views::stat_cell(
                                &mut stats[3],
                                &palette,
                                "Max grade",
                                &format!("{:.1}", route.stats.max_grade_pct),
                                "%",
                            );
                        });
                        ui.add_space(8.0);
                        charts::elevation_plot(
                            ui,
                            "route_elevation",
                            &palette,
                            route,
                            138.0,
                            None,
                        );
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if theme::accent_button(ui, Icon::Play, "Use for ride", &palette)
                                .clicked()
                            {
                                use_for_ride = Some(*id);
                            }
                            if theme::ghost_button(ui, Some(Icon::Download), "Export GPX", &palette)
                                .clicked()
                            {
                                export = Some(*id);
                            }
                            if theme::ghost_button(ui, Some(Icon::Trash), "Delete", &palette)
                                .clicked()
                            {
                                delete = Some(*id);
                            }
                        });
                        ui.add_space(8.0);
                        if !route.description.is_empty() {
                            ui.label(
                                RichText::new(&route.description)
                                    .font(theme::font_body(11.0))
                                    .color(palette.muted),
                            );
                        }
                        ui.label(
                            RichText::new(format!(
                                "Imported or created in VELOCITY · {}",
                                local_date(now_ms())
                            ))
                            .font(theme::font_body(9.5))
                            .color(palette.faint),
                        );
                    }
                },
            );
        });

        if let Some(id) = open {
            self.select_route(id);
        }
        if import {
            let path = self.route_import_path.clone();
            self.import_route(&path);
        }
        if let Some(id) = export {
            self.export_route(id);
        }
        if let Some(id) = delete {
            self.pending_delete_route = Some(id);
        }
        if let Some(id) = use_for_ride {
            self.use_route(id, false);
            self.page = Page::Ride;
        }
    }
}
