//! Scan events, configuration, and summary statistics.
//!
//! `ScanEvent` crosses a bounded crossbeam channel from the scanner thread to
//! the GUI thread, so all variants must be `Send + Sync`. `ScanError` stores
//! the error as `String` because `std::io::Error` does not implement
//! `serde::Serialize`, making it incompatible with derive macros on the enum.
//! (DL-004)

use crate::tree::FileNode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub enum ScanEvent {
    NodeDiscovered {
        node: FileNode,
        parent_index: Option<usize>,
        /// Raw extension string for the GUI to intern into the DirTree's
        /// extension table. The scanner sets `node.extension = 0` (placeholder)
        /// and passes the actual extension string here.
        extension_name: Option<Box<str>>,
        /// Node name passed separately because the scanner does not have
        /// access to the DirTree's name buffer. The GUI appends this to
        /// the buffer when inserting the node.
        node_name: Box<str>,
    },
    Progress {
        files_scanned: u64,
        bytes_scanned: u64,
    },
    DuplicateFound {
        /// SHA-256 of the file content. Retained for future "copy hash" /
        /// verify actions; the GUI currently stores only node_indices.
        hash: [u8; 32],
        node_indices: Vec<usize>,
    },
    DuplicateDetectionStarted {
        file_count: u64,
    },
    ScanComplete {
        stats: ScanStats,
    },
    ScanError {
        path: PathBuf,
        error: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScanStats {
    pub total_files: u64,
    pub total_dirs: u64,
    pub total_bytes: u64,
    pub duration_ms: u64,
    pub errors: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScanConfig {
    pub root: PathBuf,
    pub follow_symlinks: bool,
    pub exclude_patterns: Vec<String>,
    pub hash_duplicates: bool,
    pub max_nodes: Option<usize>,
}

impl Default for ScanConfig {
    fn default() -> Self {
        #[cfg(target_os = "windows")]
        let exclude_patterns = vec![
            "$RECYCLE.BIN".to_string(),
            "System Volume Information".to_string(),
            "WindowsApps".to_string(),
        ];
        #[cfg(not(target_os = "windows"))]
        let exclude_patterns = Vec::new();

        ScanConfig {
            root: PathBuf::new(),
            follow_symlinks: false,
            exclude_patterns,
            hash_duplicates: false,
            max_nodes: Some(10_000_000),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::NO_PARENT;

    #[test]
    fn scan_config_defaults() {
        let config = ScanConfig::default();
        assert!(!config.follow_symlinks);
        assert!(!config.hash_duplicates);
        assert_eq!(config.max_nodes, Some(10_000_000));
        assert_eq!(
            config.exclude_patterns,
            ScanConfig::default().exclude_patterns
        );
    }

    #[test]
    fn scan_event_node_discovered_construction() {
        let node = FileNode {
            name_offset: 0,
            name_len: 0,
            size: 1024,
            first_child: u32::MAX,
            next_sibling: u32::MAX,
            modified: 0,
            parent: NO_PARENT,
            extension: 0,
            flags: 0,
        };
        let event = ScanEvent::NodeDiscovered {
            node: node.clone(),
            parent_index: Some(0),
            extension_name: Some("txt".into()),
            node_name: "test.txt".into(),
        };
        if let ScanEvent::NodeDiscovered {
            node: n,
            parent_index,
            extension_name,
            node_name,
        } = event
        {
            assert_eq!(&*node_name, "test.txt");
            assert_eq!(n.size, 1024);
            assert_eq!(parent_index, Some(0));
            assert_eq!(extension_name.as_deref(), Some("txt"));
        } else {
            panic!("Expected NodeDiscovered variant");
        }
    }

    #[test]
    fn scan_event_is_send_and_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<ScanEvent>();
        assert_sync::<ScanEvent>();
    }

    #[test]
    fn scan_config_is_send_and_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<ScanConfig>();
        assert_sync::<ScanConfig>();
    }
}
