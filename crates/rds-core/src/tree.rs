//! Arena-allocated directory tree.
//!
//! `DirTree` stores all nodes in a flat `Vec<FileNode>`. Parent and child
//! relationships are expressed as `usize` indices into that vec. This avoids
//! reference-counting overhead and keeps traversal cache-local.
//!
//! The arena is insert-only: nodes are appended and never removed. This keeps
//! all previously returned indices valid for the lifetime of the tree, which
//! enables safe index-based linking in `ScanEvent::NodeDiscovered` and across
//! the channel boundary to the GUI thread.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A single filesystem entry (file or directory) stored inside a `DirTree`.
///
/// Fields are `pub` so downstream crates (`rds-gui`) can read them directly
/// without accessor boilerplate.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileNode {
    /// Entry name. For the root node this is the full absolute path; for all
    /// other nodes it is the filename only. Use `DirTree::path` for full path
    /// reconstruction.
    pub name: String,
    /// Disk size in bytes. For directories this is 0; use `DirTree::subtree_size`.
    pub size: u64,
    /// `true` if the entry is a directory.
    pub is_dir: bool,
    /// Indices of child nodes in the owning `DirTree`'s arena.
    pub children: Vec<usize>,
    /// Index of the parent node, or `None` for the root.
    pub parent: Option<usize>,
    /// Lowercased file extension without leading dot, or `None` for entries with no extension.
    pub extension: Option<String>,
    /// Last-modified time as a Unix timestamp (seconds), if available.
    pub modified: Option<u64>,
    /// Tombstone flag. When `true`, this node has been logically deleted.
    /// The node remains in the arena to preserve index stability.
    #[serde(default)]
    pub deleted: bool,
}

/// Arena-allocated file tree.
///
/// All nodes live in a single `Vec`. Parent/child links are `usize` indices
/// into that vec. The arena is insert-only: nodes are never removed, so all
/// previously returned indices remain valid. The root node is always at
/// index `0` and is created by `DirTree::new`. The arena is never empty.
#[derive(Debug)]
pub struct DirTree {
    nodes: Vec<FileNode>,
}

impl DirTree {
    /// Creates a new tree with a single root directory node named `root_name`.
    ///
    /// The root is at index `0` with `parent = None`.
    pub fn new(root_name: &str) -> Self {
        let root = FileNode {
            name: root_name.to_string(),
            size: 0,
            is_dir: true,
            children: Vec::new(),
            parent: None,
            extension: None,
            modified: None,
            deleted: false,
        };
        DirTree { nodes: vec![root] }
    }

    /// Creates a new tree using the given `FileNode` as the root.
    ///
    /// Clears `parent` and `children` to enforce root invariants.
    /// Preserves all other fields (`size`, `modified`, `extension`, etc.).
    pub fn from_root(mut node: FileNode) -> Self {
        node.parent = None;
        node.children = Vec::new();
        node.deleted = false;
        DirTree { nodes: vec![node] }
    }

    /// Appends `node` as a child of `parent_index` and returns the new node's index.
    ///
    /// Indices are stable: nodes are append-only. Delete operations MUST tombstone
    /// (zero size) rather than remove from Vec; removing any node invalidates all
    /// existing indices across all crates.
    pub fn insert(&mut self, parent_index: usize, mut node: FileNode) -> usize {
        assert!(
            parent_index < self.nodes.len(),
            "parent_index out of bounds"
        );
        let new_index = self.nodes.len();
        node.parent = Some(parent_index);
        self.nodes.push(node);
        self.nodes[parent_index].children.push(new_index);
        new_index
    }

    /// Sums `size` across all nodes in the subtree rooted at `index`.
    ///
    /// Uses an iterative stack traversal to avoid recursion limits on deep trees.
    /// Panics if `index` is out of bounds.
    pub fn subtree_size(&self, index: usize) -> u64 {
        assert!(index < self.nodes.len(), "index out of bounds");
        let mut total: u64 = 0;
        let mut stack = vec![index];
        while let Some(i) = stack.pop() {
            let node = &self.nodes[i];
            total += node.size;
            for &child_idx in &node.children {
                stack.push(child_idx);
            }
        }
        total
    }

    /// Reconstructs the full path from the root to the node at `index`.
    ///
    /// Walks the parent chain from `index` up to the root (where `parent` is
    /// `None`), reverses the collected names, and joins them into a `PathBuf`.
    /// Panics if `index` is out of bounds.
    pub fn path(&self, index: usize) -> PathBuf {
        assert!(index < self.nodes.len(), "index out of bounds");
        let mut components = Vec::new();
        let mut current = index;
        loop {
            components.push(self.nodes[current].name.as_str());
            match self.nodes[current].parent {
                Some(parent_idx) => current = parent_idx,
                None => break,
            }
        }
        components.reverse();
        let mut path = PathBuf::new();
        for component in components {
            path.push(component);
        }
        path
    }

    pub fn get(&self, index: usize) -> Option<&FileNode> {
        self.nodes.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut FileNode> {
        self.nodes.get_mut(index)
    }

    pub fn children(&self, index: usize) -> &[usize] {
        assert!(index < self.nodes.len(), "index out of bounds");
        &self.nodes[index].children
    }

    pub fn root(&self) -> usize {
        0
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Always returns `false` — `DirTree::new` inserts a root node, so the
    /// arena is never empty. Exists to satisfy Rust's `len`/`is_empty` convention.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Logically deletes the node at `index` and all its descendants.
    ///
    /// For each visited node: sets `deleted = true` and `size = 0`.
    /// If the tombstoned node has a parent, removes it from the parent's
    /// `children` vec. Nodes remain in the arena to preserve index stability.
    ///
    /// Uses an iterative stack traversal matching the pattern in `subtree_size`.
    /// Panics if `index` is out of bounds.
    pub fn tombstone(&mut self, index: usize) {
        assert!(index < self.nodes.len(), "index out of bounds");

        // Collect all descendant indices via stack-based DFS.
        // We read children before mutating, so collect first.
        let mut to_mark = Vec::new();
        let mut stack = vec![index];
        while let Some(i) = stack.pop() {
            to_mark.push(i);
            for &child_idx in &self.nodes[i].children {
                stack.push(child_idx);
            }
        }

        // Mark all collected nodes as deleted with zero size.
        for &i in &to_mark {
            self.nodes[i].deleted = true;
            self.nodes[i].size = 0;
        }

        // Remove from parent's children list (if not root).
        if let Some(parent_idx) = self.nodes[index].parent {
            self.nodes[parent_idx].children.retain(|&c| c != index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn root_node_properties() {
        let tree = DirTree::new("/root");
        assert_eq!(tree.root(), 0);
        assert_eq!(tree.len(), 1);
        let root = tree.get(0).unwrap();
        assert_eq!(root.name, "/root");
        assert!(root.is_dir);
        assert_eq!(root.parent, None);
        assert!(root.children.is_empty());
        assert_eq!(root.size, 0);
    }

    #[test]
    fn from_root_preserves_all_fields() {
        let node = FileNode {
            name: "/tmp/scan".to_string(),
            size: 4096,
            is_dir: true,
            children: vec![99], // should be cleared
            parent: Some(42),   // should be cleared
            extension: None,
            modified: Some(1_700_000_000),
            deleted: false,
        };
        let tree = DirTree::from_root(node);
        assert_eq!(tree.len(), 1);
        let root = tree.get(0).unwrap();
        assert_eq!(root.name, "/tmp/scan");
        assert_eq!(root.size, 4096);
        assert!(root.is_dir);
        assert_eq!(root.modified, Some(1_700_000_000));
        assert_eq!(root.parent, None);
        assert!(root.children.is_empty());
    }

    #[test]
    fn insert_and_parent_child_linking() {
        let mut tree = DirTree::new("/root");
        let file_a = make_file_node("a.txt", 100);
        let idx_a = tree.insert(0, file_a);
        assert_eq!(idx_a, 1);

        let root = tree.get(0).unwrap();
        assert_eq!(root.children, vec![1]);

        let node_a = tree.get(idx_a).unwrap();
        assert_eq!(node_a.parent, Some(0));
        assert_eq!(node_a.name, "a.txt");
        assert_eq!(node_a.size, 100);

        let file_b = make_file_node("b.txt", 200);
        let idx_b = tree.insert(0, file_b);
        assert_eq!(idx_b, 2);

        let root = tree.get(0).unwrap();
        assert_eq!(root.children, vec![1, 2]);
        assert_eq!(tree.len(), 3);
    }

    #[test]
    fn subtree_size_nested() {
        let mut tree = DirTree::new("/root");
        let subdir = make_dir_node("subdir");
        let idx_sub = tree.insert(0, subdir);

        let file_a = make_file_node("a.txt", 100);
        tree.insert(idx_sub, file_a);

        let file_b = make_file_node("b.txt", 250);
        tree.insert(idx_sub, file_b);

        let file_c = make_file_node("c.txt", 50);
        tree.insert(0, file_c);

        assert_eq!(tree.subtree_size(idx_sub), 350);
        assert_eq!(tree.subtree_size(0), 400);
    }

    #[test]
    fn path_reconstruction_three_levels() {
        let mut tree = DirTree::new("/root");
        let dir_a = make_dir_node("dir_a");
        let idx_a = tree.insert(0, dir_a);

        let dir_b = make_dir_node("dir_b");
        let idx_b = tree.insert(idx_a, dir_b);

        let file = make_file_node("file.txt", 42);
        let idx_file = tree.insert(idx_b, file);

        let path = tree.path(idx_file);
        assert_eq!(path, PathBuf::from("/root/dir_a/dir_b/file.txt"));

        let path_b = tree.path(idx_b);
        assert_eq!(path_b, PathBuf::from("/root/dir_a/dir_b"));

        let path_root = tree.path(0);
        assert_eq!(path_root, PathBuf::from("/root"));
    }

    #[test]
    fn leaf_nodes_have_empty_children() {
        let mut tree = DirTree::new("/root");
        let file = make_file_node("leaf.txt", 10);
        let idx = tree.insert(0, file);
        assert!(tree.children(idx).is_empty());
    }

    #[test]
    fn tombstone_leaf_file() {
        let mut tree = DirTree::new("/root");
        let file = make_file_node("a.txt", 100);
        let idx = tree.insert(0, file);

        tree.tombstone(idx);

        let node = tree.get(idx).unwrap();
        assert!(node.deleted);
        assert_eq!(node.size, 0);
        // Parent's children no longer contains the tombstoned index.
        let root = tree.get(0).unwrap();
        assert!(!root.children.contains(&idx));
    }

    #[test]
    fn tombstone_directory_with_descendants() {
        let mut tree = DirTree::new("/root");
        let subdir = make_dir_node("subdir");
        let idx_sub = tree.insert(0, subdir);

        let file_a = make_file_node("a.txt", 100);
        let idx_a = tree.insert(idx_sub, file_a);

        let nested_dir = make_dir_node("nested");
        let idx_nested = tree.insert(idx_sub, nested_dir);

        let file_b = make_file_node("b.txt", 200);
        let idx_b = tree.insert(idx_nested, file_b);

        tree.tombstone(idx_sub);

        // All descendants are marked deleted with size 0.
        for &i in &[idx_sub, idx_a, idx_nested, idx_b] {
            let node = tree.get(i).unwrap();
            assert!(node.deleted, "node at index {i} should be deleted");
            assert_eq!(node.size, 0, "node at index {i} should have size 0");
        }
        // Directory removed from parent's children.
        let root = tree.get(0).unwrap();
        assert!(!root.children.contains(&idx_sub));
    }

    #[test]
    fn tombstone_root() {
        let mut tree = DirTree::new("/root");
        let file = make_file_node("a.txt", 100);
        let idx_a = tree.insert(0, file);

        let subdir = make_dir_node("subdir");
        let idx_sub = tree.insert(0, subdir);

        let file_b = make_file_node("b.txt", 200);
        let idx_b = tree.insert(idx_sub, file_b);

        tree.tombstone(0);

        // Root and all descendants are deleted.
        for &i in &[0, idx_a, idx_sub, idx_b] {
            let node = tree.get(i).unwrap();
            assert!(node.deleted, "node at index {i} should be deleted");
            assert_eq!(node.size, 0, "node at index {i} should have size 0");
        }
        // Root has no parent, so no parent-removal step needed (no panic).
    }

    #[test]
    fn tombstone_preserves_arena_length() {
        let mut tree = DirTree::new("/root");
        let file_a = make_file_node("a.txt", 100);
        tree.insert(0, file_a);
        let file_b = make_file_node("b.txt", 200);
        tree.insert(0, file_b);

        let len_before = tree.len();
        tree.tombstone(1);
        assert_eq!(
            tree.len(),
            len_before,
            "arena length must not change after tombstone"
        );
    }

    #[test]
    fn tombstone_path_still_works() {
        let mut tree = DirTree::new("/root");
        let dir_a = make_dir_node("dir_a");
        let idx_a = tree.insert(0, dir_a);

        let file = make_file_node("file.txt", 42);
        let idx_file = tree.insert(idx_a, file);

        tree.tombstone(idx_a);

        // Parent pointer is preserved, so path reconstruction still works.
        let path = tree.path(idx_file);
        assert_eq!(path, PathBuf::from("/root/dir_a/file.txt"));
    }
}
