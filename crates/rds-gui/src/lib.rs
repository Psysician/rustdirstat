//! egui application shell with directory picker, scan progress, tree view,
//! treemap, and extension statistics.
//!
//! `RustDirStatApp` owns scan state and renders a 3-panel layout: directory
//! tree (MS6), treemap (MS8), and extension statistics (MS7).
//! The scanner runs on a background thread; events are drained via
//! `try_recv()` each frame (bounded to 100 events to avoid blocking
//! rendering). (ref: DL-003, DL-006)

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use rds_core::scan::{ScanConfig, ScanEvent, ScanStats};
use rds_core::stats::ExtensionStats;
use rds_core::tree::DirTree;

mod ext_stats;
mod tree_view;
mod treemap;

/// Scan lifecycle phases. (ref: DL-004)
enum ScanPhase {
    /// No scan running; waiting for user to pick a directory.
    Idle,
    /// Scanner thread is active; draining events each frame.
    Scanning,
    /// Scanner finished; summary stats available.
    Complete(ScanStats),
}

/// Main application state. Holds the scan lifecycle, DirTree arena being
/// built from scanner events, and display counters for the progress bar.
pub struct RustDirStatApp {
    phase: ScanPhase,
    /// Arena tree built incrementally from NodeDiscovered events.
    /// None until the first scan starts. (ref: DL-005)
    tree: Option<DirTree>,
    receiver: Option<Receiver<ScanEvent>>,
    cancel: Option<Arc<AtomicBool>>,
    scan_handle: Option<JoinHandle<()>>,
    files_scanned: u64,
    bytes_scanned: u64,
    scan_path: Option<PathBuf>,
    /// CLI path consumed on first frame to auto-start scan.
    initial_path: Option<PathBuf>,
    /// Text input for typing a path directly (fallback for WSL2/Wayland
    /// where rfd native dialogs crash the X11 connection).
    path_input: String,
    /// Validation error shown in toolbar when user enters an invalid path.
    path_error: Option<String>,
    /// Running count of ScanError events received during the current scan.
    scan_errors: u64,
    /// When the current scan began, for elapsed time and rate calculation.
    scan_start: Option<Instant>,
    /// When SubtreeStats/ExtensionStats were last recomputed during scan,
    /// for 500ms throttle enforcement.
    last_live_recompute: Option<Instant>,
    /// Tree node count at last recompute, for change detection (ref: DL-009).
    live_node_count: usize,
    /// Cached per-extension statistics, computed after scan completes. (ref: DL-002)
    extension_stats: Option<Vec<ExtensionStats>>,
    /// Expand/collapse state for directory tree panel.
    tree_view_state: tree_view::TreeViewState,
    /// Currently selected node index, shared across panels (MS10).
    selected_node: Option<usize>,
    /// Cached subtree sizes and file counts, computed after scan completes.
    subtree_stats: Option<tree_view::SubtreeStats>,
    /// Cached treemap layout, computed after scan completes. Recomputed
    /// when the central panel resizes. (ref: DL-005)
    treemap_layout: Option<treemap::TreemapLayout>,
    /// Extension filter: when set, treemap dims files not matching this extension.
    /// Set by clicking in ext stats panel, independent of `selected_node`. (ref: DL-001)
    selected_extension: Option<String>,
    /// Root node index for treemap drill-down. Defaults to `tree.root()` (0).
    /// Changed by double-click in treemap, navigated via breadcrumb. (ref: DL-006)
    treemap_root: usize,
}

impl Default for RustDirStatApp {
    /// Delegates to `new(None)` for backward compatibility with
    /// callers that construct via Default. (ref: DL-007)
    fn default() -> Self {
        Self::new(None)
    }
}

impl RustDirStatApp {
    /// Creates a new app. If `initial_path` is `Some`, scanning starts
    /// automatically on the first frame.
    pub fn new(initial_path: Option<PathBuf>) -> Self {
        Self {
            phase: ScanPhase::Idle,
            tree: None,
            receiver: None,
            cancel: None,
            scan_handle: None,
            files_scanned: 0,
            bytes_scanned: 0,
            scan_path: None,
            initial_path,
            path_input: String::new(),
            path_error: None,
            scan_errors: 0,
            scan_start: None,
            last_live_recompute: None,
            live_node_count: 0,
            extension_stats: None,
            tree_view_state: tree_view::TreeViewState::new(),
            selected_node: None,
            subtree_stats: None,
            treemap_layout: None,
            selected_extension: None,
            treemap_root: 0,
        }
    }

    /// Cancels any running scan, resets state, and spawns a new Scanner
    /// thread for `path`. Creates a bounded(4096) crossbeam channel and
    /// a fresh cancel flag. Old scanner cleanup happens on a detached
    /// thread to avoid freezing the GUI. (ref: DL-002, DL-009)
    fn start_scan(&mut self, path: PathBuf) {
        // Cancel existing scan if running.
        if let Some(ref cancel) = self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        // Move old channel and handle to a detached cleanup thread so the
        // GUI thread does not block on join. The cleanup thread drains
        // remaining events (unblocking the scanner if the channel was full)
        // and then joins the scanner thread. (ref: DL-009)
        let old_rx = self.receiver.take();
        let old_handle = self.scan_handle.take();
        if old_rx.is_some() || old_handle.is_some() {
            std::thread::spawn(move || {
                if let Some(rx) = old_rx {
                    for _ in rx {}
                }
                if let Some(handle) = old_handle {
                    let _ = handle.join();
                }
            });
        }

        // Reset scan state.
        self.tree = None;
        self.files_scanned = 0;
        self.bytes_scanned = 0;
        self.scan_errors = 0;
        self.scan_start = Some(Instant::now());
        self.last_live_recompute = None;
        self.live_node_count = 0;
        self.extension_stats = None;
        self.tree_view_state.reset();
        self.selected_node = None;
        self.subtree_stats = None;
        self.treemap_layout = None;
        self.selected_extension = None;
        self.treemap_root = 0;
        self.path_error = None;
        self.phase = ScanPhase::Scanning;
        self.scan_path = Some(path.clone());

        // Launch scanner.
        let (tx, rx) = crossbeam_channel::bounded(4096);
        let cancel = Arc::new(AtomicBool::new(false));
        let config = ScanConfig {
            root: path,
            ..ScanConfig::default()
        };

        let handle = rds_scanner::Scanner::scan(config, tx, cancel.clone());

        self.receiver = Some(rx);
        self.cancel = Some(cancel);
        self.scan_handle = Some(handle);
    }

    /// Transitions to Complete, drops the channel, and joins the scanner
    /// thread on a detached thread. Although the scanner has already sent
    /// `ScanComplete`, it still needs to drop locals (notably `path_to_index`
    /// which can hold millions of PathBuf entries on large scans). Joining
    /// on a detached thread avoids GUI stutter from those deallocations.
    fn finish_scan(&mut self, stats: ScanStats) {
        self.phase = ScanPhase::Complete(stats);
        self.scan_start = None;
        self.last_live_recompute = None;
        if let Some(ref tree) = self.tree {
            self.extension_stats = Some(rds_core::stats::compute_extension_stats(tree));
        }
        self.receiver = None;
        self.cancel = None;
        if let Some(handle) = self.scan_handle.take() {
            std::thread::spawn(move || {
                let _ = handle.join();
            });
        }
        if let Some(ref tree) = self.tree {
            self.subtree_stats = Some(tree_view::SubtreeStats::compute(tree));
            self.tree_view_state.expand(tree.root());
        }
        self.treemap_layout = None;
    }

    /// Drains up to 100 ScanEvent values from the channel per frame.
    /// Inserts nodes into the DirTree arena, updates progress counters,
    /// and transitions to Complete on ScanComplete or channel disconnect.
    ///
    /// Clones the Receiver (reference-counted) to avoid borrowing
    /// `self.receiver` for the duration of the loop, which would prevent
    /// `self.receiver = None` on scan completion. (ref: DL-003, DL-005, DL-008)
    fn drain_events(&mut self) {
        let rx = match self.receiver.clone() {
            Some(rx) => rx,
            None => return,
        };

        for _ in 0..100 {
            match rx.try_recv() {
                Ok(ScanEvent::NodeDiscovered { node, parent_index }) => {
                    match parent_index {
                        None => {
                            // First event: create the tree with root node.
                            self.tree = Some(DirTree::from_root(node));
                        }
                        Some(pidx) => {
                            if let Some(ref mut t) = self.tree {
                                t.insert(pidx, node);
                            }
                        }
                    }
                }
                Ok(ScanEvent::Progress {
                    files_scanned,
                    bytes_scanned,
                }) => {
                    self.files_scanned = files_scanned;
                    self.bytes_scanned = bytes_scanned;
                }
                Ok(ScanEvent::ScanComplete { stats }) => {
                    self.finish_scan(stats);
                    return;
                }
                Ok(ScanEvent::ScanError { .. }) => {
                    self.scan_errors += 1;
                }
                Ok(ScanEvent::DuplicateFound { .. }) => {}
                Err(crossbeam_channel::TryRecvError::Empty) => return,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    // Scanner thread exited without ScanComplete (shouldn't
                    // happen, but handle gracefully).
                    self.finish_scan(ScanStats {
                        total_files: 0,
                        total_dirs: 0,
                        total_bytes: 0,
                        duration_ms: 0,
                        errors: self.scan_errors,
                    });
                    return;
                }
            }
        }
    }

    /// Periodically recomputes SubtreeStats and ExtensionStats from the
    /// partially-built tree so panels can render live data during scan.
    /// Throttled to at most once per 500ms, skipped when no new nodes
    /// have arrived since the last recompute. (ref: DL-001, DL-002, DL-009)
    fn maybe_live_recompute(&mut self) {
        if !matches!(self.phase, ScanPhase::Scanning) {
            return;
        }

        let tree = match self.tree.as_ref() {
            Some(t) => t,
            None => return,
        };

        let current_count = tree.len();
        if current_count == self.live_node_count {
            return;
        }

        let now = Instant::now();
        let enough_elapsed = match self.last_live_recompute {
            None => {
                // First recompute: wait at least 500ms from scan start. (ref: DL-007)
                match self.scan_start {
                    Some(start) => now.duration_since(start) >= LIVE_RECOMPUTE_INTERVAL,
                    None => false,
                }
            }
            Some(last) => now.duration_since(last) >= LIVE_RECOMPUTE_INTERVAL,
        };
        if !enough_elapsed {
            return;
        }

        self.subtree_stats = Some(tree_view::SubtreeStats::compute(tree));
        self.extension_stats = Some(rds_core::stats::compute_extension_stats(tree));
        self.treemap_layout = None;
        self.tree_view_state.expand(tree.root());
        self.last_live_recompute = Some(now);
        self.live_node_count = current_count;
    }
}

const LIVE_RECOMPUTE_INTERVAL: Duration = Duration::from_millis(500);

/// Formats a byte count as a human-readable string (B/KB/MB/GB/TB).
pub(crate) fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

impl eframe::App for RustDirStatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Auto-start scan from CLI path on first frame.
        if let Some(path) = self.initial_path.take() {
            self.start_scan(path);
        }

        // Drain scanner events and keep repainting while scanning.
        if matches!(self.phase, ScanPhase::Scanning) {
            self.drain_events();
            self.maybe_live_recompute();
            ctx.request_repaint();
        }

        // --- Toolbar ---
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Native dialog button — may crash on WSL2/Wayland.
                if ui.button("Browse...").clicked()
                    && let Some(path) = rfd::FileDialog::new().pick_folder()
                {
                    self.path_input = path.display().to_string();
                    self.start_scan(path);
                }

                ui.separator();

                // Text input fallback — always works, including WSL2.
                // Reserve ~60px for the Scan button; fill remaining width.
                let text_width = (ui.available_width() - 60.0).max(100.0);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.path_input)
                        .hint_text("/path/to/scan")
                        .desired_width(text_width),
                );
                if response.changed() {
                    self.path_error = None;
                }
                let scan_clicked = ui.button("Scan").clicked();
                let enter_pressed = response.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter));

                if (scan_clicked || enter_pressed) && !self.path_input.is_empty() {
                    let path = PathBuf::from(&self.path_input);
                    if path.is_dir() {
                        self.path_error = None;
                        self.start_scan(path);
                    } else {
                        self.path_error = Some("Not a valid directory.".to_string());
                    }
                }

                if let Some(ref err) = self.path_error {
                    ui.colored_label(egui::Color32::from_rgb(255, 100, 100), err);
                }
            });
        });

        // --- Status bar / progress ---
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            match &self.phase {
                ScanPhase::Idle => {
                    ui.label("Ready \u{2014} open a folder to begin scanning.");
                }
                ScanPhase::Scanning => {
                    let elapsed_secs = self
                        .scan_start
                        .map(|t| t.elapsed().as_secs_f64())
                        .unwrap_or(0.0);

                    let mut text = if elapsed_secs < 0.01 {
                        format!(
                            "Scanning\u{2026} {} files \u{00B7} {} \u{00B7} {:.1}s \u{00B7} \u{2014} files/s \u{00B7} \u{2014}/s",
                            self.files_scanned,
                            format_bytes(self.bytes_scanned),
                            elapsed_secs,
                        )
                    } else {
                        let files_per_sec = self.files_scanned as f64 / elapsed_secs;
                        let bytes_per_sec = self.bytes_scanned as f64 / elapsed_secs;
                        format!(
                            "Scanning\u{2026} {} files \u{00B7} {} \u{00B7} {:.1}s \u{00B7} {:.0} files/s \u{00B7} {}/s",
                            self.files_scanned,
                            format_bytes(self.bytes_scanned),
                            elapsed_secs,
                            files_per_sec,
                            format_bytes(bytes_per_sec as u64),
                        )
                    };
                    if self.scan_errors > 0 {
                        text.push_str(&format!(" ({} errors)", self.scan_errors));
                    }

                    let fraction = (elapsed_secs * 0.3 % 1.0) as f32;
                    let bar = egui::ProgressBar::new(fraction)
                        .animate(true)
                        .text(text);
                    ui.add(bar);
                }
                ScanPhase::Complete(stats) => {
                    let mut text = format!(
                        "Done \u{2014} {} files, {} dirs, {} in {:.1}s",
                        stats.total_files,
                        stats.total_dirs,
                        format_bytes(stats.total_bytes),
                        stats.duration_ms as f64 / 1000.0,
                    );
                    if self.scan_errors > 0 {
                        text.push_str(&format!(" ({} errors)", self.scan_errors));
                    }
                    ui.label(text);
                }
            }
        });

        // --- Left panel: directory tree (MS6) ---
        egui::SidePanel::left("tree_panel")
            .default_width(250.0)
            .show(ctx, |ui| {
                ui.heading("Directory Tree");
                ui.separator();
                match (&self.tree, &self.subtree_stats) {
                    (Some(tree), Some(stats)) => {
                        tree_view::show(
                            tree,
                            stats,
                            &mut self.tree_view_state,
                            &mut self.selected_node,
                            ui,
                        );
                    }
                    (Some(_), None) => {
                        ui.colored_label(
                            egui::Color32::GRAY,
                            "Scan in progress\u{2026}",
                        );
                    }
                    _ => {
                        ui.colored_label(egui::Color32::GRAY, "No scan data.");
                    }
                }
            });

        // --- Right panel: extension statistics (MS7) ---
        egui::SidePanel::right("ext_stats_panel")
            .default_width(220.0)
            .show(ctx, |ui| {
                ui.heading("Extensions");
                ui.separator();
                match &self.extension_stats {
                    Some(stats) => {
                        ext_stats::show(stats, &mut self.selected_extension, ui);
                    }
                    None => {
                        if self.tree.is_some() {
                            ui.label(format!("{} files scanned", self.files_scanned));
                        } else {
                            ui.colored_label(egui::Color32::GRAY, "No scan data.");
                        }
                    }
                }
            });

        // --- Central panel: treemap (MS8) + breadcrumb (MS10) ---
        egui::CentralPanel::default().show(ctx, |ui| {
            if let (Some(tree), Some(stats)) =
                (self.tree.as_ref(), self.subtree_stats.as_ref())
            {
                // Breadcrumb navigation — only visible when drilled in. (ref: DL-007)
                if self.treemap_root != tree.root() {
                    ui.horizontal(|ui| {
                        let chain = treemap::breadcrumb_chain(tree, self.treemap_root);
                        for (i, (idx, name)) in chain.iter().enumerate() {
                            if i > 0 {
                                ui.label("\u{203A}"); // › separator
                            }
                            if *idx == self.treemap_root {
                                // Current directory: non-clickable label.
                                ui.strong(name);
                            } else if ui.link(name).clicked() {
                                self.treemap_root = *idx;
                                self.treemap_layout = None;
                            }
                        }
                    });
                    ui.separator();
                }

                // Recompute layout if panel size or root changed. (ref: MS8 DL-005, MS10 DL-009)
                let available_size = ui.available_size();
                let needs_recompute = self.treemap_layout.as_ref().is_none_or(|l| {
                    l.last_root != self.treemap_root
                        || (l.last_size.x - available_size.x).abs() > 1.0
                        || (l.last_size.y - available_size.y).abs() > 1.0
                });

                if needs_recompute {
                    self.treemap_layout = Some(treemap::TreemapLayout::compute(
                        tree,
                        stats,
                        available_size,
                        self.treemap_root,
                    ));
                }

                if let Some(layout) = self.treemap_layout.as_ref() {
                    let prev_root = self.treemap_root;
                    treemap::show(
                        layout,
                        tree,
                        &mut self.selected_node,
                        &self.selected_extension,
                        &mut self.treemap_root,
                        ui,
                    );
                    // Invalidate layout if drill-down changed the root.
                    if self.treemap_root != prev_root {
                        self.treemap_layout = None;
                    }
                }
            } else {
                ui.heading("Treemap");
                ui.separator();
                if matches!(self.phase, ScanPhase::Scanning) {
                    ui.colored_label(
                        egui::Color32::GRAY,
                        "Scan in progress\u{2026}",
                    );
                } else {
                    ui.colored_label(egui::Color32::GRAY, "No scan data.");
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_idle() {
        let app = RustDirStatApp::default();
        assert!(matches!(app.phase, ScanPhase::Idle));
        assert!(app.tree.is_none());
        assert!(app.receiver.is_none());
        assert!(app.cancel.is_none());
        assert!(app.scan_handle.is_none());
        assert_eq!(app.files_scanned, 0);
        assert_eq!(app.bytes_scanned, 0);
        assert_eq!(app.scan_errors, 0);
        assert!(app.extension_stats.is_none());
        assert!(app.selected_node.is_none());
        assert!(app.subtree_stats.is_none());
        assert!(app.treemap_layout.is_none());
        assert!(app.selected_extension.is_none());
        assert_eq!(app.treemap_root, 0);
        assert!(app.path_error.is_none());
        assert!(app.scan_path.is_none());
        assert!(app.path_input.is_empty());
        assert!(app.initial_path.is_none());
        assert!(app.scan_start.is_none());
        assert!(app.last_live_recompute.is_none());
        assert_eq!(app.live_node_count, 0);
    }

    #[test]
    fn format_bytes_zero() {
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn format_bytes_under_kb() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn format_bytes_kb() {
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
    }

    #[test]
    fn format_bytes_mb() {
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
    }

    #[test]
    fn format_bytes_gb() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn format_bytes_tb() {
        assert_eq!(format_bytes(1024u64 * 1024 * 1024 * 1024), "1.0 TB");
    }
}
