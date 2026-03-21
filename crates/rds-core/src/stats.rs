use crate::tree::DirTree;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HslColor {
    pub h: f64,
    pub s: f64,
    pub l: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionStats {
    pub extension: String,
    pub count: u64,
    pub total_bytes: u64,
    pub percentage: f64,
    pub color: HslColor,
}

/// Returns a deterministic `HslColor` for `ext`.
///
/// Uses byte-sum modulo 360 rather than `DefaultHasher`. `DefaultHasher` is
/// randomly seeded per process (Rust 1.36+), so it produces different results
/// on each app launch. Byte-sum is deterministic across runs, platforms, and
/// Rust versions. (DL-010)
pub fn color_for_extension(ext: &str) -> HslColor {
    let hue: u64 = ext.as_bytes().iter().map(|&b| b as u64).sum();
    HslColor {
        h: (hue % 360) as f64,
        s: 0.7,
        l: 0.5,
    }
}

/// Aggregates per-extension statistics across all file nodes in `tree`.
///
/// Directories are excluded. Files with no extension are grouped under the
/// empty string key `""`. `percentage` is relative to total file bytes (not
/// total tree bytes including directories). Sorted by `total_bytes` descending.
pub fn compute_extension_stats(tree: &DirTree) -> Vec<ExtensionStats> {
    let mut groups: HashMap<String, (u64, u64)> = HashMap::new();
    let mut total_file_bytes: u64 = 0;

    for i in 0..tree.len() {
        if let Some(node) = tree.get(i)
            && !node.is_dir
            && !node.deleted
        {
            let ext = node.extension.clone().unwrap_or_default();
            let entry = groups.entry(ext).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += node.size;
            total_file_bytes += node.size;
        }
    }

    let mut stats: Vec<ExtensionStats> = groups
        .into_iter()
        .map(|(ext, (count, total_bytes))| {
            let percentage = if total_file_bytes > 0 {
                (total_bytes as f64 / total_file_bytes as f64) * 100.0
            } else {
                0.0
            };
            let color = color_for_extension(&ext);
            ExtensionStats {
                extension: ext,
                count,
                total_bytes,
                percentage,
                color,
            }
        })
        .collect();

    stats.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::FileNode;

    #[test]
    fn color_determinism() {
        let c1 = color_for_extension("rs");
        let c2 = color_for_extension("rs");
        assert_eq!(c1, c2);

        let c3 = color_for_extension("rs");
        assert_eq!(c1, c3);
    }

    #[test]
    fn color_different_extensions() {
        let c_rs = color_for_extension("rs");
        let c_py = color_for_extension("py");
        let c_js = color_for_extension("js");
        assert_ne!(c_rs.h, c_py.h);
        assert_ne!(c_rs.h, c_js.h);
        assert_ne!(c_py.h, c_js.h);
    }

    #[test]
    fn color_fixed_saturation_lightness() {
        let c = color_for_extension("txt");
        assert_eq!(c.s, 0.7);
        assert_eq!(c.l, 0.5);
    }

    #[test]
    fn compute_stats_on_small_tree() {
        let mut tree = DirTree::new("/root");

        let file_a = FileNode {
            name: "a.rs".to_string(),
            size: 1000,
            is_dir: false,
            children: Vec::new(),
            parent: None,
            extension: Some("rs".to_string()),
            modified: None,
            deleted: false,
        };
        tree.insert(0, file_a);

        let file_b = FileNode {
            name: "b.rs".to_string(),
            size: 500,
            is_dir: false,
            children: Vec::new(),
            parent: None,
            extension: Some("rs".to_string()),
            modified: None,
            deleted: false,
        };
        tree.insert(0, file_b);

        let file_c = FileNode {
            name: "c.txt".to_string(),
            size: 500,
            is_dir: false,
            children: Vec::new(),
            parent: None,
            extension: Some("txt".to_string()),
            modified: None,
            deleted: false,
        };
        tree.insert(0, file_c);

        let stats = compute_extension_stats(&tree);
        assert_eq!(stats.len(), 2);
        assert_eq!(stats[0].extension, "rs");
        assert_eq!(stats[0].count, 2);
        assert_eq!(stats[0].total_bytes, 1500);
        assert_eq!(stats[0].percentage, 75.0);
        assert_eq!(stats[1].extension, "txt");
        assert_eq!(stats[1].count, 1);
        assert_eq!(stats[1].total_bytes, 500);
        assert_eq!(stats[1].percentage, 25.0);
    }

    #[test]
    fn no_extension_files_grouped_under_empty_key() {
        let mut tree = DirTree::new("/root");
        let no_ext = FileNode {
            name: "Makefile".to_string(),
            size: 200,
            is_dir: false,
            children: Vec::new(),
            parent: None,
            extension: None,
            modified: None,
            deleted: false,
        };
        tree.insert(0, no_ext);
        let stats = compute_extension_stats(&tree);
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].extension, "");
        assert_eq!(stats[0].count, 1);
        assert_eq!(stats[0].total_bytes, 200);
    }

    #[test]
    fn compute_stats_excludes_deleted_file() {
        let mut tree = DirTree::new("/root");

        let file_a = FileNode {
            name: "a.rs".to_string(),
            size: 1000,
            is_dir: false,
            children: Vec::new(),
            parent: None,
            extension: Some("rs".to_string()),
            modified: None,
            deleted: false,
        };
        let idx_a = tree.insert(0, file_a);

        let file_b = FileNode {
            name: "b.rs".to_string(),
            size: 500,
            is_dir: false,
            children: Vec::new(),
            parent: None,
            extension: Some("rs".to_string()),
            modified: None,
            deleted: false,
        };
        tree.insert(0, file_b);

        let file_c = FileNode {
            name: "c.txt".to_string(),
            size: 500,
            is_dir: false,
            children: Vec::new(),
            parent: None,
            extension: Some("txt".to_string()),
            modified: None,
            deleted: false,
        };
        tree.insert(0, file_c);

        // Tombstone a.rs (1000 bytes).
        tree.tombstone(idx_a);

        let stats = compute_extension_stats(&tree);
        // Only b.rs (500) and c.txt (500) remain — equal bytes, sort is stable by bytes desc.
        assert_eq!(stats.len(), 2);

        let rs_stat = stats.iter().find(|s| s.extension == "rs").unwrap();
        assert_eq!(rs_stat.count, 1, "only b.rs should remain");
        assert_eq!(rs_stat.total_bytes, 500);

        let txt_stat = stats.iter().find(|s| s.extension == "txt").unwrap();
        assert_eq!(txt_stat.count, 1);
        assert_eq!(txt_stat.total_bytes, 500);
        assert_eq!(txt_stat.percentage, 50.0);
    }

    #[test]
    fn compute_stats_deleted_file_removes_extension_group() {
        let mut tree = DirTree::new("/root");

        let file_a = FileNode {
            name: "a.rs".to_string(),
            size: 1000,
            is_dir: false,
            children: Vec::new(),
            parent: None,
            extension: Some("rs".to_string()),
            modified: None,
            deleted: false,
        };
        tree.insert(0, file_a);

        let file_c = FileNode {
            name: "only.txt".to_string(),
            size: 500,
            is_dir: false,
            children: Vec::new(),
            parent: None,
            extension: Some("txt".to_string()),
            modified: None,
            deleted: false,
        };
        let idx_c = tree.insert(0, file_c);

        // Tombstone the only .txt file.
        tree.tombstone(idx_c);

        let stats = compute_extension_stats(&tree);
        // txt extension should be completely gone.
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].extension, "rs");
        assert_eq!(stats[0].count, 1);
        assert_eq!(stats[0].total_bytes, 1000);
        assert_eq!(stats[0].percentage, 100.0);
    }
}
