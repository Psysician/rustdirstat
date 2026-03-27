//! Arena-allocated directory tree.
//!
//! `DirTree` stores all nodes in a flat `Vec<FileNode>`. Parent and child
//! relationships are expressed as indices into that vec. This avoids
//! reference-counting overhead and keeps traversal cache-local.
//!
//! The arena is insert-only: nodes are appended and never removed. This keeps
//! all previously returned indices valid for the lifetime of the tree, which
//! enables safe index-based linking in `ScanEvent::NodeDiscovered` and across
//! the channel boundary to the GUI thread.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Sentinel value for `FileNode::parent` indicating no parent (root node).
pub const NO_PARENT: u32 = u32::MAX;

/// A single filesystem entry (file or directory) stored inside a `DirTree`.
///
/// Compact representation (~48 bytes). Flags encode `is_dir` (bit 0) and
/// `deleted` (bit 1). Extensions are interned in the owning `DirTree`'s
/// extension table; `extension` stores the 1-based index (0 = none).
/// `parent` uses `NO_PARENT` sentinel instead of `Option`. `modified` uses
/// 0 for unknown instead of `Option`. Children form an intrusive singly-linked
/// list via `first_child` / `next_sibling` (both `u32::MAX` = none).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileNode {
    /// Entry name. For the root node this is the full absolute path; for all
    /// other nodes it is the filename only. Use `DirTree::path` for full path
    /// reconstruction.
    pub name: Box<str>,
    /// Disk size in bytes. For directories this is 0; use `DirTree::subtree_size`.
    pub size: u64,
    /// Last-modified time as a Unix timestamp (seconds). 0 = unknown.
    pub modified: u64,
    /// Index of the parent node in the arena, or `NO_PARENT` for the root.
    pub parent: u32,
    /// Head of the intrusive child linked list, or `u32::MAX` for no children.
    pub first_child: u32,
    /// Next sibling in the parent's child list, or `u32::MAX` for end of list.
    pub next_sibling: u32,
    /// 1-based index into `DirTree::extensions`. 0 = no extension.
    pub extension: u16,
    /// Bit flags: bit 0 = is_dir, bit 1 = deleted.
    pub flags: u8,
}

impl FileNode {
    /// Returns `true` if this entry is a directory.
    #[inline]
    pub fn is_dir(&self) -> bool {
        self.flags & 1 != 0
    }

    /// Returns `true` if this node has been logically deleted (tombstoned).
    #[inline]
    pub fn deleted(&self) -> bool {
        self.flags & 2 != 0
    }

    /// Sets the deleted (tombstone) flag.
    #[inline]
    pub fn set_deleted(&mut self) {
        self.flags |= 2;
    }
}

/// Arena-allocated file tree with interned extension strings.
///
/// All nodes live in a single `Vec`. Parent/child links are indices
/// into that vec. The arena is insert-only: nodes are never removed, so all
/// previously returned indices remain valid. The root node is always at
/// index `0` and is created by `DirTree::new`. The arena is never empty.
#[derive(Debug)]
pub struct DirTree {
    nodes: Vec<FileNode>,
    extensions: Vec<Box<str>>,
}

impl DirTree {
    /// Creates a new tree with a single root directory node named `root_name`.
    ///
    /// The root is at index `0` with `parent = NO_PARENT`.
    pub fn new(root_name: &str) -> Self {
        let root = FileNode {
            name: root_name.into(),
            size: 0,
            modified: 0,
            parent: NO_PARENT,
            first_child: u32::MAX,
            next_sibling: u32::MAX,
            extension: 0,
            flags: 1, // is_dir
        };
        DirTree {
            nodes: vec![root],
            extensions: Vec::new(),
        }
    }

    /// Creates a new tree with pre-allocated arena capacity.
    ///
    /// Behaves like [`new`](Self::new) but calls `Vec::with_capacity` to
    /// avoid repeated reallocations when the expected node count is known.
    /// `capacity` is clamped to a minimum of 1 (for the root node).
    pub fn new_with_capacity(root_name: &str, capacity: usize) -> Self {
        let root = FileNode {
            name: root_name.into(),
            size: 0,
            modified: 0,
            parent: NO_PARENT,
            first_child: u32::MAX,
            next_sibling: u32::MAX,
            extension: 0,
            flags: 1, // is_dir
        };
        let mut nodes = Vec::with_capacity(capacity.max(1));
        nodes.push(root);
        DirTree {
            nodes,
            extensions: Vec::new(),
        }
    }

    /// Creates a new tree using the given `FileNode` as the root.
    ///
    /// Clears `parent`, `first_child`, and `next_sibling` to enforce root
    /// invariants. Clears the deleted flag. Preserves all other fields.
    pub fn from_root(mut node: FileNode) -> Self {
        node.parent = NO_PARENT;
        node.first_child = u32::MAX;
        node.next_sibling = u32::MAX;
        node.flags &= !2; // clear deleted flag
        DirTree {
            nodes: vec![node],
            extensions: Vec::new(),
        }
    }

    /// Creates a new tree using the given `FileNode` as the root, with
    /// pre-allocated arena capacity.
    ///
    /// Behaves like [`from_root`](Self::from_root) but calls
    /// `Vec::with_capacity` to avoid repeated reallocations.
    /// `capacity` is clamped to a minimum of 1 (for the root node).
    pub fn from_root_with_capacity(mut node: FileNode, capacity: usize) -> Self {
        node.parent = NO_PARENT;
        node.first_child = u32::MAX;
        node.next_sibling = u32::MAX;
        node.flags &= !2; // clear deleted flag
        let mut nodes = Vec::with_capacity(capacity.max(1));
        nodes.push(node);
        DirTree {
            nodes,
            extensions: Vec::new(),
        }
    }

    /// Interns an extension string. Returns 0 for None.
    pub fn intern_extension(&mut self, ext: Option<&str>) -> u16 {
        match ext {
            None => 0,
            Some(s) => {
                if let Some(pos) = self.extensions.iter().position(|e| &**e == s) {
                    (pos as u16) + 1
                } else {
                    self.extensions.push(s.into());
                    self.extensions.len() as u16
                }
            }
        }
    }

    /// Looks up an interned extension. Returns None for index 0.
    pub fn extension_str(&self, idx: u16) -> Option<&str> {
        if idx == 0 {
            None
        } else {
            self.extensions.get((idx - 1) as usize).map(|s| &**s)
        }
    }

    /// Appends `node` as a child of `parent_index` and returns the new node's index.
    ///
    /// Indices are stable: nodes are append-only. Delete operations MUST tombstone
    /// (zero size) rather than remove from Vec; removing any node invalidates all
    /// existing indices across all crates.
    pub fn insert(&mut self, parent_index: usize, mut node: FileNode) -> usize {
        // Invariant: parent_index must refer to a valid arena node. Indices originate
        // from the scanner via crossbeam channel; a violation is a scanner bug, not a
        // user error. Intentional panic — do not replace with Result.
        assert!(
            parent_index < self.nodes.len(),
            "parent_index out of bounds"
        );
        let new_index = self.nodes.len();
        node.parent = parent_index as u32;
        // Prepend to parent's child linked list.
        node.next_sibling = self.nodes[parent_index].first_child;
        self.nodes[parent_index].first_child = new_index as u32;
        self.nodes.push(node);
        new_index
    }

    /// Sums `size` across all nodes in the subtree rooted at `index`.
    ///
    /// Uses an iterative stack traversal to avoid recursion limits on deep trees.
    /// Panics if `index` is out of bounds.
    pub fn subtree_size(&self, index: usize) -> u64 {
        // Invariant: index must be a valid arena position. All call sites pass indices
        // obtained from prior insert() calls; an out-of-bounds value is a logic bug in
        // the caller. Intentional panic — do not replace with Result.
        assert!(index < self.nodes.len(), "index out of bounds");
        let mut total: u64 = 0;
        let mut stack = vec![index];
        while let Some(i) = stack.pop() {
            let node = &self.nodes[i];
            total += node.size;
            let mut child = node.first_child;
            while child != u32::MAX {
                stack.push(child as usize);
                child = self.nodes[child as usize].next_sibling;
            }
        }
        total
    }

    /// Reconstructs the full path from the root to the node at `index`.
    ///
    /// Walks the parent chain from `index` up to the root (where `parent` is
    /// `NO_PARENT`), reverses the collected names, and joins them into a `PathBuf`.
    /// Panics if `index` is out of bounds.
    pub fn path(&self, index: usize) -> PathBuf {
        // Invariant: index must be a valid arena position. Called with indices from
        // insert() or channel events; a violation is a logic bug. Do not replace with Result.
        assert!(index < self.nodes.len(), "index out of bounds");
        let mut components = Vec::new();
        let mut current = index;
        loop {
            components.push(&*self.nodes[current].name);
            let parent = self.nodes[current].parent;
            if parent == NO_PARENT {
                break;
            } else {
                current = parent as usize;
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

    pub fn children(&self, index: usize) -> ChildIter<'_> {
        // Invariant: index must be a valid arena position. GUI panels pass indices
        // received from the scanner; a violation is a protocol bug. Do not replace with Result.
        assert!(index < self.nodes.len(), "index out of bounds");
        ChildIter {
            nodes: &self.nodes,
            current: self.nodes[index].first_child,
        }
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
    /// For each visited node: sets `deleted` flag and `size = 0`.
    /// If the tombstoned node has a parent, unlinks it from the parent's
    /// child linked list. Nodes remain in the arena to preserve index stability.
    ///
    /// Uses an iterative stack traversal matching the pattern in `subtree_size`.
    /// Panics if `index` is out of bounds.
    pub fn tombstone(&mut self, index: usize) {
        // Invariant: index must be a valid arena position. Called by delete actions
        // on user-selected nodes; a violation is a logic bug. Do not replace with Result.
        assert!(index < self.nodes.len(), "index out of bounds");

        // Collect all descendant indices via stack-based DFS.
        let mut to_mark = Vec::new();
        let mut stack = vec![index];
        while let Some(i) = stack.pop() {
            to_mark.push(i);
            let mut child = self.nodes[i].first_child;
            while child != u32::MAX {
                stack.push(child as usize);
                child = self.nodes[child as usize].next_sibling;
            }
        }

        // Mark all collected nodes as deleted with zero size.
        for &i in &to_mark {
            self.nodes[i].set_deleted();
            self.nodes[i].size = 0;
        }

        // Unlink from parent's child linked list (if not root).
        let parent = self.nodes[index].parent;
        if parent != NO_PARENT {
            let parent_idx = parent as usize;
            let target = index as u32;
            if self.nodes[parent_idx].first_child == target {
                // Head of list — advance to next sibling.
                self.nodes[parent_idx].first_child = self.nodes[index].next_sibling;
            } else {
                // Find previous sibling and unlink.
                let mut prev = self.nodes[parent_idx].first_child;
                while prev != u32::MAX {
                    let prev_usize = prev as usize;
                    if self.nodes[prev_usize].next_sibling == target {
                        self.nodes[prev_usize].next_sibling = self.nodes[index].next_sibling;
                        break;
                    }
                    prev = self.nodes[prev_usize].next_sibling;
                }
            }
        }
    }

    /// Releases excess capacity in the arena vec and extension table.
    /// Call after scanning completes to reduce steady-state memory.
    pub fn shrink_to_fit(&mut self) {
        self.nodes.shrink_to_fit();
        self.extensions.shrink_to_fit();
    }
}

/// Iterator over the child indices of a node in a `DirTree`.
///
/// Walks the intrusive `first_child` / `next_sibling` linked list.
pub struct ChildIter<'a> {
    nodes: &'a [FileNode],
    current: u32,
}

impl<'a> Iterator for ChildIter<'a> {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        if self.current == u32::MAX {
            None
        } else {
            let idx = self.current;
            self.current = self.nodes[idx as usize].next_sibling;
            Some(idx)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_file_node(name: &str, size: u64) -> FileNode {
        FileNode {
            name: name.into(),
            size,
            modified: 0,
            parent: NO_PARENT,
            first_child: u32::MAX,
            next_sibling: u32::MAX,
            extension: 0,
            flags: 0,
        }
    }

    fn make_dir_node(name: &str) -> FileNode {
        FileNode {
            name: name.into(),
            size: 0,
            modified: 0,
            parent: NO_PARENT,
            first_child: u32::MAX,
            next_sibling: u32::MAX,
            extension: 0,
            flags: 1, // is_dir
        }
    }

    #[test]
    fn root_node_properties() {
        let tree = DirTree::new("/root");
        assert_eq!(tree.root(), 0);
        assert_eq!(tree.len(), 1);
        let root = tree.get(0).unwrap();
        assert_eq!(&*root.name, "/root");
        assert!(root.is_dir());
        assert_eq!(root.parent, NO_PARENT);
        assert_eq!(root.first_child, u32::MAX);
        assert_eq!(root.size, 0);
    }

    #[test]
    fn from_root_preserves_all_fields() {
        let node = FileNode {
            name: "/tmp/scan".into(),
            size: 4096,
            modified: 1_700_000_000,
            parent: 42,       // should be cleared
            first_child: 99,  // should be cleared
            next_sibling: 99, // should be cleared
            extension: 0,
            flags: 1, // is_dir
        };
        let tree = DirTree::from_root(node);
        assert_eq!(tree.len(), 1);
        let root = tree.get(0).unwrap();
        assert_eq!(&*root.name, "/tmp/scan");
        assert_eq!(root.size, 4096);
        assert!(root.is_dir());
        assert_eq!(root.modified, 1_700_000_000);
        assert_eq!(root.parent, NO_PARENT);
        assert_eq!(root.first_child, u32::MAX);
    }

    #[test]
    fn insert_and_parent_child_linking() {
        let mut tree = DirTree::new("/root");
        let file_a = make_file_node("a.txt", 100);
        let idx_a = tree.insert(0, file_a);
        assert_eq!(idx_a, 1);

        let root = tree.get(0).unwrap();
        // Single child: first_child = 1, no next sibling.
        assert_eq!(root.first_child, 1);
        assert_eq!(tree.get(1).unwrap().next_sibling, u32::MAX);

        let node_a = tree.get(idx_a).unwrap();
        assert_eq!(node_a.parent, 0);
        assert_eq!(&*node_a.name, "a.txt");
        assert_eq!(node_a.size, 100);

        let file_b = make_file_node("b.txt", 200);
        let idx_b = tree.insert(0, file_b);
        assert_eq!(idx_b, 2);

        // Prepend: B (index 2) is head, A (index 1) is next.
        let root = tree.get(0).unwrap();
        assert_eq!(root.first_child, 2);
        assert_eq!(tree.get(2).unwrap().next_sibling, 1);
        assert_eq!(tree.get(1).unwrap().next_sibling, u32::MAX);
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
    fn leaf_nodes_have_no_children() {
        let mut tree = DirTree::new("/root");
        let file = make_file_node("leaf.txt", 10);
        let idx = tree.insert(0, file);
        assert_eq!(tree.children(idx).count(), 0);
    }

    #[test]
    fn tombstone_leaf_file() {
        let mut tree = DirTree::new("/root");
        let file = make_file_node("a.txt", 100);
        let idx = tree.insert(0, file);

        tree.tombstone(idx);

        let node = tree.get(idx).unwrap();
        assert!(node.deleted());
        assert_eq!(node.size, 0);
        // Parent's child list no longer contains the tombstoned index.
        let children: Vec<u32> = tree.children(0).collect();
        assert!(!children.contains(&(idx as u32)));
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
            assert!(node.deleted(), "node at index {i} should be deleted");
            assert_eq!(node.size, 0, "node at index {i} should have size 0");
        }
        // Directory removed from parent's child list.
        let children: Vec<u32> = tree.children(0).collect();
        assert!(!children.contains(&(idx_sub as u32)));
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
            assert!(node.deleted(), "node at index {i} should be deleted");
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

    #[test]
    fn tombstone_already_tombstoned_is_idempotent() {
        let mut tree = DirTree::new("/root");
        let file_a = make_file_node("a.txt", 100);
        let idx_a = tree.insert(0, file_a);
        let file_b = make_file_node("b.txt", 200);
        let idx_b = tree.insert(0, file_b);

        tree.tombstone(idx_a);

        // Snapshot state after first tombstone.
        let root_children_after_first: Vec<u32> = tree.children(0).collect();
        let node_a_deleted = tree.get(idx_a).unwrap().deleted();
        let node_a_size = tree.get(idx_a).unwrap().size;
        let arena_len = tree.len();

        // Tombstone the same node again — should not panic or double-remove.
        tree.tombstone(idx_a);

        // State unchanged: still deleted, still zero size, arena same length.
        assert!(tree.get(idx_a).unwrap().deleted());
        assert_eq!(tree.get(idx_a).unwrap().size, 0);
        assert_eq!(tree.len(), arena_len);
        let root_children_after_second: Vec<u32> = tree.children(0).collect();
        assert_eq!(root_children_after_second, root_children_after_first);
        // Sibling b.txt is unaffected.
        assert!(!tree.get(idx_b).unwrap().deleted());
        assert_eq!(tree.get(idx_b).unwrap().size, 200);
        // Sanity: first tombstone did mark it.
        assert!(node_a_deleted);
        assert_eq!(node_a_size, 0);
    }

    #[test]
    fn tombstone_leaf_in_deep_tree_reduces_ancestor_subtree_sizes() {
        // Build a 5-level deep tree: root -> d1 -> d2 -> d3 -> d4 -> leaf (500 bytes)
        // Plus a sibling file at each level to verify only the leaf's size is removed.
        let mut tree = DirTree::new("/root");
        let d1 = tree.insert(0, make_dir_node("d1"));
        let f_root = tree.insert(0, make_file_node("root_file.txt", 10));
        let d2 = tree.insert(d1, make_dir_node("d2"));
        let f_d1 = tree.insert(d1, make_file_node("d1_file.txt", 20));
        let d3 = tree.insert(d2, make_dir_node("d3"));
        let f_d2 = tree.insert(d2, make_file_node("d2_file.txt", 30));
        let d4 = tree.insert(d3, make_dir_node("d4"));
        let f_d3 = tree.insert(d3, make_file_node("d3_file.txt", 40));
        let leaf = tree.insert(d4, make_file_node("deep_leaf.txt", 500));

        // Before tombstone: total = 10+20+30+40+500 = 600
        assert_eq!(tree.subtree_size(0), 600);
        assert_eq!(tree.subtree_size(d1), 590);
        assert_eq!(tree.subtree_size(d2), 570);
        assert_eq!(tree.subtree_size(d3), 540);
        assert_eq!(tree.subtree_size(d4), 500);

        tree.tombstone(leaf);

        // After tombstone: the leaf's 500 bytes removed from every ancestor.
        assert_eq!(tree.subtree_size(0), 100);
        assert_eq!(tree.subtree_size(d1), 90);
        assert_eq!(tree.subtree_size(d2), 70);
        assert_eq!(tree.subtree_size(d3), 40);
        assert_eq!(tree.subtree_size(d4), 0);

        // Sibling files unaffected.
        assert_eq!(tree.get(f_root).unwrap().size, 10);
        assert_eq!(tree.get(f_d1).unwrap().size, 20);
        assert_eq!(tree.get(f_d2).unwrap().size, 30);
        assert_eq!(tree.get(f_d3).unwrap().size, 40);
    }

    #[test]
    fn tombstone_removes_from_parent_children_iteration() {
        let mut tree = DirTree::new("/root");
        let file_a = tree.insert(0, make_file_node("a.txt", 100));
        let file_b = tree.insert(0, make_file_node("b.txt", 200));
        let file_c = tree.insert(0, make_file_node("c.txt", 300));

        // Tombstone the middle child.
        tree.tombstone(file_b);

        // Iterating parent's children must not include the tombstoned index.
        let children: Vec<u32> = tree.children(0).collect();
        assert!(
            !children.contains(&(file_b as u32)),
            "tombstoned index should not appear in parent's children"
        );
        assert!(
            children.contains(&(file_a as u32)),
            "non-tombstoned sibling a should remain"
        );
        assert!(
            children.contains(&(file_c as u32)),
            "non-tombstoned sibling c should remain"
        );
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn filenode_memory_size_regression() {
        let size = std::mem::size_of::<FileNode>();
        println!("size_of::<FileNode>() = {size} bytes");
        // Compact FileNode: Box<str>(16) + u64(8) + u64(8) + u32(4) +
        // u32(4) + u32(4) + u16(2) + u8(1) + padding(1) = 48 bytes
        assert!(size <= 48, "FileNode is {size} bytes, expected <= 48");
        assert!(
            size >= 40,
            "FileNode is {size} bytes, expected >= 40 (suspiciously small)"
        );
    }

    #[test]
    fn dirtree_memory_size_regression() {
        let size = std::mem::size_of::<DirTree>();
        println!("size_of::<DirTree>() = {size} bytes");
        // DirTree wraps Vec<FileNode> + Vec<Box<str>>, so ~48 bytes on 64-bit.
        assert!(size <= 64, "DirTree is {size} bytes, expected <= 64");
    }

    #[test]
    fn new_with_capacity_creates_root() {
        let tree = DirTree::new_with_capacity("root", 1000);
        assert_eq!(tree.root(), 0);
        assert_eq!(tree.len(), 1);
        let root = tree.get(0).unwrap();
        assert_eq!(&*root.name, "root");
        assert!(root.is_dir());
        assert_eq!(root.parent, NO_PARENT);
        assert_eq!(root.first_child, u32::MAX);
    }

    #[test]
    fn from_root_with_capacity_preserves_fields() {
        let node = FileNode {
            name: "/tmp/scan".into(),
            size: 4096,
            modified: 1_700_000_000,
            parent: 42,       // should be cleared
            first_child: 99,  // should be cleared
            next_sibling: 99, // should be cleared
            extension: 0,
            flags: 1, // is_dir
        };
        let tree = DirTree::from_root_with_capacity(node, 500);
        assert_eq!(tree.len(), 1);
        let root = tree.get(0).unwrap();
        assert_eq!(&*root.name, "/tmp/scan");
        assert_eq!(root.size, 4096);
        assert!(root.is_dir());
        assert_eq!(root.modified, 1_700_000_000);
        assert_eq!(root.parent, NO_PARENT);
        assert_eq!(root.first_child, u32::MAX);
    }

    #[test]
    fn intern_extension_round_trip() {
        let mut tree = DirTree::new("/root");
        let idx = tree.intern_extension(Some("rs"));
        assert_ne!(idx, 0);
        assert_eq!(tree.extension_str(idx), Some("rs"));

        // Same extension returns same index.
        let idx2 = tree.intern_extension(Some("rs"));
        assert_eq!(idx, idx2);

        // None returns 0.
        assert_eq!(tree.intern_extension(None), 0);
        assert_eq!(tree.extension_str(0), None);
    }
}
