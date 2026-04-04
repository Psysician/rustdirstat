#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Binary entry point. Parses CLI arguments, initialises tracing, loads
//! persistent configuration, and launches the eframe window. All scanning
//! and rendering logic lives in rds-scanner and rds-gui.

use clap::Parser;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use rds_core::AppConfig;
use rds_core::scan::{ScanConfig, ScanEvent};
use tracing_subscriber::EnvFilter;

/// Command-line arguments. `path` is the root directory passed to the scanner.
#[derive(Parser)]
#[command(name = "rustdirstat", about = "Cross-platform disk usage analyzer")]
struct Cli {
    /// Path to scan
    path: Option<PathBuf>,

    /// Run scan without GUI and print stats to stdout
    #[arg(long)]
    scan_only: bool,
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

    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        tracing::warn!(
            "Failed to create config directory {}: {}",
            parent.display(),
            e
        );
        return;
    }

    if let Err(e) = std::fs::write(path, contents) {
        tracing::warn!("Failed to write config to {}: {}", path.display(), e);
    }
}

/// Runs scan-only mode: scans the given path without launching the GUI,
/// prints scan stats to stdout, and exits.
fn run_scan_only(path: PathBuf, app_config: &AppConfig) {
    let config = ScanConfig {
        root: path,
        hash_duplicates: false,
        exclude_patterns: app_config.exclude_patterns.clone(),
        follow_symlinks: app_config.follow_symlinks,
        ..ScanConfig::default()
    };

    let (tx, rx) = crossbeam_channel::bounded::<ScanEvent>(4096);
    let cancel = Arc::new(AtomicBool::new(false));

    let handle = rds_scanner::Scanner::scan(config, tx, cancel);

    loop {
        match rx.recv() {
            Ok(ScanEvent::ScanComplete { stats }) => {
                let duration_secs = stats.duration_ms as f64 / 1000.0;
                let files_per_sec = if duration_secs > 0.0 {
                    stats.total_files as f64 / duration_secs
                } else {
                    0.0
                };

                println!("Scan complete:");
                println!("  Files:     {}", stats.total_files);
                println!("  Dirs:      {}", stats.total_dirs);
                println!("  Bytes:     {}", stats.total_bytes);
                println!("  Duration:  {:.2}s", duration_secs);
                println!("  Files/sec: {:.0}", files_per_sec);
                if stats.errors > 0 {
                    println!("  Errors:    {}", stats.errors);
                }
                break;
            }
            Ok(_) => {}
            Err(_) => {
                eprintln!("Scanner channel closed unexpectedly");
                break;
            }
        }
    }

    let _ = handle.join();
}

/// Initialises tracing, parses CLI args, loads config, and runs either
/// scan-only mode or the native eframe event loop. Default window size is
/// 1024x768 with a 640x480 minimum to ensure the 3-panel layout remains usable.
/// Returns eframe::Result so OS-level window errors propagate to the process exit code.
fn main() -> eframe::Result {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();

    let (config, config_path) = load_config();

    if cli.scan_only {
        let path = cli.path.unwrap_or_else(|| PathBuf::from("."));
        run_scan_only(path, &config);
        return Ok(());
    }

    let icon_image = image::load_from_memory(include_bytes!("../assets/icon-256.png"))
        .expect("embedded icon is valid PNG")
        .to_rgba8();
    let (icon_w, icon_h) = icon_image.dimensions();
    let icon_data = egui::IconData {
        rgba: icon_image.into_raw(),
        width: icon_w,
        height: icon_h,
    };

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_min_inner_size([640.0, 480.0])
            .with_icon(Arc::new(icon_data)),
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

#[cfg(test)]
mod tests {
    use rds_core::{AppConfig, ColorScheme, SortOrder};

    #[test]
    fn toml_roundtrip_all_fields() {
        let config = AppConfig {
            exclude_patterns: vec!["*.log".to_string(), ".git".to_string()],
            custom_commands: vec![rds_core::CustomCommand {
                name: "Open Editor".to_string(),
                template: "code {path}".to_string(),
            }],
            color_scheme: ColorScheme::Default,
            default_sort: SortOrder::NameAsc,
            recent_paths: vec![
                std::path::PathBuf::from("/tmp/test"),
                std::path::PathBuf::from("/home/user"),
            ],
            max_recent_paths: 20,
            follow_symlinks: true,
        };

        let toml_str = toml::to_string(&config).expect("serialize to TOML");
        let restored: AppConfig = toml::from_str(&toml_str).expect("deserialize from TOML");

        assert_eq!(restored.exclude_patterns, config.exclude_patterns);
        assert_eq!(restored.custom_commands.len(), 1);
        assert_eq!(restored.custom_commands[0].name, "Open Editor");
        assert_eq!(restored.custom_commands[0].template, "code {path}");
        assert_eq!(restored.color_scheme, ColorScheme::Default);
        assert_eq!(restored.default_sort, SortOrder::NameAsc);
        assert_eq!(restored.recent_paths, config.recent_paths);
        assert_eq!(restored.max_recent_paths, 20);
        assert!(restored.follow_symlinks);
    }

    #[test]
    fn toml_missing_fields_use_defaults() {
        let partial = r#"color_scheme = "default""#;
        let config: AppConfig = toml::from_str(partial).expect("deserialize partial TOML");

        assert_eq!(config.color_scheme, ColorScheme::Default);
        assert_eq!(config.exclude_patterns, AppConfig::default().exclude_patterns);
        assert!(config.custom_commands.is_empty());
        assert_eq!(config.default_sort, SortOrder::SizeDesc);
        assert!(config.recent_paths.is_empty());
        assert_eq!(config.max_recent_paths, 10);
        assert!(!config.follow_symlinks);
    }

    #[test]
    fn toml_empty_string_yields_default() {
        let config: AppConfig = toml::from_str("").expect("deserialize empty TOML");
        let default = AppConfig::default();

        assert_eq!(config.color_scheme, default.color_scheme);
        assert_eq!(config.default_sort, default.default_sort);
        assert_eq!(config.max_recent_paths, default.max_recent_paths);
        assert_eq!(config.follow_symlinks, default.follow_symlinks);
        assert_eq!(config.exclude_patterns, default.exclude_patterns);
        assert_eq!(config.recent_paths, default.recent_paths);
        assert!(config.custom_commands.is_empty());
    }

    #[test]
    fn toml_invalid_returns_error() {
        let result = toml::from_str::<AppConfig>("[[[bad");
        assert!(result.is_err());
    }
}
