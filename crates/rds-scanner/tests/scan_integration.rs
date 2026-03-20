use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_channel::bounded;
use rds_core::scan::{ScanConfig, ScanEvent, ScanStats};
use rds_core::tree::DirTree;
use rds_scanner::Scanner;
use tempfile::TempDir;

fn build_tree_from_events(
    rx: crossbeam_channel::Receiver<ScanEvent>,
) -> (DirTree, ScanStats, Vec<(std::path::PathBuf, String)>) {
    let mut tree: Option<DirTree> = None;
    let mut stats: Option<ScanStats> = None;
    let mut errors: Vec<(std::path::PathBuf, String)> = Vec::new();

    for event in rx.iter() {
        match event {
            ScanEvent::NodeDiscovered { node, parent_index } => match parent_index {
                None => {
                    tree = Some(DirTree::from_root(node));
                }
                Some(pidx) => {
                    if let Some(ref mut t) = tree {
                        t.insert(pidx, node);
                    }
                }
            },
            ScanEvent::ScanComplete { stats: s } => {
                stats = Some(s);
            }
            ScanEvent::ScanError { path, error } => {
                errors.push((path, error));
            }
            ScanEvent::Progress { .. } => {}
            ScanEvent::DuplicateFound { .. } => {}
        }
    }

    (
        tree.expect("expected at least one NodeDiscovered event"),
        stats.expect("expected ScanComplete event"),
        errors,
    )
}

fn make_config(root: std::path::PathBuf) -> ScanConfig {
    ScanConfig {
        root,
        follow_symlinks: false,
        exclude_patterns: Vec::new(),
        hash_duplicates: false,
        max_nodes: None,
    }
}

#[test]
fn scan_basic_tree() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    fs::write(root.join("a.txt"), "hello").unwrap();
    fs::write(root.join("b.dat"), "0123456789").unwrap();
    let sub = root.join("subdir");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("c.rs"), "abc").unwrap();

    let (tx, rx) = bounded(4096);
    let cancel = Arc::new(AtomicBool::new(false));
    let config = make_config(root.to_path_buf());

    let handle = Scanner::scan(config, tx, cancel);
    handle.join().unwrap();

    let (tree, stats, errors) = build_tree_from_events(rx);

    assert_eq!(tree.len(), 5, "root + subdir + 3 files = 5 nodes");
    assert_eq!(tree.subtree_size(tree.root()), 18);
    assert_eq!(stats.total_files, 3);
    assert_eq!(stats.total_dirs, 2);
    assert_eq!(stats.total_bytes, 18);
    assert!(errors.is_empty(), "no errors expected: {errors:?}");

    // Root path must be the full absolute path, not just the last component.
    let root_path = tree.path(tree.root());
    assert_eq!(root_path, root.to_path_buf());
}

#[test]
fn scan_empty_directory() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    let (tx, rx) = bounded(4096);
    let cancel = Arc::new(AtomicBool::new(false));
    let config = make_config(root.to_path_buf());

    let handle = Scanner::scan(config, tx, cancel);
    handle.join().unwrap();

    let (tree, stats, errors) = build_tree_from_events(rx);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree.subtree_size(tree.root()), 0);
    assert_eq!(stats.total_files, 0);
    assert_eq!(stats.total_dirs, 1);
    assert_eq!(stats.total_bytes, 0);
    assert!(errors.is_empty());
}

#[test]
fn scan_respects_cancellation() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    for i in 0..100 {
        let dir = root.join(format!("dir_{i:03}"));
        fs::create_dir(&dir).unwrap();
        for j in 0..10 {
            fs::write(dir.join(format!("file_{j}.txt")), "data").unwrap();
        }
    }

    let (tx, rx) = bounded(10);
    let cancel = Arc::new(AtomicBool::new(false));
    let config = make_config(root.to_path_buf());

    let cancel_clone = cancel.clone();
    let handle = Scanner::scan(config, tx, cancel_clone);

    let mut received = 0;
    let mut got_complete = false;
    for event in rx.iter() {
        match event {
            ScanEvent::NodeDiscovered { .. } => {
                received += 1;
                if received == 5 {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            ScanEvent::ScanComplete { .. } => {
                got_complete = true;
                break;
            }
            _ => {}
        }
    }

    handle.join().unwrap();
    assert!(
        got_complete,
        "ScanComplete must be sent even on cancellation"
    );
    assert!(
        received < 1101,
        "scan should have stopped early, got {received} nodes"
    );
}

#[test]
fn scan_max_nodes_abort() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    for i in 0..10 {
        fs::write(root.join(format!("file_{i}.txt")), "x").unwrap();
    }

    let (tx, rx) = bounded(4096);
    let cancel = Arc::new(AtomicBool::new(false));
    let mut config = make_config(root.to_path_buf());
    config.max_nodes = Some(3);

    let handle = Scanner::scan(config, tx, cancel);
    handle.join().unwrap();

    let (tree, _stats, errors) = build_tree_from_events(rx);

    assert!(
        tree.len() <= 3,
        "tree should have at most 3 nodes, got {}",
        tree.len()
    );
    assert!(
        errors.iter().any(|(_, msg)| msg.contains("max_nodes")),
        "expected max_nodes error message in errors: {errors:?}"
    );
}

#[test]
fn scan_file_extensions() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    fs::write(root.join("UPPER.TXT"), "a").unwrap();
    fs::write(root.join("code.rs"), "b").unwrap();
    fs::write(root.join("archive.tar.gz"), "c").unwrap();
    fs::write(root.join("noext"), "d").unwrap();

    let (tx, rx) = bounded(4096);
    let cancel = Arc::new(AtomicBool::new(false));
    let config = make_config(root.to_path_buf());

    let handle = Scanner::scan(config, tx, cancel);
    handle.join().unwrap();

    let mut extensions: Vec<(String, Option<String>)> = Vec::new();

    let (tree, _stats, _errors) = build_tree_from_events(rx);
    for i in 0..tree.len() {
        let node = tree.get(i).unwrap();
        if !node.is_dir {
            extensions.push((node.name.clone(), node.extension.clone()));
        }
    }

    extensions.sort_by(|a, b| a.0.cmp(&b.0));

    let find_ext = |name: &str| -> Option<String> {
        extensions
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, e)| e.clone())
    };

    assert_eq!(find_ext("UPPER.TXT"), Some("txt".to_string()));
    assert_eq!(find_ext("code.rs"), Some("rs".to_string()));
    assert_eq!(find_ext("archive.tar.gz"), Some("gz".to_string()));
    assert_eq!(find_ext("noext"), None);
}

#[cfg(unix)]
#[test]
fn scan_reports_errors_and_continues() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    let denied = root.join("denied");
    fs::create_dir(&denied).unwrap();
    fs::write(denied.join("secret.txt"), "hidden").unwrap();

    let accessible = root.join("accessible");
    fs::create_dir(&accessible).unwrap();
    fs::write(accessible.join("visible.txt"), "hello").unwrap();

    fs::set_permissions(&denied, fs::Permissions::from_mode(0o000)).unwrap();

    let (tx, rx) = bounded(4096);
    let cancel = Arc::new(AtomicBool::new(false));
    let config = make_config(root.to_path_buf());

    let handle = Scanner::scan(config, tx, cancel);
    handle.join().unwrap();

    fs::set_permissions(&denied, fs::Permissions::from_mode(0o755)).unwrap();

    let (_tree, stats, errors) = build_tree_from_events(rx);

    assert!(
        stats.errors >= 1,
        "expected at least 1 error, got {}",
        stats.errors
    );
    assert!(
        !errors.is_empty(),
        "expected ScanError events for permission-denied directory"
    );
}

/// Compares Scanner::scan (jwalk RayonDefaultPool) against jwalk Serial mode
/// on a fixture of 50 directories x 20 files.
///
/// Uses jwalk Serial as the baseline; jwalk Serial runs on the calling thread
/// and is functionally identical to single-threaded traversal, without
/// requiring an additional dev-dependency (ref: DL-006, RA-002).
///
/// The fixture uses many subdirectories rather than many files in one directory
/// because jwalk parallelizes at the readdir level: speedup requires multiple
/// directories to read concurrently (ref: DL-006).
///
/// Marked `#[ignore]` to avoid flakiness on single-core CI runners (ref: R-003).
#[test]
#[ignore]
fn benchmark_parallel_vs_serial() {
    use std::time::Instant;

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    for i in 0..50 {
        let dir = root.join(format!("dir_{i:03}"));
        fs::create_dir(&dir).unwrap();
        for j in 0..20 {
            fs::write(dir.join(format!("file_{j}.dat")), "benchmark-data").unwrap();
        }
    }

    // Parallel: Scanner::scan() with default config (jwalk RayonDefaultPool)
    let parallel_start = Instant::now();
    let (tx, rx) = bounded(4096);
    let cancel = Arc::new(AtomicBool::new(false));
    let config = make_config(root.to_path_buf());
    let handle = Scanner::scan(config, tx, cancel);
    let mut parallel_nodes = 0u64;
    for event in rx.iter() {
        match event {
            ScanEvent::NodeDiscovered { .. } => parallel_nodes += 1,
            ScanEvent::ScanComplete { .. } => break,
            _ => {}
        }
    }
    handle.join().unwrap();
    let parallel_duration = parallel_start.elapsed();

    // Serial: jwalk::WalkDir with Parallelism::Serial on same fixture
    let serial_start = Instant::now();
    let serial_walker = jwalk::WalkDir::new(root)
        .skip_hidden(false)
        .parallelism(jwalk::Parallelism::Serial);
    let mut serial_count = 0u64;
    for entry in serial_walker {
        if entry.is_ok() {
            serial_count += 1;
        }
    }
    let serial_duration = serial_start.elapsed();

    eprintln!(
        "parallel: {parallel_nodes} nodes in {parallel_duration:?}, \
         serial: {serial_count} entries in {serial_duration:?}"
    );

    assert!(parallel_nodes > 0, "parallel scan should discover nodes");
    assert_eq!(
        serial_count, parallel_nodes,
        "both traversals should visit the same number of entries"
    );
    assert!(
        parallel_duration < serial_duration,
        "parallel ({parallel_duration:?}) should be faster than serial ({serial_duration:?})"
    );
}
