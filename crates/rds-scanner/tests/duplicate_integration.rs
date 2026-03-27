use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_channel::bounded;
use rds_core::scan::{ScanConfig, ScanEvent, ScanStats};
use rds_core::tree::DirTree;
use rds_scanner::Scanner;
use tempfile::TempDir;

struct ScanResult {
    #[allow(dead_code)]
    tree: DirTree,
    #[allow(dead_code)]
    stats: ScanStats,
    duplicates: Vec<([u8; 32], Vec<usize>)>,
    detection_started: bool,
}

fn build_tree_and_duplicates(rx: crossbeam_channel::Receiver<ScanEvent>) -> ScanResult {
    let mut tree: Option<DirTree> = None;
    let mut stats: Option<ScanStats> = None;
    let mut duplicates: Vec<([u8; 32], Vec<usize>)> = Vec::new();
    let mut detection_started = false;

    for event in rx.iter() {
        match event {
            ScanEvent::NodeDiscovered {
                node,
                parent_index,
                extension_name,
            } => match parent_index {
                None => {
                    tree = Some(DirTree::from_root(node));
                }
                Some(pidx) => {
                    if let Some(ref mut t) = tree {
                        let mut node = node;
                        node.extension = t.intern_extension(extension_name.as_deref());
                        t.insert(pidx, node);
                    }
                }
            },
            ScanEvent::ScanComplete { stats: s } => {
                stats = Some(s);
            }
            ScanEvent::DuplicateFound { hash, node_indices } => {
                duplicates.push((hash, node_indices));
            }
            ScanEvent::DuplicateDetectionStarted { .. } => {
                detection_started = true;
            }
            ScanEvent::ScanError { .. } => {}
            ScanEvent::Progress { .. } => {}
        }
    }

    ScanResult {
        tree: tree.expect("expected at least one NodeDiscovered event"),
        stats: stats.expect("expected ScanComplete event"),
        duplicates,
        detection_started,
    }
}

fn make_dup_config(root: std::path::PathBuf) -> ScanConfig {
    ScanConfig {
        root,
        follow_symlinks: false,
        exclude_patterns: Vec::new(),
        hash_duplicates: true,
        max_nodes: None,
    }
}

#[test]
fn exact_duplicates() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    fs::write(root.join("file_a.txt"), "duplicate content here").unwrap();
    fs::write(root.join("file_b.txt"), "duplicate content here").unwrap();
    fs::write(root.join("file_c.txt"), "duplicate content here").unwrap();

    let (tx, rx) = bounded(4096);
    let cancel = Arc::new(AtomicBool::new(false));
    let config = make_dup_config(root.to_path_buf());

    let handle = Scanner::scan(config, tx, cancel);
    handle.join().unwrap();

    let result = build_tree_and_duplicates(rx);

    assert!(
        result.detection_started,
        "DuplicateDetectionStarted must be sent"
    );
    assert_eq!(
        result.duplicates.len(),
        1,
        "expected exactly one duplicate group"
    );

    let (hash, ref indices) = result.duplicates[0];
    assert_eq!(indices.len(), 3, "expected 3 files in the duplicate group");

    let expected_hash: [u8; 32] = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"duplicate content here");
        hasher.finalize().into()
    };
    assert_eq!(
        hash, expected_hash,
        "hash must match SHA-256 of file content"
    );
}

#[test]
fn no_duplicates() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    fs::write(root.join("alpha.txt"), "content alpha").unwrap();
    fs::write(root.join("beta.txt"), "content beta").unwrap();
    fs::write(root.join("gamma.txt"), "content gamma plus extra").unwrap();

    let (tx, rx) = bounded(4096);
    let cancel = Arc::new(AtomicBool::new(false));
    let config = make_dup_config(root.to_path_buf());

    let handle = Scanner::scan(config, tx, cancel);
    handle.join().unwrap();

    let result = build_tree_and_duplicates(rx);

    assert!(
        result.detection_started,
        "DuplicateDetectionStarted must be sent"
    );
    assert!(
        result.duplicates.is_empty(),
        "expected no duplicates for unique files, got {:?}",
        result.duplicates
    );
}

#[test]
fn same_size_different_content() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    fs::write(root.join("one.bin"), "aaaa").unwrap();
    fs::write(root.join("two.bin"), "bbbb").unwrap();
    fs::write(root.join("three.bin"), "cccc").unwrap();

    let (tx, rx) = bounded(4096);
    let cancel = Arc::new(AtomicBool::new(false));
    let config = make_dup_config(root.to_path_buf());

    let handle = Scanner::scan(config, tx, cancel);
    handle.join().unwrap();

    let result = build_tree_and_duplicates(rx);

    assert!(
        result.duplicates.is_empty(),
        "same-size files with different content must not produce duplicates, got {:?}",
        result.duplicates
    );
}

#[test]
fn multiple_groups() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    fs::write(root.join("group1_a.txt"), "group one content").unwrap();
    fs::write(root.join("group1_b.txt"), "group one content").unwrap();

    fs::write(root.join("group2_a.dat"), "second group data!!").unwrap();
    fs::write(root.join("group2_b.dat"), "second group data!!").unwrap();

    fs::write(root.join("unique.log"), "only one of these").unwrap();

    let (tx, rx) = bounded(4096);
    let cancel = Arc::new(AtomicBool::new(false));
    let config = make_dup_config(root.to_path_buf());

    let handle = Scanner::scan(config, tx, cancel);
    handle.join().unwrap();

    let result = build_tree_and_duplicates(rx);

    assert_eq!(
        result.duplicates.len(),
        2,
        "expected exactly two duplicate groups, got {:?}",
        result.duplicates
    );

    for (_, indices) in &result.duplicates {
        assert_eq!(
            indices.len(),
            2,
            "each group should have 2 members, got {indices:?}"
        );
    }
}

#[test]
fn hash_duplicates_disabled() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    fs::write(root.join("dup_a.txt"), "same same same").unwrap();
    fs::write(root.join("dup_b.txt"), "same same same").unwrap();

    let (tx, rx) = bounded(4096);
    let cancel = Arc::new(AtomicBool::new(false));
    let config = ScanConfig {
        root: root.to_path_buf(),
        follow_symlinks: false,
        exclude_patterns: Vec::new(),
        hash_duplicates: false,
        max_nodes: None,
    };

    let handle = Scanner::scan(config, tx, cancel);
    handle.join().unwrap();

    let result = build_tree_and_duplicates(rx);

    assert!(
        !result.detection_started,
        "DuplicateDetectionStarted must not be sent when hash_duplicates is false"
    );
    assert!(
        result.duplicates.is_empty(),
        "no DuplicateFound events should be emitted when hash_duplicates is false"
    );
}

#[test]
fn cancel_during_detection() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    for i in 0..50 {
        let dir = root.join(format!("dir_{i:03}"));
        fs::create_dir(&dir).unwrap();
        for j in 0..10 {
            fs::write(
                dir.join(format!("dup_{j}.txt")),
                "identical file content for cancel test",
            )
            .unwrap();
        }
    }

    let (tx, rx) = bounded(4096);
    let cancel = Arc::new(AtomicBool::new(false));
    let config = make_dup_config(root.to_path_buf());

    let cancel_clone = cancel.clone();
    let handle = Scanner::scan(config, tx, cancel_clone);

    let mut node_count = 0;
    let mut got_complete = false;
    let mut dup_count = 0;
    for event in rx.iter() {
        match event {
            ScanEvent::NodeDiscovered { .. } => {
                node_count += 1;
                if node_count >= 10 {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            ScanEvent::ScanComplete { .. } => {
                got_complete = true;
                break;
            }
            ScanEvent::DuplicateFound { .. } => {
                dup_count += 1;
            }
            _ => {}
        }
    }

    handle.join().unwrap();
    assert!(
        got_complete,
        "ScanComplete must be sent even on cancellation"
    );
    // Cancellation should prevent most or all duplicate groups from being emitted.
    // Without cancellation, 50 dirs x 10 identical files would produce many groups.
    assert!(
        dup_count < 50,
        "cancellation should suppress most duplicate groups, got {dup_count}"
    );
}

#[test]
fn large_files_exercise_full_hash() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    // Files larger than 4KB to exercise partial -> full hash pipeline.
    let content: Vec<u8> = (0u8..=255).cycle().take(8192).collect();
    fs::write(root.join("large_a.bin"), &content).unwrap();
    fs::write(root.join("large_b.bin"), &content).unwrap();

    // Same size, different content past 4KB boundary.
    let mut different = content.clone();
    different[5000] ^= 0xFF;
    fs::write(root.join("large_c.bin"), &different).unwrap();

    let (tx, rx) = bounded(4096);
    let cancel = Arc::new(AtomicBool::new(false));
    let config = make_dup_config(root.to_path_buf());

    let handle = Scanner::scan(config, tx, cancel);
    handle.join().unwrap();

    let result = build_tree_and_duplicates(rx);

    assert_eq!(
        result.duplicates.len(),
        1,
        "expected one duplicate group (large_a + large_b), got {:?}",
        result.duplicates
    );
    assert_eq!(
        result.duplicates[0].1.len(),
        2,
        "expected 2 files in the duplicate group"
    );
}

#[test]
fn zero_byte_files() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    fs::write(root.join("empty_a.txt"), "").unwrap();
    fs::write(root.join("empty_b.txt"), "").unwrap();
    fs::write(root.join("empty_c.txt"), "").unwrap();
    fs::write(root.join("empty_d.log"), "").unwrap();

    let (tx, rx) = bounded(4096);
    let cancel = Arc::new(AtomicBool::new(false));
    let config = make_dup_config(root.to_path_buf());

    let handle = Scanner::scan(config, tx, cancel);
    handle.join().unwrap();

    let result = build_tree_and_duplicates(rx);

    assert!(
        result.duplicates.is_empty(),
        "zero-byte files must not produce DuplicateFound events (DL-013), got {:?}",
        result.duplicates
    );
}
