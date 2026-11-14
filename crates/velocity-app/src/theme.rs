//! Visual identity for VELOCITY: palette, typography, custom vector icons and
//! small reusable widgets. The design is deliberately light-first with a vivid
//! volt accent, and every surface is tuned for a 1280×800 logical viewport.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame,
    Margin, Pos2, Rect, Response, RichText, Sense, Shadow, Stroke, TextStyle, Ui, Vec2, pos2, vec2,
};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Palette
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    Light,
    Dark,
}

impl ThemeMode {
    pub fn is_dark(self) -> bool {
        matches!(self, Self::Dark)
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
    pub fn from_key(key: &str) -> Self {
        if key.eq_ignore_ascii_case("dark") {
            Self::Dark
        } else {
            Self::Light
        }
    }
    pub fn toggled(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub rail: Color32,
    pub surface: Color32,
    pub surface_alt: Color32,
    pub surface_sunken: Color32,
    pub ink: Color32,
    pub ink_soft: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub line: Color32,
    pub line_strong: Color32,
    pub accent: Color32,
    pub accent_ink: Color32,
    pub accent_soft: Color32,
    pub positive: Color32,
    pub positive_soft: Color32,
    pub warning: Color32,
    pub warning_soft: Color32,
    pub danger: Color32,
    pub danger_soft: Color32,
    pub blue: Color32,
    pub blue_soft: Color32,
    pub purple: Color32,
    pub orange: Color32,
    pub shadow: Color32,
    pub map_land: Color32,
    pub map_grid: Color32,
}

impl Palette {
    pub fn light() -> Self {
        Self {
            dark: false,
            bg: Color32::from_rgb(0xF2, 0xF5, 0xF8),
            rail: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            surface_alt: Color32::from_rgb(0xF7, 0xF9, 0xFB),
            surface_sunken: Color32::from_rgb(0xEC, 0xF0, 0xF4),
            ink: Color32::from_rgb(0x0B, 0x12, 0x20),
            ink_soft: Color32::from_rgb(0x2A, 0x33, 0x42),
            muted: Color32::from_rgb(0x5B, 0x65, 0x73),
            faint: Color32::from_rgb(0x8C, 0x96, 0xA3),
            line: Color32::from_rgb(0xE2, 0xE8, 0xEE),
            line_strong: Color32::from_rgb(0xCF, 0xD8, 0xE1),
            accent: Color32::from_rgb(0xC8, 0xF1, 0x35),
            accent_ink: Color32::from_rgb(0x14, 0x1E, 0x07),
            accent_soft: Color32::from_rgb(0xEF, 0xF9, 0xC8),
            positive: Color32::from_rgb(0x12, 0x9B, 0x5A),
            positive_soft: Color32::from_rgb(0xE4, 0xF7, 0xEC),
            warning: Color32::from_rgb(0xC8, 0x7A, 0x00),
            warning_soft: Color32::from_rgb(0xFF, 0xF4, 0xDD),
            danger: Color32::from_rgb(0xD9, 0x3A, 0x42),
            danger_soft: Color32::from_rgb(0xFD, 0xE9, 0xEA),
            blue: Color32::from_rgb(0x25, 0x63, 0xEB),
            blue_soft: Color32::from_rgb(0xE8, 0xEF, 0xFE),
            purple: Color32::from_rgb(0x7C, 0x3A, 0xED),
            orange: Color32::from_rgb(0xEA, 0x6A, 0x14),
            shadow: Color32::from_rgba_unmultiplied(15, 23, 42, 18),
            map_land: Color32::from_rgb(0xEA, 0xF0, 0xE9),
            map_grid: Color32::from_rgb(0xDC, 0xE6, 0xDC),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            bg: Color32::from_rgb(0x0A, 0x0D, 0x12),
            rail: Color32::from_rgb(0x0E, 0x12, 0x18),
            surface: Color32::from_rgb(0x12, 0x16, 0x1D),
            surface_alt: Color32::from_rgb(0x17, 0x1C, 0x24),
            surface_sunken: Color32::from_rgb(0x0D, 0x11, 0x16),
            ink: Color32::from_rgb(0xF5, 0xF7, 0xFA),
            ink_soft: Color32::from_rgb(0xD5, 0xDD, 0xE7),
            muted: Color32::from_rgb(0x98, 0xA3, 0xB4),
            faint: Color32::from_rgb(0x6B, 0x77, 0x86),
            line: Color32::from_rgb(0x24, 0x2C, 0x36),
            line_strong: Color32::from_rgb(0x33, 0x3E, 0x4B),
            accent: Color32::from_rgb(0xC8, 0xF1, 0x35),
            accent_ink: Color32::from_rgb(0x0A, 0x0D, 0x12),
            accent_soft: Color32::from_rgb(0x25, 0x30, 0x0B),
            positive: Color32::from_rgb(0x43, 0xD9, 0x8F),
            positive_soft: Color32::from_rgb(0x11, 0x2A, 0x1E),
            warning: Color32::from_rgb(0xF5, 0xB5, 0x44),
            warning_soft: Color32::from_rgb(0x2A, 0x21, 0x0D),
            danger: Color32::from_rgb(0xFF, 0x63, 0x69),
            danger_soft: Color32::from_rgb(0x2C, 0x14, 0x17),
            blue: Color32::from_rgb(0x60, 0xA5, 0xFA),
            blue_soft: Color32::from_rgb(0x12, 0x1E, 0x33),
            purple: Color32::from_rgb(0xA7, 0x8B, 0xFA),
            orange: Color32::from_rgb(0xFB, 0x92, 0x3C),
            shadow: Color32::from_rgba_unmultiplied(0, 0, 0, 80),
            map_land: Color32::from_rgb(0x10, 0x18, 0x16),
            map_grid: Color32::from_rgb(0x1A, 0x25, 0x21),
        }
    }

    pub fn of(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Light => Self::light(),
            ThemeMode::Dark => Self::dark(),
        }
    }

    /// Translucent variant used for soft fills.
    pub fn alpha(color: Color32, a: u8) -> Color32 {
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a)
    }

    /// A zone colour from 1 (recovery) to 6 (sprint).
    pub fn zone(&self, zone: usize) -> Color32 {
        match zone {
            1 => Color32::from_rgb(0x7D, 0xB8, 0xF5),
            2 => Color32::from_rgb(0x59, 0xC7, 0x9A),
            3 => Color32::from_rgb(0xC8, 0xF1, 0x35),
            4 => Color32::from_rgb(0xF5, 0xB5, 0x44),
            5 => Color32::from_rgb(0xF0, 0x7A, 0x4A),
            _ => Color32::from_rgb(0xE5, 0x4D, 0x5B),
        }
    }

    pub fn card_shadow(&self) -> Shadow {
        Shadow {
            offset: [0, if self.dark { 4 } else { 2 }],
            blur: if self.dark { 14 } else { 10 },
            spread: 0,
            color: self.shadow,
        }
    }
}

// ---------------------------------------------------------------------------
// Typography
// ---------------------------------------------------------------------------

pub const INTER: &str = "velocity-inter";
pub const INTER_SEMI: &str = "velocity-inter-semibold";
pub const DISPLAY: &str = "velocity-display";
pub const MONO: &str = "velocity-mono";

pub fn family_display() -> FontFamily {
    FontFamily::Name(DISPLAY.into())
}
pub fn family_semi() -> FontFamily {
    FontFamily::Name(INTER_SEMI.into())
}
pub fn family_mono() -> FontFamily {
    FontFamily::Name(MONO.into())
}

pub fn font_display(size: f32) -> FontId {
    FontId::new(size, family_display())
}
pub fn font_semi(size: f32) -> FontId {
    FontId::new(size, family_semi())
}
pub fn font_mono(size: f32) -> FontId {
    FontId::new(size, family_mono())
}
pub fn font_body(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn rich_display(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(font_display(size))
}
pub fn rich_semi(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(font_semi(size))
}
pub fn rich_mono(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(font_mono(size))
}

fn font_data(bytes: &'static [u8], weight: f32) -> Arc<FontData> {
    use egui::epaint::text::{FontTweak, VariationCoords};
    Arc::new(FontData::from_static(bytes).tweak(FontTweak {
        coords: VariationCoords::new([(b"wght", weight)]),
        ..Default::default()
    }))
}

/// Install the bundled variable fonts once, before the first frame.
pub fn install_fonts(ctx: &egui::Context) {
    const INTER_TTF: &[u8] = include_bytes!("../../../assets/fonts/InterVariable.ttf");
    const INTER_TIGHT_TTF: &[u8] = include_bytes!("../../../assets/fonts/InterTight.ttf");
    const MONO_TTF: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono.ttf");

    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(INTER.into(), font_data(INTER_TTF, 400.0));
    fonts
        .font_data
        .insert(INTER_SEMI.into(), font_data(INTER_TTF, 600.0));
    fonts
        .font_data
        .insert(DISPLAY.into(), font_data(INTER_TIGHT_TTF, 720.0));
    fonts.font_data.insert(MONO.into(), font_data(MONO_TTF, 460.0));

    let proportional_fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mono_fallback = fonts
        .families
        .get(&FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();

    let mut proportional = vec![INTER.to_owned()];
    proportional.extend(proportional_fallback.iter().cloned());

    let mut mono = vec![MONO.to_owned()];
    mono.extend(mono_fallback.iter().cloned());

    fonts
        .families
        .insert(FontFamily::Proportional, proportional.clone());
    fonts.families.insert(FontFamily::Monospace, mono.clone());

    let mut display = vec![DISPLAY.to_owned(), INTER_SEMI.to_owned()];
    display.extend(proportional_fallback.iter().cloned());
    fonts
        .families
        .insert(FontFamily::Name(DISPLAY.into()), display);

    let mut semi = vec![INTER_SEMI.to_owned(), INTER.to_owned()];
    semi.extend(proportional_fallback.iter().cloned());
    fonts
        .families
        .insert(FontFamily::Name(INTER_SEMI.into()), semi);

    fonts.families.insert(FontFamily::Name(MONO.into()), mono);
    ctx.set_fonts(fonts);
}

fn base_style(palette: &Palette) -> egui::Style {
    let mut style = egui::Style::default();
    style.spacing.item_spacing = vec2(10.0, 9.0);
    style.spacing.button_padding = vec2(14.0, 8.0);
    style.spacing.menu_margin = Margin::same(10);
    style.spacing.window_margin = Margin::same(18);
    style.spacing.indent = 18.0;
    style.spacing.interact_size = vec2(38.0, 32.0);
    style.spacing.slider_width = 180.0;
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.floating = true;
    style.spacing.combo_width = 160.0;

    use TextStyle as T;
    let styles = [
        (T::Small, font_body(12.0)),
        (T::Body, font_body(14.5)),
        (T::Button, font_semi(14.0)),
        (T::Monospace, font_mono(12.5)),
        (T::Heading, font_display(26.0)),
        (T::Name("h2".into()), font_display(20.0)),
        (T::Name("h3".into()), font_display(16.5)),
        (T::Name("label".into()), font_semi(11.5)),
        (T::Name("metric".into()), font_mono(36.0)),
        (T::Name("metric_sm".into()), font_mono(24.0)),
        (T::Name("tiny".into()), font_semi(10.5)),
    ];
    style.text_styles = styles.into_iter().collect();

    let v = &mut style.visuals;
    *v = if palette.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.override_text_color = Some(palette.ink);
    v.panel_fill = palette.bg;
    v.window_fill = palette.surface;
    v.extreme_bg_color = palette.surface_sunken;
    v.faint_bg_color = palette.surface_alt;
    v.code_bg_color = palette.surface_alt;
    v.window_stroke = Stroke::new(1.0, palette.line);
    v.window_corner_radius = CornerRadius::same(18);
    v.window_shadow = palette.card_shadow();
    v.popup_shadow = palette.card_shadow();
    v.menu_corner_radius = CornerRadius::same(14);
    v.selection.bg_fill = Palette::alpha(palette.accent, 120);
    v.selection.stroke = Stroke::new(1.5, palette.accent);
    v.hyperlink_color = palette.blue;
    v.warn_fg_color = palette.warning;
    v.error_fg_color = palette.danger;
    v.text_cursor.stroke = Stroke::new(1.5, palette.ink);
    v.striped = true;
    v.slider_trailing_fill = true;

    v.widgets.noninteractive.bg_fill = palette.surface;
    v.widgets.noninteractive.weak_bg_fill = palette.surface_alt;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.line);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.ink_soft);
    v.widgets.noninteractive.corner_radius = CornerRadius::same(10);

    v.widgets.inactive.bg_fill = palette.surface_alt;
    v.widgets.inactive.weak_bg_fill = palette.surface_alt;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.line);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.ink_soft);
    v.widgets.inactive.corner_radius = CornerRadius::same(10);

    v.widgets.hovered.bg_fill = palette.surface;
    v.widgets.hovered.weak_bg_fill = palette.surface;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, palette.line_strong);
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, palette.ink);
    v.widgets.hovered.corner_radius = CornerRadius::same(10);
    v.widgets.hovered.expansion = 0.0;

    v.widgets.active.bg_fill = palette.accent_soft;
    v.widgets.active.weak_bg_fill = palette.accent_soft;
    v.widgets.active.bg_stroke = Stroke::new(1.0, palette.accent);
    v.widgets.active.fg_stroke = Stroke::new(1.5, palette.ink);
    v.widgets.active.corner_radius = CornerRadius::same(10);

    v.widgets.open.bg_fill = palette.surface_alt;
    v.widgets.open.weak_bg_fill = palette.surface_alt;
    v.widgets.open.bg_stroke = Stroke::new(1.0, palette.line_strong);
    v.widgets.open.fg_stroke = Stroke::new(1.0, palette.ink);
    v.widgets.open.corner_radius = CornerRadius::same(10);

    style
}

/// Install both light and dark styles, select the requested mode.
pub fn install_style(ctx: &egui::Context, mode: ThemeMode) {
    ctx.set_style_of(egui::Theme::Light, base_style(&Palette::light()));
    ctx.set_style_of(egui::Theme::Dark, base_style(&Palette::dark()));
    ctx.set_theme(match mode {
        ThemeMode::Light => egui::ThemePreference::Light,
        ThemeMode::Dark => egui::ThemePreference::Dark,
    });
}

// ---------------------------------------------------------------------------
// Icons (hand-drawn vectors so no asset font is needed)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Dashboard,
    Ride,
    Activities,
    Routes,
    Devices,
    Settings,
    Sun,
    Moon,
    Play,
    Pause,
    Stop,
    Plus,
    Download,
    Upload,
    Bluetooth,
    Heart,
    Bolt,
    Mountain,
    ChevronRight,
    Close,
    Trash,
    Check,
    Search,
    Refresh,
    Back,
    Forward,
    Map,
    Timer,
    Flame,
    TrendUp,
    Info,
    Lock,
}

/// Draw an icon centred in `rect`.
pub fn paint_icon(ui: &Ui, rect: Rect, icon: Icon, color: Color32) {
    let painter = ui.painter();
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.5;
    let s = Stroke::new(1.7, color);
    let s2 = Stroke::new(1.4, color);
    let p = |x: f32, y: f32| pos2(c.x + x * r, c.y + y * r);

    match icon {
        Icon::Dashboard => {
            for (x, y, w, h) in [
                (-0.75, -0.75, 0.55, 0.55),
                (0.2, -0.75, 0.55, 0.55),
                (-0.75, 0.2, 0.55, 0.55),
                (0.2, 0.2, 0.55, 0.55),
            ] {
                painter.rect_stroke(
                    Rect::from_min_size(p(x, y), Vec2::new(w * r, h * r)),
                    CornerRadius::same(2),
                    s2,
                    egui::StrokeKind::Middle,
                );
            }
        }
        Icon::Ride => {
            painter.circle_stroke(p(-0.62, 0.42), 0.32 * r, s2);
            painter.circle_stroke(p(0.62, 0.42), 0.32 * r, s2);
            painter.add(egui::Shape::line(
                vec![p(-0.62, 0.42), p(-0.15, -0.25), p(0.1, 0.42), p(0.62, 0.42)],
                s2,
            ));
            painter.line_segment([p(-0.35, -0.55), p(0.05, -0.55)], s2);
            painter.line_segment([p(-0.15, -0.55), p(-0.15, -0.25)], s2);
        }
        Icon::Activities => {
            painter.add(egui::Shape::line(
                vec![
                    p(-0.85, 0.15),
                    p(-0.45, 0.15),
                    p(-0.2, -0.55),
                    p(0.15, 0.65),
                    p(0.4, 0.15),
                    p(0.85, 0.15),
                ],
                Stroke::new(1.9, color),
            ));
        }
        Icon::Routes => {
            painter.circle_stroke(p(-0.55, -0.55), 0.18 * r, s2);
            painter.circle_stroke(p(0.55, 0.55), 0.18 * r, s2);
            let pts = (0..24)
                .map(|i| {
                    let t = i as f32 / 23.0;
                    let x = -0.55 + 1.1 * t;
                    let y = 0.55 - 1.1 * t + (t * std::f32::consts::TAU).sin() * 0.22;
                    p(x, y)
                })
                .collect();
            painter.add(egui::Shape::line(pts, s2));
        }
        Icon::Devices => {
            painter.rect_stroke(
                Rect::from_center_size(p(0.0, 0.0), Vec2::new(0.95 * r, 1.55 * r)),
                CornerRadius::same(3),
                s,
                egui::StrokeKind::Middle,
            );
            painter.circle_filled(p(0.0, 0.55), 0.07 * r, color);
        }
        Icon::Settings => {
            painter.circle_stroke(c, 0.36 * r, s2);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                painter.line_segment(
                    [
                        pos2(c.x + a.cos() * 0.62 * r, c.y + a.sin() * 0.62 * r),
                        pos2(c.x + a.cos() * 0.85 * r, c.y + a.sin() * 0.85 * r),
                    ],
                    Stroke::new(2.0, color),
                );
            }
        }
        Icon::Sun => {
            painter.circle_stroke(c, 0.34 * r, s);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                painter.line_segment(
                    [
                        pos2(c.x + a.cos() * 0.58 * r, c.y + a.sin() * 0.58 * r),
                        pos2(c.x + a.cos() * 0.84 * r, c.y + a.sin() * 0.84 * r),
                    ],
                    s2,
                );
            }
        }
        Icon::Moon => {
            painter.circle_filled(p(-0.05, 0.0), 0.68 * r, color);
            painter.circle_filled(p(0.28, -0.15), 0.6 * r, ui.visuals().panel_fill);
        }
        Icon::Play => {
            painter.add(egui::Shape::convex_polygon(
                vec![p(-0.38, -0.6), p(0.62, 0.0), p(-0.38, 0.6)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Pause => {
            painter.rect_filled(
                Rect::from_center_size(p(-0.28, 0.0), Vec2::new(0.26 * r, 1.1 * r)),
                CornerRadius::same(1),
                color,
            );
            painter.rect_filled(
                Rect::from_center_size(p(0.28, 0.0), Vec2::new(0.26 * r, 1.1 * r)),
                CornerRadius::same(1),
                color,
            );
        }
        Icon::Stop => {
            painter.rect_filled(
                Rect::from_center_size(c, Vec2::splat(0.82 * r)),
                CornerRadius::same(2),
                color,
            );
        }
        Icon::Plus => {
            painter.line_segment([p(-0.55, 0.0), p(0.55, 0.0)], s);
            painter.line_segment([p(0.0, -0.55), p(0.0, 0.55)], s);
        }
        Icon::Download => {
            painter.line_segment([p(0.0, -0.65), p(0.0, 0.3)], s);
            painter.add(egui::Shape::line(
                vec![p(-0.38, -0.05), p(0.0, 0.35), p(0.38, -0.05)],
                s,
            ));
            painter.line_segment([p(-0.6, 0.68), p(0.6, 0.68)], s);
        }
        Icon::Upload => {
            painter.line_segment([p(0.0, 0.7), p(0.0, -0.3)], s);
            painter.add(egui::Shape::line(
                vec![p(-0.38, 0.05), p(0.0, -0.35), p(0.38, 0.05)],
                s,
            ));
            painter.line_segment([p(-0.6, 0.75), p(0.6, 0.75)], s);
        }
        Icon::Bluetooth => {
            painter.add(egui::Shape::line(
                vec![
                    p(-0.25, -0.6),
                    p(0.4, -0.1),
                    p(-0.4, 0.45),
                    p(0.4, 0.1),
                    p(-0.25, 0.6),
                    p(-0.25, -0.6),
                ],
                s,
            ));
        }
        Icon::Heart => {
            let pts = (0..40)
                .map(|i| {
                    let t = i as f32 / 39.0 * std::f32::consts::TAU;
                    let x = 16.0 * t.sin().powi(3) / 18.0;
                    let y = -(13.0 * t.cos() - 5.0 * (2.0 * t).cos()
                        - 2.0 * (3.0 * t).cos()
                        - (4.0 * t).cos())
                        / 18.0;
                    p(x, y)
                })
                .collect();
            painter.add(egui::Shape::line(pts, Stroke::new(1.6, color)));
        }
        Icon::Bolt => {
            painter.add(egui::Shape::convex_polygon(
                vec![p(0.18, -0.8), p(-0.48, 0.12), p(-0.02, 0.12), p(-0.2, 0.8), p(0.5, -0.16), p(0.02, -0.16)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Mountain => {
            painter.add(egui::Shape::line(
                vec![p(-0.85, 0.6), p(-0.25, -0.5), p(0.15, 0.1), p(0.45, -0.28), p(0.85, 0.6)],
                s,
            ));
            painter.line_segment([p(-0.42, 0.15), p(-0.08, 0.15)], s2);
        }
        Icon::ChevronRight => {
            painter.add(egui::Shape::line(
                vec![p(-0.25, -0.5), p(0.3, 0.0), p(-0.25, 0.5)],
                s,
            ));
        }
        Icon::Back => {
            painter.add(egui::Shape::line(
                vec![p(0.3, -0.5), p(-0.25, 0.0), p(0.3, 0.5)],
                s,
            ));
        }
        Icon::Forward => {
            painter.add(egui::Shape::line(
                vec![p(-0.3, -0.5), p(0.25, 0.0), p(-0.3, 0.5)],
                s,
            ));
        }
        Icon::Close => {
            painter.line_segment([p(-0.45, -0.45), p(0.45, 0.45)], s);
            painter.line_segment([p(0.45, -0.45), p(-0.45, 0.45)], s);
        }
        Icon::Trash => {
            painter.rect_stroke(
                Rect::from_min_max(p(-0.45, -0.3), p(0.45, 0.65)),
                CornerRadius::same(2),
                s2,
                egui::StrokeKind::Middle,
            );
            painter.line_segment([p(-0.65, -0.45), p(0.65, -0.45)], s2);
            painter.line_segment([p(-0.2, -0.65), p(0.2, -0.65)], s2);
            painter.line_segment([p(-0.16, -0.05), p(-0.16, 0.4)], s2);
            painter.line_segment([p(0.16, -0.05), p(0.16, 0.4)], s2);
        }
        Icon::Check => {
            painter.add(egui::Shape::line(
                vec![p(-0.55, 0.02), p(-0.12, 0.45), p(0.6, -0.45)],
                s,
            ));
        }
        Icon::Search => {
            painter.circle_stroke(p(-0.15, -0.15), 0.45 * r, s2);
            painter.line_segment([p(0.2, 0.2), p(0.62, 0.62)], Stroke::new(2.0, color));
        }
        Icon::Refresh => {
            let pts = (0..30)
                .map(|i| {
                    let a = -1.2 + i as f32 / 29.0 * 5.4;
                    p(a.cos() * 0.62, a.sin() * 0.62)
                })
                .collect();
            painter.add(egui::Shape::line(pts, s2));
            painter.add(egui::Shape::line(
                vec![p(0.15, -0.78), p(0.62, -0.58), p(0.35, -0.15)],
                s2,
            ));
        }
        Icon::Map => {
            painter.add(egui::Shape::line(
                vec![p(-0.8, -0.55), p(-0.25, -0.72), p(0.25, -0.5), p(0.8, -0.68)],
                s2,
            ));
            painter.add(egui::Shape::line(
                vec![p(-0.8, 0.55), p(-0.25, 0.38), p(0.25, 0.6), p(0.8, 0.42)],
                s2,
            ));
            painter.line_segment([p(-0.8, -0.55), p(-0.8, 0.55)], s2);
            painter.line_segment([p(0.8, -0.68), p(0.8, 0.42)], s2);
            painter.line_segment([p(-0.25, -0.72), p(-0.25, 0.38)], s2);
            painter.line_segment([p(0.25, -0.5), p(0.25, 0.6)], s2);
        }
        Icon::Timer => {
            painter.circle_stroke(c, 0.62 * r, s2);
            painter.line_segment([c, p(0.0, -0.35)], s);
            painter.line_segment([c, p(0.3, 0.18)], s2);
            painter.line_segment([p(-0.25, -0.85), p(0.25, -0.85)], s2);
        }
        Icon::Flame => {
            let pts = vec![
                p(0.0, 0.75),
                p(-0.5, 0.2),
                p(-0.35, -0.25),
                p(-0.05, -0.15),
                p(0.05, -0.8),
                p(0.45, -0.15),
                p(0.5, 0.3),
            ];
            painter.add(egui::Shape::line(pts, s2));
        }
        Icon::TrendUp => {
            painter.add(egui::Shape::line(
                vec![p(-0.75, 0.55), p(-0.2, 0.0), p(0.15, 0.3), p(0.75, -0.45)],
                s,
            ));
            painter.add(egui::Shape::line(
                vec![p(0.25, -0.5), p(0.8, -0.5), p(0.8, 0.02)],
                s2,
            ));
        }
        Icon::Info => {
            painter.circle_stroke(c, 0.72 * r, s2);
            painter.circle_filled(p(0.0, -0.38), 0.07 * r, color);
            painter.line_segment([p(0.0, -0.1), p(0.0, 0.42)], s);
        }
        Icon::Lock => {
            painter.rect_stroke(
                Rect::from_min_max(p(-0.5, -0.05), p(0.5, 0.6)),
                CornerRadius::same(3),
                s2,
                egui::StrokeKind::Middle,
            );
            painter.add(egui::Shape::line(
                vec![p(-0.3, -0.05), p(-0.3, -0.4), p(0.0, -0.66), p(0.3, -0.4), p(0.3, -0.05)],
                s2,
            ));
        }
    }
}

pub fn icon_button(ui: &mut Ui, icon: Icon, palette: &Palette, size: f32, active: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if ui.is_rect_visible(rect) {
        if active || response.hovered() {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(10),
                if active {
                    palette.accent_soft
                } else {
                    palette.surface_alt
                },
            );
        }
        let color = if active { palette.ink } else { palette.muted };
        paint_icon(ui, rect.shrink(size * 0.27), icon, color);
    }
    response
}

/// Navigation row for the left rail.
pub fn nav_item(ui: &mut Ui, icon: Icon, label: &str, selected: bool, palette: &Palette) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
    if ui.is_rect_visible(rect) {
        if selected {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(12), palette.accent);
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(12), palette.surface_alt);
        }
        let color = if selected {
            palette.accent_ink
        } else if response.hovered() {
            palette.ink
        } else {
            palette.muted
        };
        let icon_rect = Rect::from_center_size(
            pos2(rect.left() + 21.0, rect.center().y),
            Vec2::splat(20.0),
        );
        paint_icon(ui, icon_rect, icon, color);
        ui.painter().text(
            pos2(rect.left() + 42.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            font_semi(if selected { 14.5 } else { 14.0 }),
            color,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Logo mark: volt square with a custom V.
pub fn logo(ui: &mut Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same((size * 0.29) as u8), Color32::from_rgb(0xC8, 0xF1, 0x35));
    let c = rect.center();
    let w = size * 0.5;
    let h = size * 0.42;
    let stroke = Stroke::new(size * 0.135, Color32::from_rgb(0x0B, 0x12, 0x20));
    let a = pos2(c.x - w * 0.5, c.y - h * 0.5);
    let b = pos2(c.x + w * 0.5, c.y - h * 0.5);
    let d = pos2(c.x + w * 0.05, c.y + h * 0.5);
    painter.line_segment([a, d], stroke);
    painter.line_segment([b, d], stroke);
}

// ---------------------------------------------------------------------------
// Cards and small components
// ---------------------------------------------------------------------------

pub fn card(palette: &Palette) -> Frame {
    Frame::new()
        .fill(palette.surface)
        .stroke(Stroke::new(1.0, palette.line))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::symmetric(20, 18))
        .shadow(palette.card_shadow())
}

pub fn soft_card(palette: &Palette) -> Frame {
    Frame::new()
        .fill(palette.surface_alt)
        .stroke(Stroke::new(1.0, palette.line))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::symmetric(16, 14))
}

pub fn section_header(ui: &mut Ui, title: &str, subtitle: Option<&str>, palette: &Palette) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(title)
                    .font(font_display(17.0))
                    .color(palette.ink),
            );
            if let Some(subtitle) = subtitle {
                ui.add_space(1.0);
                ui.label(
                    RichText::new(subtitle)
                        .font(font_body(12.0))
                        .color(palette.muted),
                );
            }
        });
    });
}

pub fn page_header(
    ui: &mut Ui,
    palette: &Palette,
    eyebrow: &str,
    title: &str,
    subtitle: &str,
) {
    ui.label(
        RichText::new(eyebrow.to_ascii_uppercase())
            .font(font_semi(10.5))
            .color(palette.faint),
    );
    ui.add_space(3.0);
    ui.label(
        RichText::new(title)
            .font(font_display(33.0))
            .color(palette.ink),
    );
    if !subtitle.is_empty() {
        ui.add_space(3.0);
        ui.label(
            RichText::new(subtitle)
                .font(font_body(13.5))
                .color(palette.muted),
        );
    }
}

/// A small rounded status pill.
pub fn pill(ui: &mut Ui, text: &str, fg: Color32, bg: Color32) -> Response {
    let font = font_semi(10.5);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, fg);
    let size = vec2(galley.size().x + 14.0, galley.size().y + 6.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same((size.y * 0.5) as u8), bg);
    ui.painter().galley(
        pos2(rect.center().x - galley.size().x * 0.5, rect.center().y - galley.size().y * 0.5),
        galley,
        fg,
    );
    response
}

pub fn dot(ui: &mut Ui, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    ui.painter().circle_filled(rect.center(), size * 0.5, color);
}

pub fn primary_button(ui: &mut Ui, text: &str, palette: &Palette) -> Response {
    ui.add(
        egui::Button::new(
            RichText::new(text)
                .font(font_semi(13.5))
                .color(palette.accent_ink),
        )
        .fill(palette.accent)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(10))
        .min_size(vec2(0.0, 36.0)),
    )
}

pub fn accent_button(ui: &mut Ui, icon: Icon, text: &str, palette: &Palette) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font_semi(13.5), palette.accent_ink);
    let size = vec2(galley.size().x + 46.0, 38.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if response.hovered() {
            palette.accent.gamma_multiply(0.92)
        } else {
            palette.accent
        };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(10), fill);
        let icon_rect =
            Rect::from_center_size(pos2(rect.left() + 20.0, rect.center().y), Vec2::splat(15.0));
        paint_icon(ui, icon_rect, icon, palette.accent_ink);
        ui.painter().galley(
            pos2(
                rect.left() + 34.0,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            palette.accent_ink,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn ghost_button(ui: &mut Ui, icon: Option<Icon>, text: &str, palette: &Palette) -> Response {
    let button = egui::Button::new(
        RichText::new(text)
            .font(font_semi(13.0))
            .color(palette.ink_soft),
    )
    .fill(palette.surface)
    .stroke(Stroke::new(1.0, palette.line))
    .corner_radius(CornerRadius::same(10))
    .min_size(vec2(0.0, 36.0));
    let response = ui.add(button);
    if let Some(icon) = icon {
        if ui.is_rect_visible(response.rect) {
            let icon_rect = Rect::from_center_size(
                pos2(response.rect.left() + 18.0, response.rect.center().y),
                Vec2::splat(14.0),
            );
            paint_icon(ui, icon_rect, icon, palette.muted);
        }
    }
    response
}

pub fn segmented(
    ui: &mut Ui,
    palette: &Palette,
    options: &[&str],
    selected: usize,
    width: f32,
) -> Option<usize> {
    let mut clicked = None;
    let (rect, _) = ui.allocate_exact_size(vec2(width, 34.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(9), palette.surface_alt);
    ui.painter()
        .rect_stroke(rect, CornerRadius::same(9), Stroke::new(1.0, palette.line), egui::StrokeKind::Inside);
    let seg_w = rect.width() / options.len() as f32;
    for (i, option) in options.iter().enumerate() {
        let seg = Rect::from_min_size(
            pos2(rect.left() + i as f32 * seg_w + 3.0, rect.top() + 3.0),
            vec2(seg_w - 6.0, rect.height() - 6.0),
        );
        let response = ui.interact(seg, ui.id().with(("seg", i, width as i32)), Sense::click());
        if i == selected {
            ui.painter()
                .rect_filled(seg, CornerRadius::same(7), palette.surface);
            ui.painter().rect_stroke(
                seg,
                CornerRadius::same(7),
                Stroke::new(1.0, palette.line),
                egui::StrokeKind::Inside,
            );
        } else if response.hovered() {
            ui.painter()
                .rect_filled(seg, CornerRadius::same(7), Palette::alpha(palette.surface, 140));
        }
        let color = if i == selected { palette.ink } else { palette.muted };
        ui.painter().text(
            seg.center(),
            Align2::CENTER_CENTER,
            *option,
            font_semi(12.5),
            color,
        );
        if response.clicked() {
            clicked = Some(i);
        }
        response.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    clicked
}

/// Clickable activity / route row with a hover surface.
pub fn list_row(
    ui: &mut Ui,
    palette: &Palette,
    selected: bool,
    height: f32,
    add: impl FnOnce(&mut Ui, &Palette),
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if selected {
            palette.accent_soft
        } else if response.hovered() {
            palette.surface_alt
        } else {
            Color32::TRANSPARENT
        };
        if fill != Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, CornerRadius::same(12), fill);
        }
        if selected {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(12),
                Stroke::new(1.0, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(14.0, 6.0))));
        add(&mut child, palette);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Key/value stat used inside cards.
pub fn stat_line(ui: &mut Ui, palette: &Palette, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .font(font_body(12.5))
                .color(palette.muted),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(value)
                    .font(font_mono(12.5))
                    .color(palette.ink),
            );
        });
    });
}

pub fn kv(ui: &mut Ui, palette: &Palette, label: &str, value: &str, value_color: Option<Color32>) {
    ui.vertical(|ui| {
        ui.label(
            RichText::new(label)
                .font(font_semi(10.5))
                .color(palette.faint),
        );
        ui.add_space(1.0);
        ui.label(
            RichText::new(value)
                .font(font_mono(17.0))
                .color(value_color.unwrap_or(palette.ink)),
        );
    });
}

pub fn divider(ui: &mut Ui, palette: &Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::ZERO, palette.line);
}

pub fn empty_state(ui: &mut Ui, palette: &Palette, icon: Icon, title: &str, hint: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(18.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(12),
            palette.surface_alt,
        );
        paint_icon(ui, rect.shrink(12.0), icon, palette.faint);
        ui.add_space(8.0);
        ui.label(
            RichText::new(title)
                .font(font_semi(13.5))
                .color(palette.ink_soft),
        );
        ui.label(
            RichText::new(hint)
                .font(font_body(12.0))
                .color(palette.muted),
        );
        ui.add_space(18.0);
    });
}

/// Simple horizontal progress/zone bar.
pub fn progress_bar(ui: &mut Ui, palette: &Palette, fraction: f32, color: Color32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same((height * 0.5) as u8), palette.surface_sunken);
    let w = rect.width() * fraction.clamp(0.0, 1.0);
    if w > 0.5 {
        let bar = Rect::from_min_size(rect.min, vec2(w, rect.height()));
        ui.painter()
            .rect_filled(bar, CornerRadius::same((height * 0.5) as u8), color);
    }
}

/// A tiny sparkline drawn directly on the painter.
pub fn sparkline(ui: &mut Ui, values: &[f32], color: Color32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    if values.len() < 2 {
        return;
    }
    let min = values.iter().copied().fold(f32::INFINITY, f32::min);
    let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let span = (max - min).max(1.0);
    let pts: Vec<Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            pos2(
                rect.left() + rect.width() * i as f32 / (values.len() - 1) as f32,
                rect.bottom() - (v - min) / span * rect.height() * 0.86 - rect.height() * 0.07,
            )
        })
        .collect();
    ui.painter()
        .add(egui::Shape::line(pts, Stroke::new(1.6, color)));
}

/// Horizontal zone distribution bar from six fractions.
pub fn zone_bar(ui: &mut Ui, palette: &Palette, fractions: &[f32; 6], height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let total: f32 = fractions.iter().sum();
    if total <= 0.0 {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(3), palette.surface_sunken);
        return;
    }
    let mut x = rect.left();
    for (i, f) in fractions.iter().enumerate() {
        let w = rect.width() * (f / total);
        if w <= 0.0 {
            continue;
        }
        let seg = Rect::from_min_size(pos2(x, rect.top()), vec2(w, rect.height()));
        ui.painter()
            .rect_filled(seg, CornerRadius::same(3), palette.zone(i + 1));
        x += w;
    }
}

pub fn label(ui: &mut Ui, text: impl Into<String>, palette: &Palette, size: f32, color: Color32) {
    ui.label(RichText::new(text).font(font_body(size)).color(color));
}
