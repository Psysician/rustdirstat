//! File actions for the GUI (delete, open in file manager).
//!
//! `execute_delete` sends a file/directory to the OS trash and tombstones the
//! arena node. `cleanup_duplicate_groups` prunes stale entries from duplicate
//! groups after deletions. `open_in_file_manager` reveals a file or directory
//! in the platform's native file manager.

use rds_core::tree::DirTree;

use crate::DuplicateGroup;

/// Sends the filesystem entry at `index` to the OS trash and tombstones the
/// corresponding arena subtree.
///
/// Returns `Ok(freed_bytes)` on success (subtree size before tombstoning) or
/// `Err` with the trash error message if the OS trash operation fails.
/// No tombstoning occurs on failure.
pub(crate) fn execute_delete(tree: &mut DirTree, index: usize) -> Result<u64, String> {
    let path = tree.path(index);
    let freed_bytes = tree.subtree_size(index);
    trash::delete(&path).map_err(|e| e.to_string())?;
    tree.tombstone(index);
    Ok(freed_bytes)
}

/// Removes deleted nodes from duplicate groups and recalculates wasted bytes.
///
/// For each group: retains only indices where the node exists and is not
/// deleted, recalculates `wasted_bytes` from the remaining members, and
/// removes groups with fewer than 2 members. Re-sorts by `wasted_bytes`
/// descending.
pub(crate) fn cleanup_duplicate_groups(groups: &mut Vec<DuplicateGroup>, tree: &DirTree) {
    for group in groups.iter_mut() {
        group
            .node_indices
            .retain(|&idx| tree.get(idx).is_some_and(|n| !n.deleted));

        let file_size = group
            .node_indices
            .first()
            .and_then(|&idx| tree.get(idx))
            .map(|n| n.size)
            .unwrap_or(0);
        group.wasted_bytes =
            file_size.saturating_mul(group.node_indices.len().saturating_sub(1) as u64);
    }

    groups.retain(|g| g.node_indices.len() >= 2);
    groups.sort_by(|a, b| b.wasted_bytes.cmp(&a.wasted_bytes));
}

/// Opens the file manager for the filesystem entry at `index`.
///
/// For directories, opens the directory directly. For files, uses
/// platform-specific commands to reveal (select) the file in its parent
/// directory.
pub(crate) fn open_in_file_manager(tree: &DirTree, index: usize) -> Result<(), String> {
    let node = tree
        .get(index)
        .ok_or_else(|| format!("node at index {index} not found"))?;
    let path = tree.path(index);

    let result = if node.is_dir {
        open::that_detached(&path).map_err(|e| e.to_string())
    } else {
        open_file_revealing(&path)
    };

    match &result {
        Ok(()) => tracing::debug!("opened in file manager: {}", path.display()),
        Err(e) => tracing::warn!("failed to open in file manager: {}: {e}", path.display()),
    }

    result
}

/// Platform-specific file reveal: selects the file in the native file manager.
fn open_file_revealing(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // `raw_arg` bypasses Rust's automatic quoting so explorer sees the
        // `/select,"<path>"` token exactly as intended, even when the path
        // contains spaces, commas, or cmd metacharacters like `&` or `%`.
        use std::os::windows::process::CommandExt;
        let child = std::process::Command::new("explorer")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn()
            .map_err(|e| e.to_string())?;
        std::thread::spawn(move || { let _ = child.wait(); });
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        let child = std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
        std::thread::spawn(move || { let _ = child.wait(); });
        Ok(())
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let target = path.parent().unwrap_or(path);
        open::that_detached(target).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rds_core::tree::FileNode;

    fn make_file_node(name: &str, size: u64) -> FileNode {
        FileNode {
            name: name.to_string(),
            size,
            is_dir: false,
            children: Vec::new(),
            parent: None,
            extension: None,
            modified: None,
            deleted: false,
        }
    }

    fn make_dir_node(name: &str) -> FileNode {
        FileNode {
            name: name.to_string(),
            size: 0,
            is_dir: true,
            children: Vec::new(),
            parent: None,
            extension: None,
            modified: None,
            deleted: false,
        }
    }

    #[test]
    #[ignore]
    fn open_file_in_file_manager() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root_path = tmp.path();
        let subdir_path = root_path.join("subdir");
        std::fs::create_dir(&subdir_path).unwrap();
        let file_path = subdir_path.join("test.txt");
        std::fs::write(&file_path, "hello").unwrap();

        let mut tree = DirTree::new(root_path.to_str().unwrap());
        let subdir_idx = tree.insert(0, make_dir_node("subdir"));
        let file_idx = tree.insert(subdir_idx, make_file_node("test.txt", 5));

        let result = open_in_file_manager(&tree, file_idx);
        assert!(result.is_ok(), "open file failed: {:?}", result.err());
    }

    #[test]
    #[ignore]
    fn open_directory_in_file_manager() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root_path = tmp.path();
        let subdir_path = root_path.join("subdir");
        std::fs::create_dir(&subdir_path).unwrap();
        let file_path = subdir_path.join("test.txt");
        std::fs::write(&file_path, "hello").unwrap();

        let mut tree = DirTree::new(root_path.to_str().unwrap());
        let subdir_idx = tree.insert(0, make_dir_node("subdir"));
        tree.insert(subdir_idx, make_file_node("test.txt", 5));

        let result = open_in_file_manager(&tree, subdir_idx);
        assert!(result.is_ok(), "open dir failed: {:?}", result.err());
    }

    #[test]
    #[ignore]
    fn open_root_in_file_manager() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root_path = tmp.path();
        let subdir_path = root_path.join("subdir");
        std::fs::create_dir(&subdir_path).unwrap();
        let file_path = subdir_path.join("test.txt");
        std::fs::write(&file_path, "hello").unwrap();

        let mut tree = DirTree::new(root_path.to_str().unwrap());
        let subdir_idx = tree.insert(0, make_dir_node("subdir"));
        tree.insert(subdir_idx, make_file_node("test.txt", 5));

        let result = open_in_file_manager(&tree, 0);
        assert!(result.is_ok(), "open root failed: {:?}", result.err());
    }

    #[test]
    fn open_nonexistent_path_returns_result() {
        let mut tree = DirTree::new("/nonexistent/path/that/does/not/exist");
        let file_idx = tree.insert(0, make_file_node("ghost.txt", 42));

        let result = open_in_file_manager(&tree, file_idx);
        // The function should not panic. On Linux, xdg-open may not fail
        // immediately for nonexistent paths, so we just verify the function
        // compiles, doesn't panic, and returns a Result.
        let _ = result;
    }

    #[test]
    fn open_invalid_index_returns_error() {
        let tree = DirTree::new("/some/root");
        let result = open_in_file_manager(&tree, 999);
        assert!(result.is_err());
    }
}
