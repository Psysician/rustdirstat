use std::fs;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crossbeam_channel::bounded;
use rds_core::scan::{ScanConfig, ScanEvent};
use rds_scanner::Scanner;
use tempfile::TempDir;

fn collect_discovered_names(rx: crossbeam_channel::Receiver<ScanEvent>) -> Vec<String> {
    let mut names = Vec::new();
    for event in rx.iter() {
        match event {
            ScanEvent::NodeDiscovered { node, parent_index } => {
                if parent_index.is_some() {
                    names.push(node.name.clone());
                }
            }
            ScanEvent::ScanComplete { .. } => break,
            _ => {}
        }
    }
    names
}

#[test]
fn exclude_patterns_filter_dirs_and_files() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    let nm = root.join("node_modules");
    fs::create_dir(&nm).unwrap();
    fs::write(nm.join("package.json"), "{}").unwrap();

    fs::write(root.join("notes.tmp"), "scratch").unwrap();
    fs::write(root.join("data.tmp"), "scratch2").unwrap();

    let src = root.join("src");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("main.rs"), "fn main() {}").unwrap();
    fs::write(root.join("README.md"), "# hello").unwrap();

    let (tx, rx) = bounded(4096);
    let cancel = Arc::new(AtomicBool::new(false));
    let config = ScanConfig {
        root: root.to_path_buf(),
        follow_symlinks: false,
        exclude_patterns: vec!["node_modules".into(), "*.tmp".into()],
        hash_duplicates: false,
        max_nodes: None,
    };

    let handle = Scanner::scan(config, tx, cancel);
    handle.join().unwrap();

    let names = collect_discovered_names(rx);

    assert!(
        !names.contains(&"node_modules".to_string()),
        "node_modules directory should be excluded, got: {names:?}"
    );
    assert!(
        !names.contains(&"package.json".to_string()),
        "files inside node_modules should be excluded, got: {names:?}"
    );
    assert!(
        !names.contains(&"notes.tmp".to_string()),
        "*.tmp files should be excluded, got: {names:?}"
    );
    assert!(
        !names.contains(&"data.tmp".to_string()),
        "*.tmp files should be excluded, got: {names:?}"
    );

    assert!(
        names.contains(&"src".to_string()),
        "src directory should be present, got: {names:?}"
    );
    assert!(
        names.contains(&"main.rs".to_string()),
        "main.rs should be present, got: {names:?}"
    );
    assert!(
        names.contains(&"README.md".to_string()),
        "README.md should be present, got: {names:?}"
    );
}

#[test]
fn exclude_empty_patterns_scans_everything() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    let nm = root.join("node_modules");
    fs::create_dir(&nm).unwrap();
    fs::write(nm.join("index.js"), "module").unwrap();
    fs::write(root.join("app.tmp"), "temp").unwrap();

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

    let names = collect_discovered_names(rx);

    assert!(
        names.contains(&"node_modules".to_string()),
        "node_modules should be present with no exclude patterns, got: {names:?}"
    );
    assert!(
        names.contains(&"index.js".to_string()),
        "index.js should be present with no exclude patterns, got: {names:?}"
    );
    assert!(
        names.contains(&"app.tmp".to_string()),
        "app.tmp should be present with no exclude patterns, got: {names:?}"
    );
}
