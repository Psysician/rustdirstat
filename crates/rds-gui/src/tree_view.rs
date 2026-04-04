//! Directory tree panel with expand/collapse navigation.
//!
//! Renders the `DirTree` as an indented tree sorted by subtree size
//! descending. `SubtreeStats` caches sizes and file counts via a
//! single O(n) bottom-up pass so rendering never re-traverses the
//! arena. (ref: DL-001)

use std::cmp::Reverse;
use std::collections::HashMap;
use std::collections::HashSet;

use rds_core::CustomCommand;
use rds_core::SortOrder;
use rds_core::tree::DirTree;

use crate::PendingDelete;

/// Horizontal pixels per tree depth level.
const INDENT_PER_LEVEL: f32 = 20.0;
/// Horizontal spacer matching the expand/collapse button width.
const TOGGLE_BUTTON_WIDTH: f32 = 18.0;

/// Cached subtree sizes and file counts. Computed in a single
/// bottom-up pass over the arena — O(n) total, O(1) per lookup.
pub struct SubtreeStats {
    sizes: Vec<u64>,
    file_counts: Vec<u32>,
}

impl SubtreeStats {
    pub fn compute(tree: &DirTree) -> Self {
        let len = tree.len();
        let mut sizes = vec![0u64; len];
        let mut file_counts = vec![0u32; len];

        // Initialize with each node's own values.
        // Deleted (tombstoned) nodes contribute nothing.
        for i in 0..len {
            if let Some(node) = tree.get(i)
                && !node.deleted()
            {
                sizes[i] = node.size;
                if !node.is_dir() {
                    file_counts[i] = 1;
                }
            }
        }

        // Bottom-up accumulation. Arena order is parent-before-child
        // (depth-first insertion), so reverse iteration visits children
        // before parents — each child's accumulated total is final when
        // added to its parent.
        // Deleted nodes are skipped so their values never propagate upward.
        for i in (1..len).rev() {
            if let Some(node) = tree.get(i)
                && !node.deleted()
                && node.parent != rds_core::tree::NO_PARENT
            {
                let parent = node.parent as usize;
                sizes[parent] += sizes[i];
                file_counts[parent] += file_counts[i];
            }
        }

        Self { sizes, file_counts }
    }

    pub fn size(&self, index: usize) -> u64 {
        self.sizes.get(index).copied().unwrap_or(0)
    }

    pub fn file_count(&self, index: usize) -> u32 {
        self.file_counts.get(index).copied().unwrap_or(0)
    }
}

/// Expand/collapse state for the directory tree panel.
pub(crate) struct TreeViewState {
    expanded: HashSet<usize>,
    /// Tracks the last selection value synced from external sources (e.g., treemap click).
    /// Used to detect when selection changed externally. (ref: DL-003)
    last_synced_selection: Option<usize>,
    /// When true, the next render of the selected node calls `scroll_to_me`. (ref: DL-003)
    pending_scroll: bool,
    /// Cached sorted children per directory index. Invalidated when the tree changes.
    sorted_cache: HashMap<usize, Vec<usize>>,
    /// The sort order used to build `sorted_cache`. If sort_order changes, cache is cleared.
    cached_sort_order: Option<SortOrder>,
}

impl TreeViewState {
    pub fn new() -> Self {
        Self {
            expanded: HashSet::new(),
            last_synced_selection: None,
            pending_scroll: false,
            sorted_cache: HashMap::new(),
            cached_sort_order: None,
        }
    }

    pub fn reset(&mut self) {
        self.expanded.clear();
        self.last_synced_selection = None;
        self.pending_scroll = false;
        self.sorted_cache.clear();
        self.cached_sort_order = None;
    }

    /// Invalidates the sorted-children cache. Call after tree structure changes
    /// (node insertion, deletion) so stale sort results are not used.
    pub fn invalidate_sorted_cache(&mut self) {
        self.sorted_cache.clear();
        self.cached_sort_order = None;
    }

    pub fn toggle(&mut self, index: usize) {
        if !self.expanded.remove(&index) {
            self.expanded.insert(index);
        }
    }

    pub fn is_expanded(&self, index: usize) -> bool {
        self.expanded.contains(&index)
    }

    pub fn expand(&mut self, index: usize) {
        self.expanded.insert(index);
    }
}

/// Returns child indices of `index` sorted according to `sort_order`.
pub(crate) fn sorted_children(
    tree: &DirTree,
    index: usize,
    stats: &SubtreeStats,
    sort_order: SortOrder,
) -> Vec<usize> {
    let mut children: Vec<usize> = tree
        .children(index)
        .map(|c| c as usize)
        .filter(|&c| tree.get(c).is_some_and(|n| !n.deleted()))
        .collect();
    match sort_order {
        SortOrder::SizeDesc => children.sort_by(|&a, &b| stats.size(b).cmp(&stats.size(a))),
        SortOrder::SizeAsc => children.sort_by(|&a, &b| stats.size(a).cmp(&stats.size(b))),
        SortOrder::NameAsc => {
            children.sort_by_cached_key(|&c| tree.name(c).to_lowercase());
        }
        SortOrder::NameDesc => {
            children.sort_by_cached_key(|&c| Reverse(tree.name(c).to_lowercase()));
        }
    }
    children
}

/// Expands all ancestor directories of `index` so the node becomes visible
/// in the tree view. Walks from `index` up to the root via parent pointers,
/// expanding each parent. Idempotent — already-expanded nodes stay expanded.
/// (ref: DL-004)
fn expand_ancestors(tree: &DirTree, state: &mut TreeViewState, index: usize) {
    let mut current = index;
    while let Some(node) = tree.get(current) {
        if node.parent != rds_core::tree::NO_PARENT {
            let parent = node.parent as usize;
            state.expand(parent);
            current = parent;
        } else {
            break;
        }
    }
}

/// Renders the directory tree inside a scrollable area.
#[allow(clippy::too_many_arguments)]
pub(crate) fn show(
    tree: &DirTree,
    stats: &SubtreeStats,
    state: &mut TreeViewState,
    selected: &mut Option<usize>,
    scan_complete: bool,
    pending_delete: &mut Option<PendingDelete>,
    custom_commands: &[CustomCommand],
    sort_order: SortOrder,
    notifications: &mut crate::notifications::Notifications,
    theme_colors: &crate::ThemeColors,
    ui: &mut egui::Ui,
) {
    // Detect external selection change (e.g., treemap click).
    // Expand ancestors and queue scroll so the selected node becomes visible. (ref: DL-003)
    if *selected != state.last_synced_selection {
        if let Some(idx) = *selected {
            expand_ancestors(tree, state, idx);
            state.pending_scroll = true;
        }
        state.last_synced_selection = *selected;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        render_node(
            tree,
            tree.root(),
            stats,
            state,
            selected,
            scan_complete,
            pending_delete,
            custom_commands,
            sort_order,
            notifications,
            theme_colors,
            ui,
            0,
        );
    });
}

/// Renders a single tree node and, if expanded, its children recursively.
/// Only expanded branches are visited, keeping per-frame cost proportional
/// to visible rows. (ref: DL-007)
#[allow(clippy::too_many_arguments)]
fn render_node(
    tree: &DirTree,
    index: usize,
    stats: &SubtreeStats,
    state: &mut TreeViewState,
    selected: &mut Option<usize>,
    scan_complete: bool,
    pending_delete: &mut Option<PendingDelete>,
    custom_commands: &[CustomCommand],
    sort_order: SortOrder,
    notifications: &mut crate::notifications::Notifications,
    theme_colors: &crate::ThemeColors,
    ui: &mut egui::Ui,
    depth: usize,
) {
    let node = match tree.get(index) {
        Some(n) => n,
        None => return,
    };

    let is_dir = node.is_dir();
    let has_children = tree.get(index).is_some_and(|n| n.first_child != u32::MAX);
    let is_expanded = is_dir && has_children && state.is_expanded(index);
    let is_selected = *selected == Some(index);

    let indent = depth as f32 * INDENT_PER_LEVEL;

    ui.horizontal(|ui| {
        ui.add_space(indent);

        // Expand/collapse toggle for directories with children.
        if is_dir && has_children {
            let icon = if is_expanded { "\u{25BC}" } else { "\u{25B6}" };
            if ui.small_button(icon).clicked() {
                state.toggle(index);
            }
        } else {
            // Spacer aligned with toggle button width.
            ui.add_space(TOGGLE_BUTTON_WIDTH);
        }

        // Build label: name + size + file count (dirs only).
        let size = stats.size(index);
        let name = tree.name(index);
        let is_empty_dir = is_dir
            && tree
                .children(index)
                .all(|c| tree.get(c as usize).is_none_or(|n| n.deleted()));
        let display_name = if is_empty_dir {
            format!("{} (empty)", name)
        } else {
            name.to_string()
        };
        let label_text = if is_dir {
            let count = stats.file_count(index);
            format!(
                "{}  {}  ({} files)",
                display_name,
                super::format_bytes(size),
                count,
            )
        } else {
            format!("{}  {}", name, super::format_bytes(size))
        };

        let rich_label = if is_empty_dir {
            egui::RichText::new(&label_text).color(theme_colors.placeholder_text)
        } else {
            egui::RichText::new(&label_text)
        };
        let response = ui.selectable_label(is_selected, rich_label);
        if response.clicked() {
            *selected = Some(index);
            // Update sync tracker immediately so show() doesn't treat this as
            // an external change on the next frame. (ref: DL-004)
            state.last_synced_selection = Some(index);
        }
        // Scroll to the selected node when selection changed externally.
        if is_selected && state.pending_scroll {
            response.scroll_to_me(Some(egui::Align::Center));
            state.pending_scroll = false;
        }

        // Right-click context menu (only when scan is complete).
        // Re-interact to ensure secondary click detection works inside
        // ScrollArea (whose drag sensing can shadow click responses).
        if scan_complete {
            response.interact(egui::Sense::click()).context_menu(|ui| {
                if ui.button("Open in File Manager").clicked() {
                    if let Err(e) = crate::actions::open_in_file_manager(tree, index) {
                        notifications.error(format!("Failed to open: {e}"));
                    }
                    ui.close();
                }
                crate::actions::show_custom_commands_menu(
                    ui,
                    tree,
                    index,
                    custom_commands,
                    notifications,
                );
                if index != tree.root() {
                    ui.separator();
                    if ui.button("Delete").clicked() {
                        let path = tree.path(index);
                        let size = if is_dir { stats.size(index) } else { node.size };
                        *pending_delete = Some(PendingDelete {
                            node_index: index,
                            path_display: path.display().to_string(),
                            size_bytes: size,
                            is_dir,
                        });
                        ui.close();
                    }
                }
            });
        }
    });

    // Recurse into children sorted according to sort_order.
    // Sort results are cached in TreeViewState and invalidated when the tree changes,
    // avoiding per-frame Vec allocation + sort for every expanded directory.
    if is_expanded {
        // Clear cache if sort order changed.
        if state.cached_sort_order != Some(sort_order) {
            state.sorted_cache.clear();
            state.cached_sort_order = Some(sort_order);
        }
        // Compute and cache if not yet present.
        state
            .sorted_cache
            .entry(index)
            .or_insert_with(|| sorted_children(tree, index, stats, sort_order));
        // Clone the cached Vec to allow mutable `state` borrow in recursive call.
        // Vec<usize> clone is O(k), far cheaper than the O(k log k) sort it replaces.
        let children = state.sorted_cache[&index].clone();
        for child_idx in children {
            render_node(
                tree,
                child_idx,
                stats,
                state,
                selected,
                scan_complete,
                pending_delete,
                custom_commands,
                sort_order,
                notifications,
                theme_colors,
                ui,
                depth + 1,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rds_core::tree::{FileNode, NO_PARENT};

    fn make_file(size: u64) -> FileNode {
        FileNode {
            name_offset: 0,
            name_len: 0,
            size,
            first_child: u32::MAX,
            next_sibling: u32::MAX,
            modified: 0,
            parent: NO_PARENT,
            extension: 0,
            flags: 0,
        }
    }

    fn make_dir() -> FileNode {
        FileNode {
            name_offset: 0,
            name_len: 0,
            size: 0,
            first_child: u32::MAX,
            next_sibling: u32::MAX,
            modified: 0,
            parent: NO_PARENT,
            extension: 0,
            flags: 1,
        }
    }

    #[test]
    fn subtree_stats_single_file() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file(100), "a.txt");

        let stats = SubtreeStats::compute(&tree);
        assert_eq!(stats.size(0), 100);
        assert_eq!(stats.size(1), 100);
        assert_eq!(stats.file_count(0), 1);
        assert_eq!(stats.file_count(1), 1);
    }

    #[test]
    fn subtree_stats_nested_dirs() {
        let mut tree = DirTree::new("/root");
        let sub = tree.insert(0, make_dir(), "sub");
        tree.insert(sub, make_file(100), "a.txt");
        tree.insert(sub, make_file(200), "b.txt");
        tree.insert(0, make_file(50), "c.txt");

        let stats = SubtreeStats::compute(&tree);
        assert_eq!(stats.size(0), 350);
        assert_eq!(stats.size(sub), 300);
        assert_eq!(stats.size(4), 50);
        assert_eq!(stats.file_count(0), 3);
        assert_eq!(stats.file_count(sub), 2);
        assert_eq!(stats.file_count(4), 1);
    }

    #[test]
    fn subtree_stats_agrees_with_tree_method() {
        let mut tree = DirTree::new("/root");
        let sub = tree.insert(0, make_dir(), "sub");
        tree.insert(sub, make_file(100), "a.txt");
        tree.insert(sub, make_file(200), "b.txt");
        tree.insert(0, make_file(50), "c.txt");

        let stats = SubtreeStats::compute(&tree);
        for i in 0..tree.len() {
            assert_eq!(
                stats.size(i),
                tree.subtree_size(i),
                "size mismatch at index {i}"
            );
        }
    }

    #[test]
    fn subtree_stats_empty_dir() {
        let tree = DirTree::new("/empty");
        let stats = SubtreeStats::compute(&tree);
        assert_eq!(stats.size(0), 0);
        assert_eq!(stats.file_count(0), 0);
    }

    #[test]
    fn subtree_stats_out_of_bounds_returns_zero() {
        let tree = DirTree::new("/root");
        let stats = SubtreeStats::compute(&tree);
        assert_eq!(stats.size(999), 0);
        assert_eq!(stats.file_count(999), 0);
    }

    #[test]
    fn tree_view_state_expand_collapse() {
        let mut state = TreeViewState::new();
        assert!(!state.is_expanded(0));
        state.expand(0);
        assert!(state.is_expanded(0));
        state.toggle(0);
        assert!(!state.is_expanded(0));
        state.toggle(0);
        assert!(state.is_expanded(0));
    }

    #[test]
    fn tree_view_state_reset_clears_all() {
        let mut state = TreeViewState::new();
        state.expand(0);
        state.expand(1);
        state.expand(5);
        state.reset();
        assert!(!state.is_expanded(0));
        assert!(!state.is_expanded(1));
        assert!(!state.is_expanded(5));
    }

    #[test]
    fn sorted_children_by_size_descending() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file(10), "small.txt"); // index 1
        tree.insert(0, make_file(1000), "big.txt"); // index 2
        tree.insert(0, make_file(500), "medium.txt"); // index 3

        let stats = SubtreeStats::compute(&tree);
        let sorted = sorted_children(&tree, 0, &stats, SortOrder::SizeDesc);
        assert_eq!(sorted, vec![2, 3, 1]);
    }

    #[test]
    fn sorted_children_dirs_sorted_by_subtree_size() {
        let mut tree = DirTree::new("/root");
        let small_dir = tree.insert(0, make_dir(), "small_dir"); // index 1
        tree.insert(small_dir, make_file(10), "s.txt"); // index 2
        let big_dir = tree.insert(0, make_dir(), "big_dir"); // index 3
        tree.insert(big_dir, make_file(1000), "b.txt"); // index 4

        let stats = SubtreeStats::compute(&tree);
        let sorted = sorted_children(&tree, 0, &stats, SortOrder::SizeDesc);
        assert_eq!(sorted, vec![3, 1]);
    }

    #[test]
    fn sorted_children_empty_dir() {
        let tree = DirTree::new("/root");
        let stats = SubtreeStats::compute(&tree);
        let sorted = sorted_children(&tree, 0, &stats, SortOrder::SizeDesc);
        assert!(sorted.is_empty());
    }

    #[test]
    fn expand_ancestors_deep_node() {
        let mut tree = DirTree::new("/root");
        let d1 = tree.insert(0, make_dir(), "d1");
        let d2 = tree.insert(d1, make_dir(), "d2");
        let d3 = tree.insert(d2, make_dir(), "d3");
        let file = tree.insert(d3, make_file(100), "deep.txt");

        let mut state = TreeViewState::new();
        expand_ancestors(&tree, &mut state, file);

        assert!(state.is_expanded(0)); // root
        assert!(state.is_expanded(d1)); // d1
        assert!(state.is_expanded(d2)); // d2
        assert!(state.is_expanded(d3)); // d3
    }

    #[test]
    fn expand_ancestors_root_is_noop() {
        let tree = DirTree::new("/root");
        let mut state = TreeViewState::new();
        expand_ancestors(&tree, &mut state, 0);
        // Root has no parent — nothing to expand.
        assert!(!state.is_expanded(0));
    }

    #[test]
    fn expand_ancestors_direct_child_of_root() {
        let mut tree = DirTree::new("/root");
        let file = tree.insert(0, make_file(100), "a.txt");

        let mut state = TreeViewState::new();
        expand_ancestors(&tree, &mut state, file);

        // Only root should be expanded (parent of the file).
        assert!(state.is_expanded(0));
    }

    #[test]
    fn expand_ancestors_idempotent() {
        let mut tree = DirTree::new("/root");
        let d1 = tree.insert(0, make_dir(), "d1");
        let file = tree.insert(d1, make_file(100), "a.txt");

        let mut state = TreeViewState::new();
        expand_ancestors(&tree, &mut state, file);
        expand_ancestors(&tree, &mut state, file);

        assert!(state.is_expanded(0));
        assert!(state.is_expanded(d1));
    }

    #[test]
    fn subtree_stats_excludes_deleted_file() {
        let mut tree = DirTree::new("/root");
        let sub = tree.insert(0, make_dir(), "sub");
        let a = tree.insert(sub, make_file(100), "a.txt");
        tree.insert(sub, make_file(200), "b.txt");
        tree.insert(0, make_file(50), "c.txt");

        // Before tombstone: root = 350 bytes, 3 files; sub = 300 bytes, 2 files.
        let stats = SubtreeStats::compute(&tree);
        assert_eq!(stats.size(0), 350);
        assert_eq!(stats.file_count(0), 3);
        assert_eq!(stats.size(sub), 300);
        assert_eq!(stats.file_count(sub), 2);

        // Tombstone a.txt (100 bytes).
        tree.tombstone(a);

        let stats = SubtreeStats::compute(&tree);
        assert_eq!(stats.size(0), 250, "root size should exclude deleted file");
        assert_eq!(
            stats.file_count(0),
            2,
            "root file count should exclude deleted file"
        );
        assert_eq!(stats.size(sub), 200, "sub size should exclude deleted file");
        assert_eq!(
            stats.file_count(sub),
            1,
            "sub file count should exclude deleted file"
        );
    }

    #[test]
    fn subtree_stats_excludes_deleted_directory() {
        let mut tree = DirTree::new("/root");
        let sub = tree.insert(0, make_dir(), "sub");
        tree.insert(sub, make_file(100), "a.txt");
        tree.insert(sub, make_file(200), "b.txt");
        tree.insert(0, make_file(50), "c.txt");

        // Tombstone entire subdirectory (sub + a.txt + b.txt).
        tree.tombstone(sub);

        let stats = SubtreeStats::compute(&tree);
        assert_eq!(stats.size(0), 50, "root size should only include c.txt");
        assert_eq!(stats.file_count(0), 1, "root should count only c.txt");
        assert_eq!(stats.size(sub), 0, "deleted sub should have zero size");
        assert_eq!(
            stats.file_count(sub),
            0,
            "deleted sub should have zero file count"
        );
    }

    #[test]
    fn sorted_children_excludes_deleted() {
        let mut tree = DirTree::new("/root");
        let small = tree.insert(0, make_file(10), "small.txt");
        tree.insert(0, make_file(1000), "big.txt");
        tree.insert(0, make_file(500), "medium.txt");

        // Tombstone the small file.
        tree.tombstone(small);

        let stats = SubtreeStats::compute(&tree);
        let sorted = sorted_children(&tree, 0, &stats, SortOrder::SizeDesc);
        // small.txt (index 1) should be excluded.
        assert_eq!(sorted, vec![2, 3]);
    }

    #[test]
    fn stats_recompute_after_tombstone_directory_reflects_decreased_totals() {
        let mut tree = DirTree::new("/root");
        let dir_a = tree.insert(0, make_dir(), "dir_a");
        tree.insert(dir_a, make_file(100), "a1.txt");
        tree.insert(dir_a, make_file(200), "a2.txt");
        let dir_b = tree.insert(0, make_dir(), "dir_b");
        tree.insert(dir_b, make_file(250), "b1.txt");
        tree.insert(dir_b, make_file(250), "b2.txt");
        tree.insert(dir_b, make_file(250), "b3.txt");
        tree.insert(0, make_file(50), "top.txt");

        let stats_before = SubtreeStats::compute(&tree);
        assert_eq!(stats_before.size(0), 1100);
        assert_eq!(stats_before.file_count(0), 6);
        assert_eq!(stats_before.size(dir_a), 300);
        assert_eq!(stats_before.file_count(dir_a), 2);
        assert_eq!(stats_before.size(dir_b), 750);
        assert_eq!(stats_before.file_count(dir_b), 3);

        tree.tombstone(dir_a);

        let stats_after = SubtreeStats::compute(&tree);
        assert_eq!(stats_after.size(0), 800);
        assert_eq!(stats_after.file_count(0), 4);
        assert_eq!(stats_after.size(dir_b), 750);
        assert_eq!(stats_after.file_count(dir_b), 3);
        assert_eq!(stats_after.size(dir_a), 0);
        assert_eq!(stats_after.file_count(dir_a), 0);
    }

    #[test]
    fn stats_tombstone_all_children_yields_zero_size_and_count() {
        let mut tree = DirTree::new("/root");
        let parent_dir = tree.insert(0, make_dir(), "parent_dir");
        let child1 = tree.insert(parent_dir, make_file(400), "child1.txt");
        let child2 = tree.insert(parent_dir, make_file(600), "child2.txt");
        let sub = tree.insert(parent_dir, make_dir(), "sub");
        tree.insert(sub, make_file(1000), "deep.txt");

        let stats_before = SubtreeStats::compute(&tree);
        assert_eq!(stats_before.size(parent_dir), 2000);
        assert_eq!(stats_before.file_count(parent_dir), 3);

        tree.tombstone(child1);
        tree.tombstone(child2);
        tree.tombstone(sub);

        let stats_after = SubtreeStats::compute(&tree);
        assert_eq!(stats_after.size(parent_dir), 0);
        assert_eq!(stats_after.file_count(parent_dir), 0);
        assert!(!tree.get(parent_dir).unwrap().deleted());
        assert_eq!(stats_after.size(0), 0);
        assert_eq!(stats_after.file_count(0), 0);
    }
}
