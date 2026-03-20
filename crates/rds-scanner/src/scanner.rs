use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Instant, UNIX_EPOCH};

use crossbeam_channel::Sender;
use rds_core::scan::{ScanConfig, ScanEvent, ScanStats};
use rds_core::tree::FileNode;
use tracing::{debug, warn};
use walkdir::WalkDir;

/// Single-threaded filesystem scanner. Spawn via [`Scanner::scan`].
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
    let root_name = config
        .root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| config.root.to_string_lossy().into_owned());

    let root_node = FileNode {
        name: root_name,
        size: 0,
        is_dir: true,
        children: Vec::new(),
        parent: None,
        extension: None,
        modified: root_modified,
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

fn entry_to_node(entry: &walkdir::DirEntry) -> FileNode {
    let is_dir = entry.file_type().is_dir();
    let (size, modified) = match entry.metadata() {
        Ok(ref m) => {
            let sz = if is_dir { 0 } else { m.len() };
            (sz, epoch_seconds(m))
        }
        Err(_) => (0, None),
    };

    let ext = if is_dir {
        None
    } else {
        entry
            .path()
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
    };

    let name = entry.file_name().to_string_lossy().into_owned();

    FileNode {
        name,
        size,
        is_dir,
        children: Vec::new(),
        parent: None,
        extension: ext,
        modified,
    }
}

struct WalkAccum {
    total_files: u64,
    total_dirs: u64,
    total_bytes: u64,
    errors: u64,
    node_count: usize,
}

impl Scanner {
    pub fn scan(
        config: ScanConfig,
        tx: Sender<ScanEvent>,
        cancel: Arc<AtomicBool>,
    ) -> JoinHandle<()> {
        thread::spawn(move || {
            let start = Instant::now();
            let mut path_to_index: HashMap<PathBuf, usize> = HashMap::new();

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

            Self::walk_entries(&config, &tx, &cancel, &mut path_to_index, &mut acc);

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

    fn walk_entries(
        config: &ScanConfig,
        tx: &Sender<ScanEvent>,
        cancel: &Arc<AtomicBool>,
        path_to_index: &mut HashMap<PathBuf, usize>,
        acc: &mut WalkAccum,
    ) {
        let walker = WalkDir::new(&config.root).follow_links(config.follow_symlinks);

        for entry_result in walker {
            if cancel.load(Ordering::Relaxed) {
                break;
            }

            if let Some(max) = config.max_nodes
                && acc.node_count >= max
            {
                let _ = tx.send(ScanEvent::ScanError {
                    path: config.root.clone(),
                    error: format!("max_nodes limit ({max}) reached, aborting scan"),
                });
                acc.errors += 1;
                break;
            }

            let entry = match entry_result {
                Ok(e) => e,
                Err(e) => {
                    let err_path = e.path().map(|p| p.to_path_buf()).unwrap_or_default();
                    warn!(path = %err_path.display(), error = %e, "walkdir error");
                    let _ = tx.send(ScanEvent::ScanError {
                        path: err_path,
                        error: e.to_string(),
                    });
                    acc.errors += 1;
                    continue;
                }
            };

            let entry_path = entry.path().to_path_buf();
            if entry_path == config.root {
                continue;
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

            let node = entry_to_node(&entry);
            let is_dir = node.is_dir;
            let size = node.size;

            if tx
                .send(ScanEvent::NodeDiscovered {
                    node,
                    parent_index: Some(parent_idx),
                })
                .is_err()
            {
                return;
            }

            if is_dir {
                acc.total_dirs += 1;
            } else {
                acc.total_files += 1;
                acc.total_bytes += size;
            }

            path_to_index.insert(entry_path, acc.node_count);
            acc.node_count += 1;

            if acc.node_count.is_multiple_of(100) {
                let _ = tx.send(ScanEvent::Progress {
                    files_scanned: acc.total_files + acc.total_dirs,
                    bytes_scanned: acc.total_bytes,
                });
            }
        }
    }
}
