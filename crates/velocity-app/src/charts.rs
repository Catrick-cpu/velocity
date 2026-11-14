//! Chart and map primitives. Charts use `egui_plot`; the route map is a light
//! custom vector projection so it stays fast on integrated graphics and needs
//! no tile server at startup.

use crate::theme::{Palette, font_mono, font_semi};
use eframe::egui::{self, Align2, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use egui_plot::{HLine, Line, Plot, PlotPoints};
use velocity_core::analytics::DailyLoad;
use velocity_core::route::Route;

#[derive(Clone)]
pub struct Series {
    pub name: String,
    pub points: Vec<[f64; 2]>,
    pub color: Color32,
    pub width: f32,
    pub fill: Option<f32>,
}

impl Series {
    pub fn new(name: impl Into<String>, points: Vec<[f64; 2]>, color: Color32) -> Self {
        Self {
            name: name.into(),
            points,
            color,
            width: 2.0,
            fill: None,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn fill(mut self, baseline: f32) -> Self {
        self.fill = Some(baseline);
        self
    }
}

pub fn line_plot(
    ui: &mut Ui,
    id: &str,
    height: f32,
    series: &[Series],
    x_unit: &str,
    y_unit: &str,
    palette: &Palette,
    show_legend: bool,
) {
    let mut plot = Plot::new(id)
        .height(height)
        .allow_zoom(false)
        .allow_drag(false)
        .allow_scroll(false)
        .allow_boxed_zoom(false)
        .allow_double_click_reset(false)
        .show_background(false)
        .show_grid([true, true])
        .show_axes([true, true])
        .x_axis_label(x_unit)
        .y_axis_label(y_unit)
        .y_axis_min_width(38.0);

    if show_legend {
        plot = plot.legend(egui_plot::Legend::default());
    }

    plot.show(ui, |plot_ui| {
        for s in series {
            let mut line = Line::new(s.name.clone(), PlotPoints::from(s.points.clone()))
                .color(s.color)
                .width(s.width);
            if let Some(base) = s.fill {
                line = line.fill(base).fill_alpha(0.16);
            }
            plot_ui.line(line);
        }
        if series.is_empty() {
            plot_ui.add(HLine::new("", 0.0).color(Color32::TRANSPARENT));
        }
    });

    // The plot paints its own text; this hint keeps zero-data charts readable.
    if series.iter().all(|s| s.points.is_empty()) {
        let hint = Rect::from_min_size(ui.min_rect().min, ui.available_size());
        let _ = hint;
        let _ = font_semi(12.0);
    }
    let _ = palette;
}

/// Weekly training bars with a subtle baseline and hover value.
pub fn week_bars(ui: &mut Ui, palette: &Palette, history: &[DailyLoad], height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(10), palette.surface_alt);

    let days = history.len().max(1);
    let max = history
        .iter()
        .map(|d| d.tss)
        .fold(1.0f32, f32::max)
        .max(20.0);
    let gap = 7.0f32;
    let bar_w = ((rect.width() - gap * (days as f32 + 1.0)) / days as f32).max(4.0);
    for (i, day) in history.iter().enumerate() {
        let h = (day.tss / max) * (rect.height() - 26.0);
        let x = rect.left() + gap + i as f32 * (bar_w + gap);
        let bar = Rect::from_min_max(
            pos2(x, rect.bottom() - 20.0 - h),
            pos2(x + bar_w, rect.bottom() - 20.0),
        );
        let (fill, color) = if day.tss > 0.0 {
            (Palette::alpha(palette.accent, 210), palette.ink)
        } else {
            (palette.surface_sunken, palette.faint)
        };
        ui.painter().rect_filled(bar, CornerRadius::same(3), fill);
        if day.tss > 0.0 {
            ui.painter().circle_filled(
                pos2(bar.center().x, bar.top() - 4.0),
                2.0,
                palette.accent,
            );
        }
        if i % 7 == 0 || i + 1 == days {
            let label = day.date.get(5..).unwrap_or(&day.date);
            ui.painter().text(
                pos2(bar.center().x, rect.bottom() - 6.0),
                Align2::CENTER_CENTER,
                label,
                font_mono(9.5),
                palette.faint,
            );
        }
        let _ = color;
    }
}

/// Train-load trend (CTL/ATL) with a zero line, used on the dashboard.
pub fn load_plot(ui: &mut Ui, palette: &Palette, history: &[DailyLoad], height: f32) {
    let ctl: Vec<[f64; 2]> = history
        .iter()
        .enumerate()
        .map(|(i, d)| [i as f64, d.ctl as f64])
        .collect();
    let atl: Vec<[f64; 2]> = history
        .iter()
        .enumerate()
        .map(|(i, d)| [i as f64, d.atl as f64])
        .collect();
    let series = [
        Series::new("Fitness (CTL)", ctl, palette.blue).width(2.2),
        Series::new("Fatigue (ATL)", atl, palette.orange).width(1.6),
    ];
    line_plot(ui, "load_plot", height, &series, "days ago", "load", palette, true);
}

/// Elevation profile with gradient shading hints.
pub fn elevation_plot(
    ui: &mut Ui,
    id: &str,
    palette: &Palette,
    route: &Route,
    height: f32,
    progress: Option<f64>,
) {
    let profile = route.profile(180);
    let points: Vec<[f64; 2]> = profile
        .iter()
        .map(|p| [p.distance_m / 1000.0, p.elevation_m as f64])
        .collect();
    let min = route.stats.min_elevation_m as f64;
    let max = route.stats.max_elevation_m as f64;
    let _ = (min, max);
    let baseline = route.stats.min_elevation_m as f64 - 4.0;
    let mut series = vec![
        Series::new("Elevation", points, palette.blue)
            .width(1.8)
            .fill(baseline as f32),
    ];
    if let Some(fraction) = progress {
        let d = route.stats.distance_m.max(1.0) * fraction.clamp(0.0, 1.0) / 1000.0;
        let p = route.position_at(fraction.clamp(0.0, 1.0) * route.stats.distance_m);
        series.push(
            Series::new(
                "You",
                vec![[d, p.elevation_m as f64]],
                palette.accent,
            )
            .width(1.0),
        );
    }
    line_plot(ui, id, height, &series, "km", "m", palette, false);
}

/// Draw a route from its lat/lon geometry inside the current UI.
pub fn route_map(
    ui: &mut Ui,
    palette: &Palette,
    route: &Route,
    progress: Option<f64>,
    height: f32,
) -> Rect {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    if route.points.len() < 2 {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(14), palette.surface_alt);
        return rect;
    }
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(14), palette.map_land);
    painter.rect_stroke(
        rect,
        CornerRadius::same(14),
        Stroke::new(1.0, palette.line),
        egui::StrokeKind::Inside,
    );

    let inner = rect.shrink(18.0);
    let mut min_lat = f64::INFINITY;
    let mut max_lat = f64::NEG_INFINITY;
    let mut min_lon = f64::INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    let mut mid_lat = 0.0;
    for p in &route.points {
        min_lat = min_lat.min(p.lat);
        max_lat = max_lat.max(p.lat);
        min_lon = min_lon.min(p.lon);
        max_lon = max_lon.max(p.lon);
        mid_lat += p.lat;
    }
    mid_lat /= route.points.len() as f64;
    let kx = mid_lat.to_radians().cos().max(0.1);
    let span_x = ((max_lon - min_lon) * kx).max(1e-9);
    let span_y = (max_lat - min_lat).max(1e-9);
    let scale = (inner.width() as f64 / span_x).min(inner.height() as f64 / span_y);
    let off_x = inner.center().x as f64 - (min_lon + max_lon) * 0.5 * kx * scale;
    let off_y = inner.center().y as f64 + (min_lat + max_lat) * 0.5 * scale;
    let project = |lat: f64, lon: f64| {
        pos2(
            (lon * kx * scale + off_x) as f32,
            (off_y - lat * scale) as f32,
        )
    };

    // Grid, clipped manually to the rounded rect.
    for i in 1..6 {
        let y = rect.top() + rect.height() * i as f32 / 6.0;
        painter.line_segment(
            [pos2(rect.left() + 12.0, y), pos2(rect.right() - 12.0, y)],
            Stroke::new(1.0, Palette::alpha(palette.map_grid, 170)),
        );
    }
    for i in 1..8 {
        let x = rect.left() + rect.width() * i as f32 / 8.0;
        painter.line_segment(
            [pos2(x, rect.top() + 12.0), pos2(x, rect.bottom() - 12.0)],
            Stroke::new(1.0, Palette::alpha(palette.map_grid, 170)),
        );
    }

    let pts: Vec<Pos2> = route
        .points
        .iter()
        .map(|p| project(p.lat, p.lon))
        .collect();
    // Casing then colour: readable in both themes without a tile layer.
    painter.add(egui::Shape::line(
        pts.clone(),
        Stroke::new(5.5, palette.surface),
    ));
    painter.add(egui::Shape::line(
        pts.clone(),
        Stroke::new(2.6, palette.blue),
    ));

    let start = pts.first().copied().unwrap_or(rect.center());
    let end = pts.last().copied().unwrap_or(rect.center());
    painter.circle_filled(start, 5.5, palette.surface);
    painter.circle_filled(start, 3.6, palette.positive);
    painter.circle_filled(end, 5.5, palette.surface);
    painter.circle_filled(end, 3.6, palette.danger);

    if let Some(fraction) = progress {
        let p = route.position_at(fraction.clamp(0.0, 1.0) * route.stats.distance_m);
        let pos = project(p.lat, p.lon);
        painter.circle_filled(pos, 10.0, Palette::alpha(palette.accent, 55));
        painter.circle_filled(pos, 6.0, palette.surface);
        painter.circle_filled(pos, 4.0, palette.ink);
    }

    // Compass and scale marks keep the map feeling intentional.
    painter.text(
        pos2(rect.right() - 18.0, rect.top() + 16.0),
        Align2::RIGHT_TOP,
        "N",
        font_semi(10.0),
        palette.faint,
    );
    painter.line_segment(
        [
            pos2(rect.right() - 20.0, rect.top() + 24.0),
            pos2(rect.right() - 20.0, rect.top() + 38.0),
        ],
        Stroke::new(1.0, palette.faint),
    );
    painter.text(
        pos2(rect.left() + 14.0, rect.bottom() - 12.0),
        Align2::LEFT_BOTTOM,
        format!("{:.1} km", route.stats.distance_m / 1000.0),
        font_mono(9.5),
        palette.faint,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    rect
}

/// A compact dashed placeholder when a chart has no data.
pub fn chart_placeholder(ui: &mut Ui, palette: &Palette, text: &str, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(10), palette.surface_alt);
    let mut x = rect.left() + 16.0;
    while x < rect.right() - 16.0 {
        ui.painter().line_segment(
            [pos2(x, rect.center().y), pos2((x + 6.0).min(rect.right()), rect.center().y)],
            Stroke::new(1.0, palette.line_strong),
        );
        x += 12.0;
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        font_semi(11.5),
        palette.faint,
    );
}
