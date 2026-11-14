//! VELOCITY — a local-first cycling computer, smart-trainer cockpit and
//! training log. Native Rust, no browser runtime, no account, no cloud.

mod app;
mod charts;
mod theme;
mod views;
mod world;

use app::{AppOptions, VelocityApp};
use std::path::PathBuf;
use theme::ThemeMode;
use views::Page;

const HELP: &str = "\
VELOCITY — local-first cycling computer

USAGE:
    velocity [OPTIONS]

OPTIONS:
    --db <PATH>            SQLite database (default: user data directory)
    --export-dir <PATH>    Where FIT/GPX/TCX exports are written
    --demo                 Seed demo activities and a simulated ride
    --offline              Do not start Bluetooth (UI and simulator still work)
    --screenshot <PATH>    Render a frame to a PNG, then exit
    --screenshot-delay <S> Wait S seconds before capturing (terrain streaming)
    --world-quality <Q>    auto | ultra-low | low | medium | high | ultra
    --page <NAME>          dashboard | ride | activities | routes | devices | settings
    --theme <MODE>         light | dark
    --width <PX>           Window width (default 1920)
    --height <PX>          Window height (default 1200)
    --zoom <FACTOR>        UI zoom factor (default 1.5)
    -h, --help             Show this help
    -V, --version          Show the version

EXAMPLES:
    velocity
    velocity --demo --offline
    velocity --demo --offline --page ride --theme dark --screenshot ride.png
";

fn main() {
    if let Err(err) = run() {
        eprintln!("velocity: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,velocity_app=info"),
    )
    .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{HELP}");
        return Ok(());
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("velocity {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let options = parse_options(&args)?;
    let title = if options.demo {
        "VELOCITY · demo"
    } else {
        "VELOCITY"
    };
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(title)
            .with_app_id("dev.velocity.Velocity")
            .with_inner_size([options.width, options.height])
            .with_min_inner_size([1080.0, 720.0]),
        persist_window: false,
        ..Default::default()
    };

    eframe::run_native(
        "VELOCITY",
        native_options,
        Box::new(move |cc| {
            let app = VelocityApp::new(cc, options).map_err(
                |e| -> Box<dyn std::error::Error + Send + Sync> { format!("{e:#}").into() },
            )?;
            Ok(Box::new(app))
        }),
    )
    .map_err(|err| anyhow::anyhow!("could not start the native window: {err}"))?;
    Ok(())
}

fn parse_options(args: &[String]) -> anyhow::Result<AppOptions> {
    let dirs = directories::ProjectDirs::from("dev", "velocity", "Velocity");
    let data_dir = dirs
        .as_ref()
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let mut options = AppOptions {
        db_path: data_dir.join("velocity.db"),
        export_dir: default_export_dir(&data_dir),
        demo: false,
        offline: false,
        screenshot: None,
        screenshot_delay_ms: 0,
        world_quality: None,
        page: Page::Dashboard,
        theme: None,
        width: 1920.0,
        height: 1200.0,
        zoom: 1.5,
    };

    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        let mut next = |name: &str| -> anyhow::Result<String> {
            i += 1;
            args.get(i)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("{name} requires a value"))
        };
        match arg {
            "--db" => options.db_path = PathBuf::from(next("--db")?),
            "--export-dir" => options.export_dir = PathBuf::from(next("--export-dir")?),
            "--demo" => options.demo = true,
            "--offline" => options.offline = true,
            "--screenshot" => options.screenshot = Some(PathBuf::from(next("--screenshot")?)),
            "--screenshot-delay" => {
                let value = parse_f32(&next("--screenshot-delay")?, "--screenshot-delay")?;
                options.screenshot_delay_ms = (value.max(0.0) * 1000.0) as u64;
            }
            "--world-quality" => {
                let value = next("--world-quality")?;
                options.world_quality = Some(world::QualityPreset::from_key(&value).ok_or_else(|| {
                    anyhow::anyhow!(
                        "unknown world quality “{value}”. Use auto, ultra-low, low, medium, high or ultra."
                    )
                })?);
            }
            "--page" => {
                let value = next("--page")?;
                options.page = Page::from_key(&value).ok_or_else(|| {
                    anyhow::anyhow!(
                        "unknown page “{value}”. Use dashboard, ride, activities, routes, devices or settings."
                    )
                })?;
            }
            "--theme" => {
                let value = next("--theme")?;
                options.theme = Some(match value.to_ascii_lowercase().as_str() {
                    "light" => ThemeMode::Light,
                    "dark" => ThemeMode::Dark,
                    _ => anyhow::bail!("unknown theme “{value}”. Use light or dark."),
                });
            }
            "--width" => options.width = parse_f32(&next("--width")?, "--width")?,
            "--height" => options.height = parse_f32(&next("--height")?, "--height")?,
            "--zoom" => options.zoom = parse_f32(&next("--zoom")?, "--zoom")?,
            other => anyhow::bail!("unknown option “{other}”. Run velocity --help."),
        }
        i += 1;
    }

    if options.screenshot.is_some() {
        // Screenshots should never depend on Bluetooth timing.
        options.offline = true;
    }
    if let Some(path) = options.screenshot.as_ref() {
        if path.extension().and_then(|e| e.to_str()) != Some("png") {
            anyhow::bail!("--screenshot must end in .png");
        }
    }
    if let Some(parent) = options.db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir_all(&options.export_dir)?;
    Ok(options)
}

fn parse_f32(value: &str, name: &str) -> anyhow::Result<f32> {
    value
        .parse::<f32>()
        .map_err(|_| anyhow::anyhow!("{name} expects a number, got “{value}”"))
}

fn default_export_dir(data_dir: &std::path::Path) -> PathBuf {
    directories::UserDirs::new()
        .and_then(|dirs| dirs.document_dir().map(|d| d.join("Velocity Exports")))
        .unwrap_or_else(|| data_dir.join("exports"))
}
