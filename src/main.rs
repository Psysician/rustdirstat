//! Binary entry point. Parses CLI arguments, initialises tracing, loads
//! persistent configuration, and launches the eframe window. All scanning
//! and rendering logic lives in rds-scanner and rds-gui.

use clap::Parser;
use std::path::{Path, PathBuf};

use rds_core::AppConfig;

/// Command-line arguments. `path` is the root directory passed to the scanner.
#[derive(Parser)]
#[command(name = "rustdirstat", about = "Cross-platform disk usage analyzer")]
struct Cli {
    /// Path to scan
    path: Option<PathBuf>,
}

/// Loads `AppConfig` from the platform config directory (`config_dir/config.toml`).
/// Returns the loaded config and the path where it should be saved. On any error
/// (missing file, parse failure), logs a warning and returns `AppConfig::default()`.
fn load_config() -> (AppConfig, PathBuf) {
    let config_path = directories::ProjectDirs::from("", "", "rustdirstat")
        .map(|dirs| dirs.config_dir().join("config.toml"))
        .unwrap_or_else(|| PathBuf::from("config.toml"));

    let config = match std::fs::read_to_string(&config_path) {
        Ok(contents) => match toml::from_str::<AppConfig>(&contents) {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::warn!("Failed to parse {}: {}", config_path.display(), e);
                AppConfig::default()
            }
        },
        Err(e) => {
            tracing::warn!("Failed to read {}: {}", config_path.display(), e);
            AppConfig::default()
        }
    };

    (config, config_path)
}

/// Serializes `config` as TOML and writes it to `path`, creating parent
/// directories as needed. Logs warnings on failure — never panics.
fn save_config(config: &AppConfig, path: &Path) {
    let contents = match toml::to_string_pretty(config) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Failed to serialize config: {}", e);
            return;
        }
    };

    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!("Failed to create config directory {}: {}", parent.display(), e);
            return;
        }
    }

    if let Err(e) = std::fs::write(path, contents) {
        tracing::warn!("Failed to write config to {}: {}", path.display(), e);
    }
}

/// Initialises tracing, parses CLI args, loads config, and runs the native
/// eframe event loop. Default window size is 1024x768; eframe enforces no
/// minimum size, so this provides a usable starting layout for the treemap
/// without requiring the user to resize first.
/// Returns eframe::Result so OS-level window errors propagate to the process exit code.
fn main() -> eframe::Result {
    let cli = Cli::parse();

    tracing_subscriber::fmt::init();

    let (config, config_path) = load_config();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1024.0, 768.0]),
        ..Default::default()
    };

    eframe::run_native(
        "rustdirstat",
        native_options,
        Box::new(move |_cc| {
            let mut app = rds_gui::RustDirStatApp::new(cli.path, config);
            app.set_config_save_fn(move |cfg: &AppConfig| {
                save_config(cfg, &config_path);
            });
            Ok(Box::new(app))
        }),
    )
}
