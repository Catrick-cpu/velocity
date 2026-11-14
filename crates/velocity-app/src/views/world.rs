//! The 3D world viewport, its in-world HUD and the graphics controls.

use crate::app::VelocityApp;
use crate::theme::{self, Palette};
use crate::world::{CameraMode, GpuClass, QualityPreset, WorldInput};
use eframe::egui::{self, Align2, Color32, CornerRadius, CursorIcon, FontId, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use std::sync::Arc;
use std::time::Duration;
use velocity_core::telemetry::SessionState;
use velocity_core::types::{fmt_duration, now_ms};

impl VelocityApp {
    /// Full world viewport with HUD overlay and a compact control strip.
    pub(crate) fn world_panel(&mut self, ui: &mut Ui, palette: &Palette) {
        let now = now_ms();
        let live = &self.session.live;
        let speed_kmh = live.current_speed(now).map(|r| r.value * 3.6).unwrap_or(0.0);
        let power_w = live.current_power(now).map(|r| r.value).unwrap_or(0.0);
        let cadence = live.current_cadence(now).map(|r| r.value).unwrap_or(0.0);
        let heart = live.current_hr(now).map(|r| r.value).unwrap_or(0.0);
        let grade = live
            .current_grade(now)
            .map(|r| r.value)
            .unwrap_or(self.sim_grade_pct);
        let distance_m = live.distance_m;
        let elapsed_s = live.elapsed_s;

        // Keep one Arc per route so the renderer does not rebuild terrain every frame.
        let route_arc = if let Some((id, route)) = &self.current_route {
            match &self.world_route {
                Some((cached, arc)) if cached == id => Some(arc.clone()),
                _ => {
                    let arc = Arc::new(route.clone());
                    self.world_route = Some((*id, arc.clone()));
                    Some(arc)
                }
            }
        } else {
            None
        };
        let route_name = self
            .current_route
            .as_ref()
            .map(|(_, route)| route.name.clone())
            .unwrap_or_else(|| "Free ride".to_string());
        let total_m = self
            .current_route
            .as_ref()
            .map(|(_, route)| route.stats.distance_m)
            .unwrap_or(0.0);
        let running = self.session.state == SessionState::Running;

        self.world.update(
            WorldInput {
                route: route_arc,
                distance_m,
                speed_mps: speed_kmh / 3.6,
                cadence_rpm: cadence,
                power_w,
                heart_rate_bpm: heart,
                grade_pct: grade,
                running,
                updated_ms: now,
            },
            self.world_prefs,
        );

        let height = (ui.available_width() * 0.44).clamp(300.0, 520.0);
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::drag());
        let response = response.on_hover_cursor(CursorIcon::Grab);
        if response.dragged() {
            let delta = response.drag_delta();
            self.world.orbit(delta.x, delta.y);
        }
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(18),
            Color32::from_rgb(13, 17, 21),
        );
        ui.painter().add(self.world.callback(rect));

        let stats = self.world.stats();
        paint_hud(
            ui,
            palette,
            rect,
            &HudData {
                route: &route_name,
                dem: stats.dem_status.label(),
                running,
                paused: self.session.state == SessionState::Paused,
                speed_kmh,
                power_w,
                cadence,
                heart,
                grade,
                distance_km: (distance_m / 1000.0) as f32,
                total_km: (total_m / 1000.0) as f32,
                elapsed_s,
            },
        );

        // Controls ----------------------------------------------------------
        ui.add_space(8.0);
        let mut prefs = self.world_prefs;
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("CAMERA")
                    .font(theme::font_semi(9.0))
                    .color(palette.faint),
            );
            for mode in CameraMode::all() {
                if ui
                    .selectable_label(prefs.camera.mode == mode, mode.label())
                    .clicked()
                {
                    prefs.camera.mode = mode;
                    changed = true;
                }
            }
            ui.add_space(2.0);
            if ui.small_button("−").on_hover_text("Further away").clicked() {
                self.world.zoom(140.0);
            }
            if ui.small_button("+").on_hover_text("Closer").clicked() {
                self.world.zoom(-140.0);
            }
            ui.add_space(8.0);
            ui.label(
                RichText::new("QUALITY")
                    .font(theme::font_semi(9.0))
                    .color(palette.faint),
            );
            egui::ComboBox::from_id_salt("world_quality")
                .selected_text(prefs.quality.label())
                .width(96.0)
                .show_ui(ui, |ui| {
                    for preset in QualityPreset::all() {
                        if ui
                            .selectable_label(prefs.quality == preset, preset.label())
                            .clicked()
                        {
                            prefs.quality = preset;
                            changed = true;
                        }
                    }
                });
            ui.add_space(4.0);
            if crate::views::accent_switch(ui, palette, &mut prefs.adaptive, "Adaptive") {
                changed = true;
            }
            if prefs.quality == QualityPreset::Ultra {
                ui.add_space(6.0);
                ui.label(
                    RichText::new("ULTRA: maximum fidelity, may run slowly on this GPU")
                        .font(theme::font_semi(9.0))
                        .color(palette.warning),
                );
            }
            ui.add_space(8.0);
            ui.label(
                RichText::new("TIME")
                    .font(theme::font_semi(9.0))
                    .color(palette.faint),
            );
            if ui
                .add(
                    egui::Slider::new(&mut prefs.hour, 4.5..=21.5)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .changed()
            {
                changed = true;
            }
            ui.label(
                RichText::new(prefs.hour_label())
                    .font(theme::font_mono(10.5))
                    .color(palette.ink_soft),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new("CLOUD")
                    .font(theme::font_semi(9.0))
                    .color(palette.faint),
            );
            if ui
                .add(egui::Slider::new(&mut prefs.cloud, 0.0..=1.0).show_value(false))
                .changed()
            {
                changed = true;
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new("WET")
                    .font(theme::font_semi(9.0))
                    .color(palette.faint),
            );
            if ui
                .add(egui::Slider::new(&mut prefs.wet, 0.0..=1.0).show_value(false))
                .changed()
            {
                changed = true;
            }
        });
        if changed {
            self.world_prefs = prefs;
            self.world.set_prefs(prefs);
            if ui.input(|i| i.pointer.any_released()) {
                self.save_world_prefs();
            }
        }

        let stats = self.world.stats();
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let fps_color = if stats.frame_ms > 24.0 {
                palette.warning
            } else if stats.frame_ms > 0.0 {
                palette.positive
            } else {
                palette.muted
            };
            ui.label(
                RichText::new(format!("{:.0} fps · {:.1} ms", stats.fps, stats.frame_ms))
                    .font(theme::font_mono(10.5))
                    .color(fps_color),
            );
            ui.label(
                RichText::new(format!(
                    "· {} · {} chunks · {:.0}k tris · {} draws · {}",
                    stats.quality,
                    stats.visible_chunks,
                    stats.triangles as f32 / 1000.0,
                    stats.draws,
                    stats.dem_status.label()
                ))
                .font(theme::font_mono(10.5))
                .color(palette.faint),
            );
        });
        ui.label(
            RichText::new(format!(
                "{} · {} · {} · render {:.0}%",
                stats.gpu,
                GpuClass::classify(&stats.gpu).label(),
                stats.terrain_credit,
                stats.scale * 100.0
            ))
            .font(theme::font_body(9.5))
            .color(palette.faint),
        );
        if let Some(err) = &stats.error {
            ui.label(
                RichText::new(format!("World: {err}"))
                    .font(theme::font_body(10.0))
                    .color(palette.danger),
            );
        }

        // Keep the world animating while it is on screen.
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
}

struct HudData<'a> {
    route: &'a str,
    dem: &'a str,
    running: bool,
    paused: bool,
    speed_kmh: f32,
    power_w: f32,
    cadence: f32,
    heart: f32,
    grade: f32,
    distance_km: f32,
    total_km: f32,
    elapsed_s: f64,
}

fn paint_hud(ui: &Ui, palette: &Palette, rect: Rect, data: &HudData<'_>) {
    let painter = ui.painter();
    let glass = Color32::from_black_alpha(112);
    let white = Color32::from_rgb(244, 246, 248);
    let soft = Color32::from_rgb(176, 184, 192);
    let accent = palette.accent;

    // Route + status, top left.
    let top = Rect::from_min_size(
        rect.left_top() + vec2(14.0, 14.0),
        vec2(rect.width().min(430.0) - 28.0, 52.0),
    );
    painter.rect_filled(top, CornerRadius::same(12), glass);
    painter.text(
        top.left_top() + vec2(12.0, 9.0),
        Align2::LEFT_TOP,
        data.route.to_ascii_uppercase(),
        theme::font_semi(11.0),
        white,
    );
    painter.text(
        top.left_bottom() + vec2(12.0, -9.0),
        Align2::LEFT_BOTTOM,
        format!(
            "3D WORLD · {} · {:.0}% GRADIENT",
            data.dem.to_ascii_uppercase(),
            data.grade
        ),
        theme::font_mono(9.5),
        soft,
    );
    let (dot, label) = if data.paused {
        (palette.warning, "PAUSED")
    } else if data.running {
        (accent, "LIVE")
    } else {
        (soft, "READY")
    };
    let badge = Rect::from_min_size(
        pos2(top.right() - 74.0, top.top() + 10.0),
        vec2(62.0, 18.0),
    );
    painter.rect_filled(badge, CornerRadius::same(9), Color32::from_black_alpha(70));
    painter.circle_filled(badge.left_center() + vec2(10.0, 0.0), 3.0, dot);
    painter.text(
        badge.center() + vec2(4.0, 0.0),
        Align2::CENTER_CENTER,
        label,
        theme::font_semi(9.0),
        white,
    );

    // Big speed, bottom left.
    let speed_box = Rect::from_min_size(
        pos2(rect.left() + 14.0, rect.bottom() - 108.0),
        vec2(286.0, 94.0),
    );
    painter.rect_filled(speed_box, CornerRadius::same(14), glass);
    painter.text(
        speed_box.left_top() + vec2(14.0, 8.0),
        Align2::LEFT_TOP,
        "SPEED",
        theme::font_semi(9.0),
        soft,
    );
    painter.text(
        speed_box.left_bottom() + vec2(14.0, -10.0),
        Align2::LEFT_BOTTOM,
        format!("{:.0}", data.speed_kmh),
        FontId::monospace(46.0),
        white,
    );
    painter.text(
        speed_box.left_bottom() + vec2(150.0, -16.0),
        Align2::LEFT_BOTTOM,
        "km/h",
        theme::font_semi(11.0),
        soft,
    );
    painter.text(
        speed_box.right_bottom() + vec2(-14.0, -14.0),
        Align2::RIGHT_BOTTOM,
        format!(
            "{} W   {} rpm   {} bpm",
            data.power_w as i32, data.cadence as i32, data.heart as i32
        ),
        theme::font_mono(10.5),
        soft,
    );

    // Distance / time, bottom right.
    let info_box = Rect::from_min_size(
        pos2(rect.right() - 236.0, rect.bottom() - 108.0),
        vec2(222.0, 94.0),
    );
    painter.rect_filled(info_box, CornerRadius::same(14), glass);
    let remaining = (data.total_km - data.distance_km).max(0.0);
    painter.text(
        info_box.left_top() + vec2(14.0, 10.0),
        Align2::LEFT_TOP,
        format!("{:.2} km", data.distance_km),
        theme::font_mono(22.0),
        white,
    );
    painter.text(
        info_box.left_top() + vec2(14.0, 40.0),
        Align2::LEFT_TOP,
        format!("{:.1} km to go", remaining),
        theme::font_mono(10.0),
        soft,
    );
    painter.text(
        info_box.right_top() + vec2(-14.0, 12.0),
        Align2::RIGHT_TOP,
        fmt_duration(data.elapsed_s),
        theme::font_mono(14.0),
        white,
    );

    // Hint, top right.
    let hint = Rect::from_min_size(
        pos2(rect.right() - 196.0, rect.top() + 14.0),
        vec2(182.0, 24.0),
    );
    if rect.width() > 520.0 {
        painter.rect_filled(hint, CornerRadius::same(12), Color32::from_black_alpha(70));
        painter.text(
            hint.center(),
            Align2::CENTER_CENTER,
            "DRAG TO ORBIT · WORLD IS LIVE",
            theme::font_semi(8.5),
            soft,
        );
    }

    // Thin accent line marks the road horizon.
    painter.line_segment(
        [
            pos2(rect.left() + 14.0, rect.top() + 1.0),
            pos2(rect.right() - 14.0, rect.top() + 1.0),
        ],
        Stroke::new(1.0, Color32::from_black_alpha(60)),
    );
}
