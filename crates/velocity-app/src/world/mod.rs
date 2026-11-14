//! VELOCITY's virtual 3D world.
//!
//! The world is built from the selected route's real geography: elevation comes
//! from streamed DEM tiles, the road follows the recorded GPS track, and terrain,
//! vegetation, villages and water are generated around the rider as they move.
//! Rendering happens on the UI thread inside an egui paint callback (the app is
//! single-threaded by design); the expensive work — tile download, decoding and
//! height sampling — is either backgrounded or streamed in small per-frame
//! budgets.

pub mod dem;
pub mod geometry;
pub mod math;
pub mod renderer;
pub mod shaders;
pub mod terrain;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui::{self, PaintCallbackInfo};
use eframe::egui_glow;
use velocity_core::route::Route;

pub use dem::DemStatus;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CameraMode {
    #[default]
    Chase,
    Close,
    Hood,
    Drone,
}

impl CameraMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Chase => "Chase",
            Self::Close => "Close",
            Self::Hood => "Onboard",
            Self::Drone => "Aerial",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Chase => "chase",
            Self::Close => "close",
            Self::Hood => "hood",
            Self::Drone => "drone",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "chase" => Some(Self::Chase),
            "close" => Some(Self::Close),
            "hood" => Some(Self::Hood),
            "drone" => Some(Self::Drone),
            _ => None,
        }
    }

    pub fn all() -> [Self; 4] {
        [Self::Chase, Self::Close, Self::Hood, Self::Drone]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum QualityPreset {
    #[default]
    Auto,
    UltraLow,
    Low,
    Medium,
    High,
    Ultra,
}

impl QualityPreset {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::UltraLow => "Ultra low",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Ultra => "Ultra",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::UltraLow => "ultra-low",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Ultra => "ultra",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "auto" => Some(Self::Auto),
            "ultra-low" => Some(Self::UltraLow),
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "ultra" => Some(Self::Ultra),
            _ => None,
        }
    }

    pub fn all() -> [Self; 6] {
        [
            Self::Auto,
            Self::UltraLow,
            Self::Low,
            Self::Medium,
            Self::High,
            Self::Ultra,
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuClass {
    Software,
    Integrated,
    Discrete,
}

impl GpuClass {
    /// Best-effort classification of the OpenGL renderer string.
    pub fn classify(renderer: &str) -> Self {
        let name = renderer.to_ascii_lowercase();
        const DISCRETE: [&str; 12] = [
            "rtx", "gtx", "radeon rx", "radeon pro", "arc a", "quadro", "geforce", "rx 5",
            "rx 6", "rx 7", "vega", "apple m",
        ];
        const SOFTWARE: [&str; 4] = ["llvmpipe", "softpipe", "swrast", "software"];
        if SOFTWARE.iter().any(|needle| name.contains(needle)) {
            Self::Software
        } else if DISCRETE.iter().any(|needle| name.contains(needle)) {
            Self::Discrete
        } else {
            Self::Integrated
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Software => "software rasterizer",
            Self::Integrated => "integrated graphics",
            Self::Discrete => "discrete GPU",
        }
    }
}

/// Resolved render settings for one frame.
#[derive(Clone, Copy, Debug)]
pub struct Quality {
    pub label: &'static str,
    pub view_distance: f32,
    pub tree_density: f32,
    pub tree_distance: f32,
    pub grass: bool,
    pub grass_density: f32,
    pub fxaa: bool,
    pub scale: f32,
    pub shadows: bool,
    pub shadow_radius: f32,
}

impl Quality {
    pub fn resolve(preset: QualityPreset, class: GpuClass) -> Self {
        let preset = match preset {
            QualityPreset::Auto => match class {
                GpuClass::Software => QualityPreset::UltraLow,
                GpuClass::Integrated => QualityPreset::Low,
                GpuClass::Discrete => QualityPreset::Medium,
            },
            explicit => explicit,
        };
        let (
            label,
            view_distance,
            tree_density,
            tree_distance,
            grass,
            grass_density,
            fxaa,
            scale,
            shadows,
            shadow_radius,
        ) = match preset {
            QualityPreset::UltraLow | QualityPreset::Auto => (
                "ultra low", 420.0, 0.55, 260.0, false, 0.0, false, 0.6, false, 0.0,
            ),
            QualityPreset::Low => ("low", 680.0, 0.9, 420.0, true, 0.5, false, 0.7, true, 55.0),
            QualityPreset::Medium => (
                "medium", 1_000.0, 1.3, 540.0, true, 1.0, true, 0.8, true, 80.0,
            ),
            QualityPreset::High => (
                "high", 1_400.0, 1.8, 720.0, true, 1.5, true, 0.9, true, 120.0,
            ),
            // Ultra is an explicit maximum-fidelity preset: it pushes draw
            // distance, density and shadow coverage beyond High and may run
            // slowly on weak GPUs, but it never auto-downgrades.
            QualityPreset::Ultra => (
                "ultra", 2_600.0, 3.4, 1_300.0, true, 3.8, true, 1.0, true, 240.0,
            ),
        };
        Self {
            label,
            view_distance,
            tree_density,
            tree_distance,
            grass,
            grass_density,
            fxaa,
            scale,
            shadows,
            shadow_radius,
        }
    }

    pub fn preset_label(self) -> &'static str {
        self.label
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CameraSettings {
    pub mode: CameraMode,
}

/// Everything the world needs to know about how it should look.
#[derive(Clone, Copy, Debug)]
pub struct WorldPrefs {
    pub quality: QualityPreset,
    pub camera: CameraSettings,
    /// Local hour of the day, 0..24.
    pub hour: f32,
    /// Cloud cover, 0..1.
    pub cloud: f32,
    /// Wet road surface, 0..1.
    pub wet: f32,
    /// Lower resolution automatically when frame time slips.
    pub adaptive: bool,
}

impl Default for WorldPrefs {
    fn default() -> Self {
        Self {
            quality: QualityPreset::Auto,
            camera: CameraSettings {
                mode: CameraMode::Chase,
            },
            hour: 9.4,
            cloud: 0.22,
            wet: 0.0,
            adaptive: true,
        }
    }
}

impl WorldPrefs {
    pub fn hour_label(&self) -> String {
        let h = self.hour.floor() as i32;
        let m = ((self.hour.fract() * 60.0).round() as i32).clamp(0, 59);
        format!("{h:02}:{m:02}")
    }
}

/// One frame of ride data fed to the renderer.
#[derive(Clone, Debug, Default)]
pub struct WorldInput {
    pub route: Option<Arc<Route>>,
    pub distance_m: f64,
    pub speed_mps: f32,
    pub cadence_rpm: f32,
    pub power_w: f32,
    pub heart_rate_bpm: f32,
    pub grade_pct: f32,
    pub running: bool,
    pub updated_ms: i64,
}

/// Live diagnostics shown in the world panel and on the settings page.
#[derive(Clone, Debug)]
pub struct WorldStats {
    pub gpu: String,
    pub initialized: bool,
    pub frame_ms: f32,
    pub fps: f32,
    pub chunks: usize,
    pub visible_chunks: usize,
    pub dem_tiles: usize,
    pub dem_status: DemStatus,
    pub terrain_credit: &'static str,
    pub scale: f32,
    pub quality: &'static str,
    pub sun_elevation: f32,
    pub triangles: u32,
    pub draws: u32,
    pub error: Option<String>,
}

impl Default for WorldStats {
    fn default() -> Self {
        Self {
            gpu: "detecting graphics device".to_string(),
            initialized: false,
            frame_ms: 0.0,
            fps: 0.0,
            chunks: 0,
            visible_chunks: 0,
            dem_tiles: 0,
            dem_status: DemStatus::Idle,
            terrain_credit: dem::DEM_ATTRIBUTION,
            scale: 1.0,
            quality: "auto",
            sun_elevation: 0.0,
            triangles: 0,
            draws: 0,
            error: None,
        }
    }
}

struct SceneState {
    renderer: Option<renderer::WorldRenderer>,
    input: WorldInput,
    prefs: WorldPrefs,
    stats: WorldStats,
    cache_dir: PathBuf,
}

/// Cloneable handle shared by the UI (updates) and the paint callback (render).
#[derive(Clone)]
pub struct WorldHandle {
    state: Arc<Mutex<SceneState>>,
}

impl WorldHandle {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            state: Arc::new(Mutex::new(SceneState {
                renderer: None,
                input: WorldInput::default(),
                prefs: WorldPrefs::default(),
                stats: WorldStats::default(),
                cache_dir,
            })),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, SceneState> {
        match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub fn update(&self, input: WorldInput, prefs: WorldPrefs) {
        let mut state = self.lock();
        state.input = input;
        state.prefs = prefs;
    }

    pub fn set_prefs(&self, prefs: WorldPrefs) {
        self.lock().prefs = prefs;
    }

    pub fn stats(&self) -> WorldStats {
        self.lock().stats.clone()
    }

    pub fn orbit(&self, dx: f32, dy: f32) {
        if let Some(renderer) = self.lock().renderer.as_mut() {
            renderer.orbit(dx, dy);
        }
    }

    pub fn zoom(&self, delta: f32) {
        if let Some(renderer) = self.lock().renderer.as_mut() {
            renderer.zoom(delta);
        }
    }

    /// Paint callback installed into the egui painter for the world viewport.
    pub fn callback(&self, rect: egui::Rect) -> egui::PaintCallback {
        let handle = self.clone();
        egui::PaintCallback {
            rect,
            callback: Arc::new(egui_glow::CallbackFn::new(
                move |info: PaintCallbackInfo, painter: &egui_glow::Painter| {
                    handle.paint(painter, info);
                },
            )),
        }
    }

    fn paint(&self, painter: &egui_glow::Painter, info: PaintCallbackInfo) {
        let mut state = self.lock();
        if state.renderer.is_none() {
            let renderer =
                renderer::WorldRenderer::new(painter.gl(), state.cache_dir.clone());
            state.renderer = Some(renderer);
        }
        let SceneState {
            renderer,
            input,
            prefs,
            stats,
            ..
        } = &mut *state;
        if let Some(renderer) = renderer {
            renderer.paint(painter, info, input, prefs, stats);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_quality_scales_with_the_gpu() {
        let software = Quality::resolve(QualityPreset::Auto, GpuClass::Software);
        let integrated = Quality::resolve(QualityPreset::Auto, GpuClass::Integrated);
        let discrete = Quality::resolve(QualityPreset::Auto, GpuClass::Discrete);
        assert!(software.view_distance < integrated.view_distance);
        assert!(integrated.view_distance < discrete.view_distance);
        assert!(!software.grass);
    }

    #[test]
    fn gpu_detection_recognises_common_renderers() {
        assert_eq!(
            GpuClass::classify("llvmpipe (LLVM 15.0.7, 256 bits)"),
            GpuClass::Software
        );
        assert_eq!(
            GpuClass::classify("Mesa Intel(R) HD Graphics 520 (SKL GT2)"),
            GpuClass::Integrated
        );
        assert_eq!(
            GpuClass::classify("NVIDIA GeForce RTX 3060/PCIe/SSE2"),
            GpuClass::Discrete
        );
    }

    #[test]
    fn presets_round_trip_through_keys() {
        for preset in QualityPreset::all() {
            assert_eq!(QualityPreset::from_key(preset.key()), Some(preset));
        }
    }
}
