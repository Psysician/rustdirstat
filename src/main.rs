//! Binary entry point. Parses CLI arguments, initialises tracing, and launches
//! the eframe window. All scanning and rendering logic lives in rds-scanner and rds-gui.

use clap::Parser;
use std::path::PathBuf;

/// Command-line arguments. `path` is the root directory passed to the scanner.
#[derive(Parser)]
#[command(name = "rustdirstat", about = "Cross-platform disk usage analyzer")]
struct Cli {
    /// Path to scan
    path: Option<PathBuf>,
}

/// Initialises tracing, parses CLI args, and runs the native eframe event loop.
/// Default window size is 1024x768; eframe enforces no minimum size, so this provides
/// a usable starting layout for the treemap without requiring the user to resize first.
/// Returns eframe::Result so OS-level window errors propagate to the process exit code.
fn main() -> eframe::Result {
    let _cli = Cli::parse();

    tracing_subscriber::fmt::init();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1024.0, 768.0]),
        ..Default::default()
    };

    eframe::run_native(
        "rustdirstat",
        native_options,
        Box::new(|_cc| Ok(Box::new(rds_gui::RustDirStatApp))),
    )
}
