use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Instant, UNIX_EPOCH};

use crossbeam_channel::Sender;
use glob::Pattern;
use jwalk::WalkDir;
use rds_core::scan::{ScanConfig, ScanEvent, ScanStats};
use rds_core::tree::FileNode;
use tracing::{debug, warn};

use crate::duplicate::DuplicateDetector;

pub struct FileEntry {
    pub path: PathBuf,
    pub arena_index: usize,
    pub size: u64,
}

/// Filesystem scanner using jwalk parallel traversal. Spawn via [`Scanner::scan`].
pub struct Scanner;

fn epoch_seconds(metadata: &std::fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

fn send_root_node(
    config: &ScanConfig,
    tx: &Sender<ScanEvent>,
    path_to_index: &mut HashMap<PathBuf, usize>,
) -> Result<(), ()> {
    let root_metadata = match std::fs::metadata(&config.root) {
        Ok(m) => m,
        Err(e) => {
            let _ = tx.send(ScanEvent::ScanError {
                path: config.root.clone(),
                error: e.to_string(),
            });
            return Err(());
        }
    };

    let root_modified = epoch_seconds(&root_metadata);
    let root_name = config.root.to_string_lossy().into_owned();

    let root_node = FileNode {
        name: root_name,
        size: 0,
        is_dir: true,
        children: Vec::new(),
        parent: None,
        extension: None,
        modified: root_modified,
        deleted: false,
    };

    if tx
        .send(ScanEvent::NodeDiscovered {
            node: root_node,
            parent_index: None,
        })
        .is_err()
    {
        return Err(());
    }

    path_to_index.insert(config.root.clone(), 0);
    Ok(())
}

fn entry_to_node(entry: &jwalk::DirEntry<((), ())>) -> Result<FileNode, String> {
    let is_dir = entry.file_type().is_dir();
    let metadata = entry
        .metadata()
        .map_err(|e| format!("{}: {e}", entry.path().display()))?;

    let size = if is_dir { 0 } else { metadata.len() };
    let modified = epoch_seconds(&metadata);

    let ext = if is_dir {
        None
    } else {
        entry
            .path()
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
    };

    let name = entry.file_name().to_string_lossy().into_owned();

    Ok(FileNode {
        name,
        size,
        is_dir,
        children: Vec::new(),
        parent: None,
        extension: ext,
        modified,
        deleted: false,
    })
}

struct WalkAccum {
    total_files: u64,
    total_dirs: u64,
    total_bytes: u64,
    errors: u64,
    node_count: usize,
}

enum EntryAction {
    Skip,
    Process(jwalk::DirEntry<((), ())>, PathBuf),
}

/// Resolves a jwalk iterator result into either an entry to process or a skip.
///
/// On `Err`: sends `ScanError`, increments error counter, returns `Skip`.
/// On `Ok` where path == root: returns `Skip` (root already emitted).
/// On `Ok` with a `read_children_error`: sends `ScanError` for the readdir
/// failure, then falls through to return the entry for normal processing.
fn process_entry_result(
    entry_result: Result<jwalk::DirEntry<((), ())>, jwalk::Error>,
    root: &std::path::Path,
    tx: &Sender<ScanEvent>,
    acc: &mut WalkAccum,
) -> EntryAction {
    let entry = match entry_result {
        Ok(e) => e,
        Err(e) => {
            let err_path = e.path().map(|p| p.to_path_buf()).unwrap_or_default();
            warn!(path = %err_path.display(), error = %e, "jwalk error");
            let _ = tx.send(ScanEvent::ScanError {
                path: err_path,
                error: e.to_string(),
            });
            acc.errors += 1;
            return EntryAction::Skip;
        }
    };

    let entry_path = entry.path().to_path_buf();
    if entry_path == root {
        return EntryAction::Skip;
    }

    if let Some(ref read_err) = entry.read_children_error {
        warn!(
            path = %entry_path.display(),
            error = %read_err,
            "directory read error"
        );
        let _ = tx.send(ScanEvent::ScanError {
            path: entry_path.clone(),
            error: read_err.to_string(),
        });
        acc.errors += 1;
    }

    EntryAction::Process(entry, entry_path)
}

/// Converts an entry to a `FileNode`, sends `NodeDiscovered`, and updates
/// bookkeeping (stats + path-to-index map).
///
/// Returns `Err(())` when the channel is closed (receiver dropped).
fn emit_node(
    entry: &jwalk::DirEntry<((), ())>,
    entry_path: PathBuf,
    parent_idx: usize,
    tx: &Sender<ScanEvent>,
    path_to_index: &mut HashMap<PathBuf, usize>,
    acc: &mut WalkAccum,
    mut file_entries: Option<&mut Vec<FileEntry>>,
) -> Result<(), ()> {
    let node = match entry_to_node(entry) {
        Ok(n) => n,
        Err(error) => {
            warn!(path = %entry_path.display(), %error, "metadata error");
            let _ = tx.send(ScanEvent::ScanError {
                path: entry_path,
                error,
            });
            acc.errors += 1;
            return Ok(());
        }
    };
    let is_dir = node.is_dir;
    let size = node.size;

    if tx
        .send(ScanEvent::NodeDiscovered {
            node,
            parent_index: Some(parent_idx),
        })
        .is_err()
    {
        return Err(());
    }

    if is_dir {
        acc.total_dirs += 1;
    } else {
        acc.total_files += 1;
        acc.total_bytes += size;
    }

    if let Some(ref mut entries) = file_entries
        && !is_dir
        && !entry.file_type().is_symlink()
    {
        entries.push(FileEntry {
            path: entry_path.clone(),
            arena_index: acc.node_count,
            size,
        });
    }
    path_to_index.insert(entry_path, acc.node_count);
    acc.node_count += 1;
    Ok(())
}

impl Scanner {
    /// Spawns a background thread that traverses `config.root` and sends
    /// [`ScanEvent`] values to `tx`.
    ///
    /// The first event is always `NodeDiscovered` for the root with
    /// `parent_index: None`. `ScanComplete` is always the last event, even
    /// on cancellation or error. `ScanError` does not consume an arena index.
    ///
    /// `cancel` is checked at each iteration step and inside the jwalk
    /// `process_read_dir` callback for earliest possible abort (ref: DL-002).
    pub fn scan(
        config: ScanConfig,
        tx: Sender<ScanEvent>,
        cancel: Arc<AtomicBool>,
    ) -> JoinHandle<()> {
        thread::spawn(move || {
            let start = Instant::now();
            let mut path_to_index: HashMap<PathBuf, usize> = HashMap::new();
            // path_to_index is only accessed from this thread (the jwalk result
            // iterator runs on the calling thread); process_read_dir callbacks run
            // on rayon threads and never touch it, so no synchronization is needed.
            // Signals process_read_dir to stop spawning new readdir work when
            // the node count ceiling is reached on the main thread (ref: DL-003).
            let max_nodes_reached = Arc::new(AtomicBool::new(false));

            if send_root_node(&config, &tx, &mut path_to_index).is_err() {
                let _ = tx.send(ScanEvent::ScanComplete {
                    stats: ScanStats {
                        total_files: 0,
                        total_dirs: 0,
                        total_bytes: 0,
                        duration_ms: start.elapsed().as_millis() as u64,
                        errors: 1,
                    },
                });
                return;
            }

            let mut acc = WalkAccum {
                total_files: 0,
                total_dirs: 1,
                total_bytes: 0,
                errors: 0,
                node_count: 1,
            };

            let mut file_entries = if config.hash_duplicates {
                Some(Vec::new())
            } else {
                None
            };

            let exclude: Vec<Pattern> = config
                .exclude_patterns
                .iter()
                .filter_map(|p| match Pattern::new(p) {
                    Ok(pat) => Some(pat),
                    Err(e) => {
                        warn!(pattern = %p, error = %e, "invalid exclude glob pattern");
                        None
                    }
                })
                .collect();
            let exclude = Arc::new(exclude);

            Self::walk_entries(
                &config,
                &tx,
                &cancel,
                &max_nodes_reached,
                &exclude,
                &mut path_to_index,
                &mut acc,
                &mut file_entries,
            );

            if let Some(ref entries) = file_entries
                && !cancel.load(Ordering::Relaxed)
            {
                let _ = tx.send(ScanEvent::DuplicateDetectionStarted {
                    file_count: entries.len() as u64,
                });
                DuplicateDetector::find_duplicates(entries, &tx, &cancel);
            }

            debug!(
                files = acc.total_files,
                dirs = acc.total_dirs,
                bytes = acc.total_bytes,
                errors = acc.errors,
                "scan complete"
            );

            let _ = tx.send(ScanEvent::ScanComplete {
                stats: ScanStats {
                    total_files: acc.total_files,
                    total_dirs: acc.total_dirs,
                    total_bytes: acc.total_bytes,
                    duration_ms: start.elapsed().as_millis() as u64,
                    errors: acc.errors,
                },
            });
        })
    }

    /// Iterates jwalk entries and sends [`ScanEvent`] values to `tx`.
    ///
    /// jwalk configuration:
    /// - `skip_hidden(false)`: jwalk defaults to skipping hidden files; this
    ///   disables that behavior to include dotfiles in the scan (ref: DL-004).
    /// - `follow_links`: controlled by `ScanConfig::follow_symlinks`. When
    ///   true, jwalk detects symlink cycles via path-string comparison rather
    ///   than inode comparison. String comparison is more conservative (may
    ///   report false positives on hardlinked directories) but never misses
    ///   real loops. Default is false, so this code path is not exercised in
    ///   normal operation (ref: DL-009).
    /// - `process_read_dir`: runs on rayon worker threads. Clears `children`
    ///   when `cancel` or `max_nodes_reached` is true, preventing new readdir
    ///   work from being scheduled. Sets `read_children_path = None` on each
    ///   directory entry before clearing so jwalk does not schedule those
    ///   subdirectory reads (ref: DL-008).
    ///
    /// Two-level abort:
    /// 1. `process_read_dir` stops new readdir work at the rayon layer.
    /// 2. `cancel` check at iteration top discards already-queued entries.
    ///
    /// Error handling:
    /// - Iterator `Err` values: jwalk::Error (Io, Loop, ThreadpoolBusy).
    ///   Path extracted via `Error::path()`, falls back to empty PathBuf for
    ///   ThreadpoolBusy which carries no path (ref: DL-005).
    /// - `entry.read_children_error`: directory readdir failure surfaced on
    ///   an Ok entry; emitted as ScanError without consuming an arena index
    ///   (ref: DL-005).
    ///
    /// `max_nodes_reached` is set to `true` on the main thread when the node
    /// ceiling is hit. The `process_read_dir` callback reads this flag to stop
    /// spawning new work. A bool flag avoids atomic counter contention on the
    /// hot path; the main thread still enforces the exact limit (ref: DL-003).
    #[allow(clippy::too_many_arguments)]
    fn walk_entries(
        config: &ScanConfig,
        tx: &Sender<ScanEvent>,
        cancel: &Arc<AtomicBool>,
        max_nodes_reached: &Arc<AtomicBool>,
        exclude: &Arc<Vec<Pattern>>,
        path_to_index: &mut HashMap<PathBuf, usize>,
        acc: &mut WalkAccum,
        file_entries: &mut Option<Vec<FileEntry>>,
    ) {
        // Clone Arc handles for move into process_read_dir closure (ref: DL-002).
        let cancel_flag = Arc::clone(cancel);
        let mnr_flag = Arc::clone(max_nodes_reached);
        let exclude_flag = Arc::clone(exclude);

        let walker = WalkDir::new(&config.root)
            .skip_hidden(false)
            .follow_links(config.follow_symlinks)
            // process_read_dir runs on rayon threads. Sets read_children_path=None
            // first to prevent subdirectory scheduling, then clears children to
            // prevent entries from being yielded. Order is required (ref: DL-008).
            .process_read_dir(move |_depth, _path, _state, children| {
                if cancel_flag.load(Ordering::Relaxed) || mnr_flag.load(Ordering::Relaxed) {
                    for e in children.iter_mut().flatten() {
                        e.read_children_path = None;
                    }
                    children.clear();
                    return;
                }

                if !exclude_flag.is_empty() {
                    for entry_result in children.iter_mut().flatten() {
                        let name = entry_result.file_name.to_string_lossy();
                        if exclude_flag.iter().any(|p| p.matches(&name)) {
                            entry_result.read_children_path = None;
                        }
                    }
                    children.retain(|entry_result| {
                        let entry = match entry_result {
                            Ok(e) => e,
                            Err(_) => return true,
                        };
                        let name = entry.file_name.to_string_lossy();
                        !exclude_flag.iter().any(|p| p.matches(&name))
                    });
                }
            });

        for entry_result in walker {
            if cancel.load(Ordering::Relaxed) {
                break;
            }

            if let Some(max) = config.max_nodes
                && acc.node_count >= max
            {
                max_nodes_reached.store(true, Ordering::Relaxed);
                let _ = tx.send(ScanEvent::ScanError {
                    path: config.root.clone(),
                    error: format!("max_nodes limit ({max}) reached, aborting scan"),
                });
                acc.errors += 1;
                break;
            }

            let (entry, entry_path) =
                match process_entry_result(entry_result, &config.root, tx, acc) {
                    EntryAction::Skip => continue,
                    EntryAction::Process(e, p) => (e, p),
                };

            if !exclude.is_empty() {
                let name = entry.file_name().to_string_lossy();
                if exclude.iter().any(|p| p.matches(&name)) {
                    continue;
                }
            }

            let parent_path = match entry_path.parent() {
                Some(p) => p.to_path_buf(),
                None => continue,
            };

            let parent_idx = match path_to_index.get(&parent_path) {
                Some(&idx) => idx,
                None => {
                    warn!(
                        path = %entry_path.display(),
                        parent = %parent_path.display(),
                        "parent not in index map; parent was likely inaccessible"
                    );
                    let _ = tx.send(ScanEvent::ScanError {
                        path: entry_path,
                        error: "parent directory was inaccessible".to_string(),
                    });
                    acc.errors += 1;
                    continue;
                }
            };

            if emit_node(
                &entry,
                entry_path,
                parent_idx,
                tx,
                path_to_index,
                acc,
                file_entries.as_mut(),
            )
            .is_err()
            {
                return;
            }

            if acc.node_count.is_multiple_of(100) {
                let _ = tx.send(ScanEvent::Progress {
                    files_scanned: acc.total_files + acc.total_dirs,
                    bytes_scanned: acc.total_bytes,
                });
            }
        }
    }
}
