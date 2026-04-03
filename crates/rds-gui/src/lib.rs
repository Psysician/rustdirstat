//! egui application shell with directory picker, scan progress, tree view,
//! treemap, and extension statistics.
//!
//! `RustDirStatApp` owns scan state and renders a 4-panel layout: directory
//! tree (MS6), treemap (MS8), extension statistics (MS7), and duplicate
//! file detection (MS12).
//! The scanner runs on a background thread; events are drained via
//! `try_recv()` each frame (bounded to `DRAIN_BATCH_SIZE` events to avoid
//! blocking rendering). (ref: DL-003, DL-006)

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use rds_core::CustomCommand;
use rds_core::scan::{ScanConfig, ScanEvent, ScanStats};
use rds_core::stats::ExtensionStats;
use rds_core::tree::DirTree;

mod actions;
mod command_editor;
mod duplicates;
mod error_log;
mod export;
mod ext_stats;
mod notifications;
mod settings;
mod tree_view;
mod treemap;

#[cfg(feature = "bench-internals")]
pub use tree_view::SubtreeStats;
#[cfg(feature = "bench-internals")]
pub use treemap::{MAX_DISPLAY_RECTS, TreemapLayout, TreemapRect};

/// Scan lifecycle phases. (ref: DL-004)
enum ScanPhase {
    /// No scan running; waiting for user to pick a directory.
    Idle,
    /// Scanner thread is active; draining events each frame.
    Scanning,
    /// Scanner finished; summary stats available.
    Complete(ScanStats),
}

/// A group of files with identical content, detected by SHA-256 hashing.
pub(crate) struct DuplicateGroup {
    pub(crate) node_indices: Vec<usize>,
    pub(crate) wasted_bytes: u64,
}

/// Transient UI state for the custom command editor window.
#[derive(Default)]
pub(crate) struct CommandEditorState {
    pub(crate) show: bool,
    pub(crate) new_name: String,
    pub(crate) new_template: String,
}

/// Pending delete confirmation state. Populated when the user initiates a
/// delete action; consumed by `confirm_delete` when the user confirms.
pub(crate) struct PendingDelete {
    pub(crate) node_index: usize,
    pub(crate) path_display: String,
    pub(crate) size_bytes: u64,
    pub(crate) is_dir: bool,
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
    /// Pre-allocation capacity hint for DirTree arena, derived from ScanConfig.
    tree_capacity_hint: usize,
    /// Accumulated scan errors with path and message detail, capped at 1000.
    scan_error_log: error_log::ScanErrorLog,
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
    /// Cached treemap GPU mesh. Rebuilt when layout or extension filter changes.
    treemap_mesh_cache: Option<treemap::TreemapMeshCache>,
    /// Root node index for treemap drill-down. Defaults to `tree.root()` (0).
    /// Changed by double-click in treemap, navigated via breadcrumb. (ref: DL-006)
    treemap_root: usize,
    /// Accumulated duplicate groups from DuplicateFound events, sorted by
    /// wasted_bytes descending after scan completes.
    duplicate_groups: Vec<DuplicateGroup>,
    /// Whether to run duplicate detection on next scan.
    hash_duplicates_enabled: bool,
    /// True while the scanner is running the duplicate detection pipeline.
    detecting_duplicates: bool,
    /// Pending delete confirmation. Set when user initiates delete, consumed
    /// by `confirm_delete` when user confirms in the dialog.
    pending_delete: Option<PendingDelete>,
    /// Cumulative bytes freed by trash-delete actions in the current scan session.
    /// Reset to 0 when a new scan starts.
    freed_bytes: u64,
    /// Error message from the most recent failed delete attempt.
    delete_error: Option<String>,
    /// User-defined custom commands available in context menus.
    custom_commands: Vec<CustomCommand>,
    /// Transient UI state for the custom command editor window.
    command_editor: CommandEditorState,
    /// Export dialog state (format, scope, visibility, last result).
    export_dialog: export::ExportDialogState,
    /// Settings dialog state (working copies of config fields).
    settings_dialog: settings::SettingsDialogState,
    /// Glob patterns to exclude from scans.
    exclude_patterns: Vec<String>,
    /// Recently scanned paths for quick re-access.
    recent_paths: Vec<PathBuf>,
    /// Default sort order for tree/stats panels.
    default_sort: rds_core::SortOrder,
    /// Active color scheme name.
    color_scheme: rds_core::ColorScheme,
    /// Maximum number of recent paths to retain.
    max_recent_paths: usize,
    /// Whether the scanner should follow symbolic links.
    follow_symlinks: bool,
    /// Whether to show the max-nodes limit dialog.
    max_nodes_dialog: bool,
    /// Cached message for the max-nodes dialog, extracted once in drain_events.
    max_nodes_message: Option<String>,
    /// Toast notification overlay.
    notifications: notifications::Notifications,
    /// Optional callback invoked to persist config changes to disk.
    #[allow(clippy::type_complexity)]
    config_save_fn: Option<Box<dyn Fn(&rds_core::AppConfig) + Send>>,
}

impl Default for RustDirStatApp {
    /// Delegates to `new(None, AppConfig::default())` for backward compatibility
    /// with callers that construct via Default. (ref: DL-007)
    ///
    /// Note: `config_save_fn` is `None`, so config changes are not persisted.
    /// Call [`set_config_save_fn`](Self::set_config_save_fn) after construction
    /// if persistence is required.
    fn default() -> Self {
        Self::new(None, rds_core::AppConfig::default())
    }
}

impl RustDirStatApp {
    /// Minimum interval between live recomputes of SubtreeStats/ExtensionStats
    /// during scan. Caps treemap re-layout at 2x/second. (ref: DL-002)
    const LIVE_RECOMPUTE_INTERVAL: Duration = Duration::from_millis(500);

    /// Maximum number of ScanEvent values drained from the channel per frame.
    const DRAIN_BATCH_SIZE: usize = 5000;

    /// Creates a new app. If `initial_path` is `Some`, scanning starts
    /// automatically on the first frame. Config fields are populated from
    /// the provided `AppConfig`.
    pub fn new(initial_path: Option<PathBuf>, config: rds_core::AppConfig) -> Self {
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
            tree_capacity_hint: 100_000,
            scan_error_log: error_log::ScanErrorLog::default(),
            scan_start: None,
            last_live_recompute: None,
            live_node_count: 0,
            extension_stats: None,
            tree_view_state: tree_view::TreeViewState::new(),
            selected_node: None,
            subtree_stats: None,
            treemap_layout: None,
            selected_extension: None,
            treemap_mesh_cache: None,
            treemap_root: 0,
            duplicate_groups: Vec::new(),
            hash_duplicates_enabled: false,
            detecting_duplicates: false,
            pending_delete: None,
            freed_bytes: 0,
            delete_error: None,
            custom_commands: config.custom_commands,
            command_editor: CommandEditorState::default(),
            export_dialog: export::ExportDialogState::default(),
            settings_dialog: settings::SettingsDialogState::default(),
            exclude_patterns: config.exclude_patterns,
            recent_paths: config.recent_paths,
            default_sort: config.default_sort,
            color_scheme: config.color_scheme,
            max_recent_paths: config.max_recent_paths,
            follow_symlinks: config.follow_symlinks,
            max_nodes_dialog: false,
            max_nodes_message: None,
            notifications: notifications::Notifications::default(),
            config_save_fn: None,
        }
    }

    /// Sets the callback used to persist config changes to disk.
    pub fn set_config_save_fn(&mut self, f: impl Fn(&rds_core::AppConfig) + Send + 'static) {
        self.config_save_fn = Some(Box::new(f));
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
        self.scan_error_log.clear();
        self.scan_start = Some(Instant::now());
        self.last_live_recompute = None;
        self.live_node_count = 0;
        self.extension_stats = None;
        self.tree_view_state.reset();
        self.selected_node = None;
        self.subtree_stats = None;
        self.treemap_layout = None;
        self.treemap_mesh_cache = None;
        self.selected_extension = None;
        self.treemap_root = 0;
        self.duplicate_groups = Vec::new();
        self.detecting_duplicates = false;
        self.pending_delete = None;
        self.freed_bytes = 0;
        self.delete_error = None;
        self.path_error = None;
        self.export_dialog.last_result = None;
        self.max_nodes_dialog = false;
        self.max_nodes_message = None;
        self.phase = ScanPhase::Scanning;
        self.scan_path = Some(path.clone());
        self.track_recent_path(path.clone());

        // Launch scanner.
        let (tx, rx) = crossbeam_channel::bounded(4096);
        let cancel = Arc::new(AtomicBool::new(false));
        let config = ScanConfig {
            root: path,
            hash_duplicates: self.hash_duplicates_enabled,
            exclude_patterns: self.exclude_patterns.clone(),
            follow_symlinks: self.follow_symlinks,
            ..ScanConfig::default()
        };
        self.tree_capacity_hint = config.max_nodes.unwrap_or(100_000).min(100_000);

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
        self.live_node_count = 0;
        self.detecting_duplicates = false;
        self.duplicate_groups
            .sort_by(|a, b| b.wasted_bytes.cmp(&a.wasted_bytes));
        if let Some(ref mut tree) = self.tree {
            tree.shrink_to_fit();
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
            self.tree_view_state.invalidate_sorted_cache();
            self.tree_view_state.expand(tree.root());
        }
        self.treemap_layout = None;
        self.treemap_mesh_cache = None;
    }

    /// Drains up to `DRAIN_BATCH_SIZE` ScanEvent values from the channel per frame.
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

        for _ in 0..Self::DRAIN_BATCH_SIZE {
            match rx.try_recv() {
                Ok(ScanEvent::NodeDiscovered {
                    node,
                    parent_index,
                    extension_name,
                    node_name,
                }) => {
                    match parent_index {
                        None => {
                            // First event: create the tree with root node.
                            self.tree = Some(DirTree::from_root_with_capacity(
                                node,
                                &node_name,
                                self.tree_capacity_hint,
                            ));
                        }
                        Some(pidx) => {
                            if let Some(ref mut t) = self.tree {
                                let mut node = node;
                                node.extension = t.intern_extension(extension_name.as_deref());
                                t.insert(pidx, node, &node_name);
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
                Ok(ScanEvent::ScanError { path, error }) => {
                    let is_max_nodes = error.contains("max_nodes limit");
                    if is_max_nodes {
                        self.max_nodes_message = Some(error.clone());
                        self.max_nodes_dialog = true;
                    }
                    self.scan_error_log.push(path, error);
                }
                Ok(ScanEvent::DuplicateFound { node_indices, .. }) => {
                    let size = node_indices
                        .first()
                        .and_then(|&idx| self.tree.as_ref()?.get(idx))
                        .map(|node| node.size)
                        .unwrap_or(0);
                    let wasted_bytes =
                        size.saturating_mul(node_indices.len().saturating_sub(1) as u64);
                    self.duplicate_groups.push(DuplicateGroup {
                        node_indices,
                        wasted_bytes,
                    });
                }
                Ok(ScanEvent::DuplicateDetectionStarted { .. }) => {
                    self.detecting_duplicates = true;
                }
                Err(crossbeam_channel::TryRecvError::Empty) => return,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    // Scanner thread exited without ScanComplete (shouldn't
                    // happen, but handle gracefully).
                    self.finish_scan(ScanStats {
                        total_files: 0,
                        total_dirs: 0,
                        total_bytes: 0,
                        duration_ms: 0,
                        errors: self.scan_error_log.total_count(),
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
                    Some(start) => now.duration_since(start) >= Self::LIVE_RECOMPUTE_INTERVAL,
                    None => false,
                }
            }
            Some(last) => now.duration_since(last) >= Self::LIVE_RECOMPUTE_INTERVAL,
        };
        if !enough_elapsed {
            return;
        }

        self.subtree_stats = Some(tree_view::SubtreeStats::compute(tree));
        self.extension_stats = Some(rds_core::stats::compute_extension_stats(tree));
        self.treemap_layout = None;
        self.treemap_mesh_cache = None;
        self.tree_view_state.invalidate_sorted_cache();
        self.tree_view_state.expand(tree.root());
        self.last_live_recompute = Some(now);
        self.live_node_count = current_count;
    }

    /// Executes a pending delete: sends the entry to the OS trash, tombstones
    /// the arena node, invalidates cached stats and layout, and cleans up
    /// duplicate groups. Called when the user confirms in the delete dialog.
    fn confirm_delete(&mut self) {
        let pending = match self.pending_delete.take() {
            Some(p) => p,
            None => return,
        };

        let tree = match self.tree.as_mut() {
            Some(t) => t,
            None => return,
        };

        match actions::execute_delete(tree, pending.node_index) {
            Ok(freed) => {
                self.freed_bytes += freed;

                // Recompute cached stats immediately. There is no lazy-recompute
                // path in ScanPhase::Complete — panels check for Some and show
                // fallback text when None.
                let tree_ref = self.tree.as_ref().unwrap();
                self.subtree_stats = Some(tree_view::SubtreeStats::compute(tree_ref));
                self.extension_stats = Some(rds_core::stats::compute_extension_stats(tree_ref));
                self.treemap_layout = None;
                self.treemap_mesh_cache = None;
                self.tree_view_state.invalidate_sorted_cache();

                // Clear selection if it pointed at a now-deleted node (the target
                // or any of its descendants).
                if let Some(sel) = self.selected_node
                    && self
                        .tree
                        .as_ref()
                        .unwrap()
                        .get(sel)
                        .is_some_and(|n| n.deleted())
                {
                    self.selected_node = None;
                }

                // Reset treemap root if it pointed at a now-deleted node (e.g.,
                // user drilled into a subdirectory that was then deleted).
                if self
                    .tree
                    .as_ref()
                    .unwrap()
                    .get(self.treemap_root)
                    .is_some_and(|n| n.deleted())
                {
                    self.treemap_root = self.tree.as_ref().unwrap().root();
                }

                actions::cleanup_duplicate_groups(
                    &mut self.duplicate_groups,
                    self.tree.as_ref().unwrap(),
                );

                self.delete_error = None;
            }
            Err(msg) => {
                self.delete_error = Some(msg);
                // Restore pending_delete so the dialog stays open and shows
                // the error message instead of silently disappearing.
                self.pending_delete = Some(pending);
            }
        }
    }

    /// Persists the current config to disk via the save callback.
    fn save_config(&self) {
        let config = self.collect_config();
        if let Some(ref save_fn) = self.config_save_fn {
            save_fn(&config);
        }
    }

    /// Tracks a path in the recent paths list. Canonicalizes the path (falling
    /// back to the original if canonicalization fails), removes any existing
    /// occurrence, inserts at position 0, and truncates to `max_recent_paths`.
    /// Persists the updated config via the save callback.
    fn track_recent_path(&mut self, path: PathBuf) {
        let canonical = std::fs::canonicalize(&path).unwrap_or(path);
        self.recent_paths.retain(|p| p != &canonical);
        self.recent_paths.insert(0, canonical);
        self.recent_paths.truncate(self.max_recent_paths);
        self.save_config();
    }

    fn collect_config(&self) -> rds_core::AppConfig {
        rds_core::AppConfig {
            custom_commands: self.custom_commands.clone(),
            exclude_patterns: self.exclude_patterns.clone(),
            color_scheme: self.color_scheme,
            default_sort: self.default_sort,
            recent_paths: self.recent_paths.clone(),
            max_recent_paths: self.max_recent_paths,
            follow_symlinks: self.follow_symlinks,
        }
    }
}

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

        // --- Apply theme based on color scheme setting ---
        let theme_pref = match self.color_scheme {
            rds_core::ColorScheme::Default => egui::ThemePreference::System,
            rds_core::ColorScheme::Dark => egui::ThemePreference::Dark,
            rds_core::ColorScheme::Light => egui::ThemePreference::Light,
        };
        ctx.set_theme(theme_pref);

        // --- Keyboard shortcuts ---
        // Process shortcuts before UI rendering so key events are consumed
        // before text fields or buttons process them.

        // Ctrl/Cmd+O: Open browse dialog (same as Browse button).
        if ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::O,
            ))
        }) && let Some(path) = rfd::FileDialog::new().pick_folder()
        {
            self.path_input = path.display().to_string();
            self.start_scan(path);
        }

        // F5: Rescan current directory.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F5))
            && let Some(ref path) = self.scan_path
        {
            let path = path.clone();
            self.start_scan(path);
        }

        // Escape: Close topmost dialog / cancel scan / deselect.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            if self.max_nodes_dialog {
                self.max_nodes_dialog = false;
            } else if self.pending_delete.is_some() {
                self.pending_delete = None;
                self.delete_error = None;
            } else if self.settings_dialog.show {
                self.settings_dialog.show = false;
            } else if self.command_editor.show {
                self.command_editor.show = false;
            } else if self.export_dialog.show {
                self.export_dialog.show = false;
            } else if matches!(self.phase, ScanPhase::Scanning)
                && let Some(ref cancel) = self.cancel
            {
                cancel.store(true, Ordering::Relaxed);
            } else {
                self.selected_node = None;
            }
        }

        // Backspace: Navigate up from treemap drill-down (only when no text
        // widget has keyboard focus, to avoid conflicting with text editing).
        if !ctx.wants_keyboard_input()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace))
            && let Some(ref tree) = self.tree
            && self.treemap_root != tree.root()
            && let Some(node) = tree.get(self.treemap_root)
            && node.parent != rds_core::tree::NO_PARENT
        {
            self.treemap_root = node.parent as usize;
            self.treemap_layout = None;
            self.treemap_mesh_cache = None;
        }

        // --- Max-nodes limit dialog ---
        if self.max_nodes_dialog {
            let limit_msg = self
                .max_nodes_message
                .as_deref()
                .unwrap_or("The scan was stopped because the node limit was reached.");
            egui::Window::new("Scan Limit Reached")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(limit_msg);
                    ui.label("Partial results are still available below.");
                    ui.label("For more detailed analysis, try scanning a subdirectory instead.");
                    ui.separator();
                    if ui.button("OK").clicked() {
                        self.max_nodes_dialog = false;
                    }
                });
        }

        // --- Confirmation dialog ---
        // Rendered before panels so it draws on top. Button clicks set flags
        // that are acted on after the Window block to avoid borrow conflicts.
        let mut do_confirm = false;
        let mut do_cancel = false;
        if let Some(ref pending) = self.pending_delete {
            let item_type = if pending.is_dir { "directory" } else { "file" };
            egui::Window::new("Confirm Delete")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "Delete {} \"{}\"?",
                        item_type, pending.path_display,
                    ));
                    ui.label(format!("Size: {}", format_bytes(pending.size_bytes)));
                    ui.label("The item will be moved to the recycle bin.");
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() {
                            do_confirm = true;
                        }
                        if ui.button("Cancel").clicked() {
                            do_cancel = true;
                        }
                    });
                    if let Some(ref err) = self.delete_error {
                        ui.colored_label(egui::Color32::from_rgb(255, 80, 80), err);
                    }
                });
        }
        if do_confirm {
            self.confirm_delete();
        }
        if do_cancel {
            self.pending_delete = None;
            self.delete_error = None;
        }

        // --- Command editor window ---
        let commands_changed =
            command_editor::show(&mut self.custom_commands, &mut self.command_editor, ctx);
        if commands_changed {
            self.save_config();
        }

        // --- Export dialog ---
        export::show_dialog(
            &mut self.export_dialog,
            self.tree.as_ref(),
            self.treemap_root,
            &self.duplicate_groups,
            &mut self.notifications,
            ctx,
        );

        // --- Settings dialog ---
        let settings_applied = settings::show(
            &mut self.settings_dialog,
            &mut self.exclude_patterns,
            &mut self.default_sort,
            &mut self.color_scheme,
            &mut self.follow_symlinks,
            &mut self.max_recent_paths,
            ctx,
        );
        if settings_applied {
            self.recent_paths.truncate(self.max_recent_paths);
            self.save_config();
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

                if !self.recent_paths.is_empty() {
                    let mut selected_recent: Option<PathBuf> = None;
                    ui.menu_button("Recent", |ui| {
                        for recent in &self.recent_paths {
                            let label = recent.display().to_string();
                            if ui.selectable_label(false, &label).clicked() {
                                selected_recent = Some(recent.clone());
                                ui.close();
                            }
                        }
                    });
                    if let Some(path) = selected_recent {
                        self.path_input = path.display().to_string();
                        self.start_scan(path);
                    }
                }

                ui.separator();

                // Text input fallback — always works, including WSL2.
                // Reserve space for right-side controls. Per-button estimates:
                const BTN_SCAN: f32 = 50.0;
                const BTN_DETECT_DUPES: f32 = 150.0;
                const BTN_COMMANDS: f32 = 90.0;
                const BTN_SETTINGS: f32 = 80.0;
                const BTN_EXPORT: f32 = 70.0;
                const SEPARATORS: f32 = 40.0; // ~10px each x4
                // In release builds the "Detect Duplicates" checkbox and its separator are hidden.
                #[cfg(debug_assertions)]
                const TOOLBAR_FIXED_CONTROLS_WIDTH: f32 = BTN_SCAN
                    + BTN_DETECT_DUPES
                    + BTN_COMMANDS
                    + BTN_SETTINGS
                    + BTN_EXPORT
                    + SEPARATORS;
                #[cfg(not(debug_assertions))]
                const TOOLBAR_FIXED_CONTROLS_WIDTH: f32 = BTN_SCAN
                    + BTN_COMMANDS
                    + BTN_SETTINGS
                    + BTN_EXPORT
                    + SEPARATORS
                    - 10.0; // subtract one hidden separator
                let text_width = (ui.available_width() - TOOLBAR_FIXED_CONTROLS_WIDTH).max(100.0);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.path_input)
                        .hint_text("/path/to/scan")
                        .desired_width(text_width),
                );
                if response.changed() {
                    self.path_error = None;
                }
                let scan_clicked = ui.button("Scan").clicked();
                let enter_pressed =
                    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

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

                ui.separator();
                #[cfg(debug_assertions)]
                ui.checkbox(&mut self.hash_duplicates_enabled, "Detect Duplicates");
                #[cfg(debug_assertions)]
                ui.separator();
                if ui.button("Commands...").clicked() {
                    self.command_editor.show = !self.command_editor.show;
                }

                if ui.button("Settings...").clicked() {
                    if !self.settings_dialog.show {
                        self.settings_dialog.exclude_patterns = self.exclude_patterns.clone();
                        self.settings_dialog.default_sort = self.default_sort;
                        self.settings_dialog.color_scheme = self.color_scheme;
                        self.settings_dialog.follow_symlinks = self.follow_symlinks;
                        self.settings_dialog.max_recent_paths = self.max_recent_paths;
                        self.settings_dialog.new_pattern = String::new();
                    }
                    self.settings_dialog.show = !self.settings_dialog.show;
                }

                ui.separator();
                let scan_complete = matches!(self.phase, ScanPhase::Complete(_));
                if scan_complete {
                    if ui.button("Export...").clicked() {
                        self.export_dialog.show = !self.export_dialog.show;
                    }
                } else {
                    ui.add_enabled(false, egui::Button::new("Export..."));
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

                    let mut text = if self.detecting_duplicates {
                        format!(
                            "Detecting duplicates\u{2026} {} files \u{00B7} {} \u{00B7} {:.1}s",
                            self.files_scanned,
                            format_bytes(self.bytes_scanned),
                            elapsed_secs,
                        )
                    } else if elapsed_secs < 0.01 {
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
                    if self.scan_error_log.total_count() > 0 {
                        text.push_str(&format!(" ({} errors)", self.scan_error_log.total_count()));
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
                    if self.scan_error_log.total_count() > 0 {
                        text.push_str(&format!(" ({} errors)", self.scan_error_log.total_count()));
                    }
                    if self.freed_bytes > 0 {
                        text.push_str(&format!(" | {} freed", format_bytes(self.freed_bytes)));
                    }
                    ui.label(text);
                }
            }
        });

        // --- Duplicates panel (MS12) ---
        let scan_complete = matches!(self.phase, ScanPhase::Complete(_));
        if let Some(ref tree) = self.tree
            && !self.duplicate_groups.is_empty()
        {
            egui::TopBottomPanel::bottom("duplicates_panel")
                .resizable(true)
                .show(ctx, |ui| {
                    duplicates::show(
                        &self.duplicate_groups,
                        tree,
                        &mut self.selected_node,
                        scan_complete,
                        &mut self.pending_delete,
                        &self.custom_commands,
                        &mut self.notifications,
                        ui,
                    );
                });
        }

        // --- Error log panel ---
        if !self.scan_error_log.is_empty() && scan_complete {
            egui::TopBottomPanel::bottom("error_log_panel")
                .resizable(true)
                .show(ctx, |ui| {
                    error_log::show(&self.scan_error_log, ui);
                });
        }

        // --- Bottom panel: treemap (WinDirStat layout) ---
        // Rendered before side panels so it claims the bottom half.
        // Tree and extensions share the remaining top half.
        egui::TopBottomPanel::bottom("treemap_panel")
            .resizable(true)
            .default_height(300.0)
            .show(ctx, |ui| {
                // Paint dark background over the entire panel (including frame margins)
                // so no default grey leaks through gaps in the treemap.
                ui.painter()
                    .rect_filled(ui.max_rect(), 0.0, egui::Color32::from_rgb(30, 30, 30));

                if let (Some(tree), Some(stats)) = (self.tree.as_ref(), self.subtree_stats.as_ref())
                {
                    // Breadcrumb navigation — only visible when drilled in.
                    if self.treemap_root != tree.root() {
                        ui.horizontal(|ui| {
                            let chain = treemap::breadcrumb_chain(tree, self.treemap_root);
                            for (i, (idx, name)) in chain.iter().enumerate() {
                                if i > 0 {
                                    ui.label("\u{203A}"); // › separator
                                }
                                if *idx == self.treemap_root {
                                    ui.strong(name);
                                } else if ui.link(name).clicked() {
                                    self.treemap_root = *idx;
                                    self.treemap_layout = None;
                                    self.treemap_mesh_cache = None;
                                }
                            }
                        });
                        ui.separator();
                    }

                    // Recompute layout if panel size or root changed.
                    let available_size = ui.available_size();
                    let needs_recompute = self.treemap_layout.as_ref().is_none_or(|l| {
                        l.last_root != self.treemap_root
                            || (l.last_size.x - available_size.x).abs() > 0.5
                            || (l.last_size.y - available_size.y).abs() > 0.5
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
                            stats,
                            tree,
                            &mut self.selected_node,
                            &self.selected_extension,
                            &mut self.treemap_root,
                            scan_complete,
                            &mut self.pending_delete,
                            &self.custom_commands,
                            &mut self.notifications,
                            &mut self.treemap_mesh_cache,
                            ui,
                        );
                        if self.treemap_root != prev_root {
                            self.treemap_layout = None;
                            self.treemap_mesh_cache = None;
                        }
                    }
                } else if matches!(self.phase, ScanPhase::Scanning) {
                    ui.colored_label(egui::Color32::GRAY, "Scan in progress\u{2026}");
                } else {
                    ui.colored_label(egui::Color32::GRAY, "No scan data.");
                }
            });

        // --- Left panel: directory tree (top-left) ---
        egui::SidePanel::left("tree_panel")
            .default_width(ctx.available_rect().width() * 0.5)
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
                            scan_complete,
                            &mut self.pending_delete,
                            &self.custom_commands,
                            self.default_sort,
                            &mut self.notifications,
                            ui,
                        );
                    }
                    (Some(_), None) => {
                        ui.colored_label(egui::Color32::GRAY, "Scan in progress\u{2026}");
                    }
                    _ => {
                        ui.colored_label(egui::Color32::GRAY, "No scan data.");
                    }
                }
            });

        // --- Central panel: extension statistics (top-right) ---
        egui::CentralPanel::default().show(ctx, |ui| {
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

        self.notifications.show(ctx);
    }

    fn on_exit(&mut self, _gl: std::option::Option<&eframe::glow::Context>) {
        self.save_config();
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
        assert!(app.scan_error_log.is_empty());
        assert!(app.extension_stats.is_none());
        assert!(app.selected_node.is_none());
        assert!(app.subtree_stats.is_none());
        assert!(app.treemap_layout.is_none());
        assert!(app.selected_extension.is_none());
        assert_eq!(app.treemap_root, 0);
        assert!(app.duplicate_groups.is_empty());
        assert!(!app.hash_duplicates_enabled);
        assert!(!app.detecting_duplicates);
        assert!(app.pending_delete.is_none());
        assert_eq!(app.freed_bytes, 0);
        assert!(app.delete_error.is_none());
        assert!(!app.max_nodes_dialog);
        assert!(app.path_error.is_none());
        assert!(app.scan_path.is_none());
        assert!(app.path_input.is_empty());
        assert!(app.initial_path.is_none());
        assert!(app.scan_start.is_none());
        assert!(app.last_live_recompute.is_none());
        assert_eq!(app.live_node_count, 0);
        assert!(app.custom_commands.is_empty());
        assert!(!app.command_editor.show);
        assert!(!app.export_dialog.show);
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
