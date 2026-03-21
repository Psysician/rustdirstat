//! Delete and cleanup actions for the GUI.
//!
//! `execute_delete` sends a file/directory to the OS trash and tombstones the
//! arena node. `cleanup_duplicate_groups` prunes stale entries from duplicate
//! groups after deletions.

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
