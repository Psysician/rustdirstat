//! Directory tree panel with expand/collapse navigation.
//!
//! Renders the `DirTree` as an indented tree sorted by subtree size
//! descending. `SubtreeStats` caches sizes and file counts via a
//! single O(n) bottom-up pass so rendering never re-traverses the
//! arena. (ref: DL-001)

use std::collections::HashSet;

use rds_core::tree::DirTree;

/// Cached subtree sizes and file counts. Computed in a single
/// bottom-up pass over the arena — O(n) total, O(1) per lookup.
pub(crate) struct SubtreeStats {
    sizes: Vec<u64>,
    file_counts: Vec<u64>,
}

impl SubtreeStats {
    pub fn compute(tree: &DirTree) -> Self {
        let len = tree.len();
        let mut sizes = vec![0u64; len];
        let mut file_counts = vec![0u64; len];

        // Initialize with each node's own values.
        for i in 0..len {
            if let Some(node) = tree.get(i) {
                sizes[i] = node.size;
                if !node.is_dir {
                    file_counts[i] = 1;
                }
            }
        }

        // Bottom-up accumulation. Arena order is parent-before-child
        // (depth-first insertion), so reverse iteration visits children
        // before parents — each child's accumulated total is final when
        // added to its parent.
        for i in (1..len).rev() {
            if let Some(node) = tree.get(i)
                && let Some(parent) = node.parent
            {
                sizes[parent] += sizes[i];
                file_counts[parent] += file_counts[i];
            }
        }

        Self { sizes, file_counts }
    }

    pub fn size(&self, index: usize) -> u64 {
        self.sizes.get(index).copied().unwrap_or(0)
    }

    pub fn file_count(&self, index: usize) -> u64 {
        self.file_counts.get(index).copied().unwrap_or(0)
    }
}

/// Expand/collapse state for the directory tree panel.
pub(crate) struct TreeViewState {
    expanded: HashSet<usize>,
}

impl TreeViewState {
    pub fn new() -> Self {
        Self {
            expanded: HashSet::new(),
        }
    }

    pub fn reset(&mut self) {
        self.expanded.clear();
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

/// Returns child indices of `index` sorted by subtree size descending.
pub(crate) fn sorted_children(tree: &DirTree, index: usize, stats: &SubtreeStats) -> Vec<usize> {
    let mut children: Vec<usize> = tree.children(index).to_vec();
    children.sort_by(|&a, &b| stats.size(b).cmp(&stats.size(a)));
    children
}

#[cfg(test)]
mod tests {
    use super::*;
    use rds_core::tree::FileNode;

    fn make_file(name: &str, size: u64) -> FileNode {
        FileNode {
            name: name.to_string(),
            size,
            is_dir: false,
            children: Vec::new(),
            parent: None,
            extension: None,
            modified: None,
        }
    }

    fn make_dir(name: &str) -> FileNode {
        FileNode {
            name: name.to_string(),
            size: 0,
            is_dir: true,
            children: Vec::new(),
            parent: None,
            extension: None,
            modified: None,
        }
    }

    #[test]
    fn subtree_stats_single_file() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file("a.txt", 100));

        let stats = SubtreeStats::compute(&tree);
        assert_eq!(stats.size(0), 100);
        assert_eq!(stats.size(1), 100);
        assert_eq!(stats.file_count(0), 1);
        assert_eq!(stats.file_count(1), 1);
    }

    #[test]
    fn subtree_stats_nested_dirs() {
        let mut tree = DirTree::new("/root");
        let sub = tree.insert(0, make_dir("sub"));
        tree.insert(sub, make_file("a.txt", 100));
        tree.insert(sub, make_file("b.txt", 200));
        tree.insert(0, make_file("c.txt", 50));

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
        let sub = tree.insert(0, make_dir("sub"));
        tree.insert(sub, make_file("a.txt", 100));
        tree.insert(sub, make_file("b.txt", 200));
        tree.insert(0, make_file("c.txt", 50));

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
        tree.insert(0, make_file("small.txt", 10));   // index 1
        tree.insert(0, make_file("big.txt", 1000));    // index 2
        tree.insert(0, make_file("medium.txt", 500));  // index 3

        let stats = SubtreeStats::compute(&tree);
        let sorted = sorted_children(&tree, 0, &stats);
        assert_eq!(sorted, vec![2, 3, 1]);
    }

    #[test]
    fn sorted_children_dirs_sorted_by_subtree_size() {
        let mut tree = DirTree::new("/root");
        let small_dir = tree.insert(0, make_dir("small_dir"));  // index 1
        tree.insert(small_dir, make_file("s.txt", 10));          // index 2
        let big_dir = tree.insert(0, make_dir("big_dir"));       // index 3
        tree.insert(big_dir, make_file("b.txt", 1000));          // index 4

        let stats = SubtreeStats::compute(&tree);
        let sorted = sorted_children(&tree, 0, &stats);
        assert_eq!(sorted, vec![3, 1]);
    }

    #[test]
    fn sorted_children_empty_dir() {
        let tree = DirTree::new("/root");
        let stats = SubtreeStats::compute(&tree);
        let sorted = sorted_children(&tree, 0, &stats);
        assert!(sorted.is_empty());
    }
}
