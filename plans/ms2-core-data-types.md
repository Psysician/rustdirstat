# Plan

## Overview

rustdirstat has no foundational data types. All other crates (rds-scanner, rds-gui, binary) depend on shared types for file nodes, directory trees, scan events, configuration, and extension statistics. Without rds-core, no other milestone can proceed.

**Approach**: Create rds-core crate with four submodules (tree, scan, config, stats), each owning a distinct type group. DirTree uses arena allocation (Vec<FileNode> with usize indices) for cache locality and safe concurrent access patterns. All types derive serde traits for serialization. Unit tests inline per module verify tree operations, default values, serde round-trips, and Send+Sync guarantees.

## Planning Context

### Decision Log

| ID | Decision | Reasoning Chain |
|---|---|---|
| DL-001 | Modular subfile organization for rds-core | Single-file monolith works for small crates but rds-core defines 7+ public types across different concerns (tree, events, config, stats) -> grouping all in lib.rs hurts navigability as crate grows -> separate modules with re-exports from lib.rs follows idiomatic Rust crate organization and satisfies single-responsibility per module |
| DL-002 | Arena allocation with raw usize indices, no newtype wrapper — indices exposed in public API | Spec mandates Vec<FileNode> with usize index-based references -> DirTree methods (insert, subtree_size, path, get) and ScanEvent::NodeDiscovered all expose raw usize in public signatures -> newtype NodeIndex(usize) would add type safety but increases boilerplate and diverges from spec -> chose raw usize matching spec for MS2; if misuse bugs emerge, wrapping is backward-compatible in a later milestone. Note: contrary to initial claim, usize indices DO cross API boundaries (ScanEvent, DirTree public methods) but the simplicity tradeoff still holds. |
| DL-003 | Deterministic HSL color via byte-sum hash of extension string (see DL-010 for algorithm choice) | Spec requires same extension always gets same color -> hashing extension string to hue value in 0..360 range provides deterministic mapping without needing a lookup table or assignment tracking -> fixed saturation (70%) and lightness (50%) produce visually distinct, readable colors across the hue spectrum -> DL-010 specifies byte-sum modulo 360 as the concrete algorithm, chosen because std::collections::hash_map::DefaultHasher is randomly seeded per process and non-deterministic across runs |
| DL-004 | ScanError stores error as String, not io::Error | ScanEvent must derive or support serde serialization for export (MS16) and cross-channel communication -> std::io::Error does not implement serde::Serialize or serde::Deserialize, making it incompatible with derive macros on ScanEvent -> converting to String at creation time preserves the error message, enables serde derives on the enum, and as a side benefit guarantees Send+Sync (though io::Error is already Send+Sync, that is not the primary motivation) |
| DL-005 | Inline #[cfg(test)] mod tests per module file | Rust convention places unit tests in same file as code under test -> tests for tree ops in tree.rs, tests for stats in stats.rs -> avoids separate test files, keeps test close to implementation, and cargo test -p rds-core runs all automatically |
| DL-006 | Four submodules: tree, scan, config, stats | rds-core has 4 distinct type groups: (1) FileNode+DirTree = tree structure, (2) ScanEvent+ScanStats+ScanConfig = scanning concerns, (3) AppConfig = application configuration, (4) ExtensionStats = per-extension aggregation with color -> mapping each to a module produces clean single-responsibility boundaries without over-splitting |
| DL-007 | DuplicateFound uses [u8; 32] hash field, committing to SHA-256 | Spec requires SHA-256 for duplicate detection (sha2 crate listed as dependency) -> SHA-256 produces 32-byte digests -> [u8; 32] is the natural fixed-size representation -> alternatives: SHA-1 (20 bytes, cryptographically broken, unsuitable), SHA-512 (64 bytes, overkill for file identity), BLAKE3 (32 bytes, faster but adds non-spec dependency) -> SHA-256 is the spec choice, widely trusted for file integrity, and 32 bytes is small enough to store inline in the enum variant |
| DL-008 | FileNode fields are pub (fully public), not pub(crate) | rds-gui must read FileNode fields directly (name for tree view, size for treemap layout, extension for color lookup, children for traversal) -> pub(crate) would restrict access to rds-core only, requiring getter methods for every field -> spec defines FileNode as a plain data struct consumed by multiple crates -> pub fields match the data-transfer-object pattern and avoid boilerplate accessor methods |
| DL-009 | DirTree root is always index 0; new() takes root node name/path, arena is never empty | Arena-based tree needs a stable root reference -> index 0 as root sentinel is simplest convention (no Option<usize> root field needed, no empty-tree state to handle) -> DirTree::new(root_name) creates the root FileNode at index 0 with parent=None -> callers always know root_index=0 -> path() reconstruction terminates when parent is None (i.e., at index 0) -> this means DirTree cannot represent an empty tree, which is acceptable because a scan always has at least a root directory |
| DL-010 | Extension color hashing uses a simple deterministic byte-sum algorithm, not std::collections::hash_map::DefaultHasher | color_for_extension must be a pure function producing identical output across runs and Rust versions -> std::collections::hash_map::DefaultHasher uses SipHash with random seeding (since Rust 1.36), so it produces different results per process invocation -> a simple sum-of-bytes modulo 360 is deterministic, platform-independent, and sufficient for distributing hue values across the color wheel -> collision resistance is irrelevant since this is visual-only (two extensions getting similar colors is acceptable) |
| DL-011 | ScanConfig.root and ScanError.path use PathBuf; platform path differences deferred to scanner and error-handling milestones | ScanConfig.root and ScanError.path represent filesystem paths -> PathBuf is the idiomatic Rust type for owned filesystem paths and handles OS-native path representations (including non-UTF8 on Unix and UNC/long paths on Windows) -> alternatives: (1) String would lose non-UTF8 path support on Unix and require manual conversion, (2) OsString lacks serde support without custom impls -> PathBuf has built-in serde support via serde derive macros (serializes as string, lossy for non-UTF8) -> platform-specific edge cases (Windows UNC paths, MAX_PATH=260 limits, non-UTF8 paths on Unix causing lossy serialization) do not affect type definitions in MS2; they affect path construction and error handling in rds-scanner (MS3/MS4) and are polished in MS18 (error handling) and MS20 (cross-platform) -> at the type level, PathBuf is correct and sufficient; no special handling or wrapper type is needed |

### Constraints

- rds-core has zero dependencies beyond std and serde (per design spec)
- FileNode contains name, size, metadata, children indices, parent index — all fields pub
- DirTree uses arena allocation (Vec<FileNode> with index-based references)
- DirTree provides traversal, subtree size computation, path reconstruction
- ScanEvent enum has variants: NodeDiscovered, Progress, DuplicateFound, ScanComplete, ScanError
- ScanConfig contains root path, follow_symlinks, exclude patterns, hash_duplicates toggle, max_nodes Option<usize> (default 10_000_000)
- ExtensionStats provides per-extension aggregation: count, total size, percentage, assigned color
- AppConfig is serde-deserializable for TOML loading
- cargo test -p rds-core passes with full coverage of tree operations
- Unit tests cover insert, subtree size, path reconstruction, parent-child linking
- No code snippets or examples in the plan document

### Known Risks

- **Arena memory overflow at 10M nodes (~2GB at ~200 bytes per node)**: max_nodes defaults to Some(10_000_000) in ScanConfig; scanner aborts with ScanError when exceeded. Users can reduce the limit or scan subdirectories. Memory estimation should be validated in MS19 performance milestone.
- **serde derive incompatibility with certain field types**: All field types (String, u64, bool, Vec, Option, PathBuf, [u8;32]) have serde impls in std or serde itself. ScanEvent uses Clone+Debug (not Serialize) to avoid issues with FileNode inside the enum. Only ScanStats, ScanConfig, AppConfig, ExtensionStats need Serialize+Deserialize.
- **Non-deterministic color hashing if wrong hash function used**: DL-010 specifies byte-sum algorithm, explicitly prohibiting std DefaultHasher. Code intent CI-M-001-006 and tests enforce determinism.
- **PathBuf platform differences (Windows UNC paths, MAX_PATH limits, non-UTF8 Unix paths) in ScanConfig.root and ScanError.path fields**: MS2 only defines types; path handling logic lives in rds-scanner (MS3/MS4). exclude_patterns uses Vec<String> which is UTF-8; glob matching against OsStr paths is an MS3 concern. PathBuf is the correct type per Rust conventions (see DL-011); platform edge cases (UNC paths, MAX_PATH, non-UTF8 paths) are addressed in MS18 (error handling) and MS20 (cross-platform polish). No special handling needed at the type-definition level in MS2.

## Invisible Knowledge

### System

Arena allocation chosen over tree of Rc/Box for cache locality and to avoid reference counting overhead. Index-based parent-child linking enables safe concurrent access patterns (indices are Copy, no borrow issues). ScanEvent is designed to be Send so it can cross the crossbeam channel from scanner thread to GUI thread. ExtensionStats color assignment is deterministic (same extension always gets same color) via hash-based HSL. DuplicateFound uses [u8; 32] for SHA-256 hash (see DL-007). ScanError serializes error to String because std::io::Error does not implement serde::Serialize, making it incompatible with serde derive macros on ScanEvent (see DL-004). Root node is always at arena index 0; DirTree cannot represent an empty tree (see DL-009). FileNode fields are pub for cross-crate access by rds-gui (see DL-008). Extension color hashing uses deterministic byte-sum, not DefaultHasher which is randomly seeded (see DL-010).

### Invariants

- Every FileNode in the arena except root has a parent index pointing to a valid node
- Every parent node's children vec contains the index of each child that references it
- DirTree arena indices are stable: once assigned, an index always refers to the same node. MS13 (delete action) must use tombstoning (mark node as deleted, zero its size) rather than removing from Vec, to preserve index validity for all other references
- color_for_extension is a pure function: same input always produces same HslColor output
- ScanConfig::default().max_nodes is Some(10_000_000)
- All ScanEvent variants are Send + Sync

### Tradeoffs

- Raw usize indices instead of newtype wrapper: less type safety but matches spec and reduces boilerplate
- String error in ScanError instead of typed error enum: loses structured error info but enables serde derive on ScanEvent (io::Error lacks Serialize) and simplifies enum design
- Simple byte-sum hash for extension color instead of cryptographic hash: fast and sufficient for color distribution, not collision-resistant but that is acceptable for visual-only use

## Milestones

### Milestone 1: rds-core data types and tree operations

**Files**: crates/rds-core/src/lib.rs, crates/rds-core/src/tree.rs, crates/rds-core/src/scan.rs, crates/rds-core/src/config.rs, crates/rds-core/src/stats.rs, crates/rds-core/Cargo.toml

#### Code Intent

- **CI-M-001-001** `crates/rds-core/Cargo.toml`: Declares rds-core crate with serde dependency (features = ["derive"]). No other external dependencies. Edition 2021.
- **CI-M-001-002** `crates/rds-core/src/lib.rs`: Root module declaring four submodules (tree, scan, config, stats) and re-exporting all public types from each. Public API surface: FileNode, DirTree, ScanEvent, ScanStats, ScanConfig, AppConfig, ExtensionStats, HslColor, CustomCommand. (refs: DL-001, DL-006, DL-008)
- **CI-M-001-003** `crates/rds-core/src/tree.rs`: FileNode struct with all fields pub: name (String), size (u64), is_dir (bool), children (Vec<usize>), parent (Option<usize>), extension (Option<String>), modified (Option<u64> epoch seconds). Derives Clone, Debug, Serialize, Deserialize. DirTree struct wrapping Vec<FileNode>. Root is always at index 0 (arena is never empty). Methods: new(root_name: &str) creates tree with root FileNode at index 0 (is_dir=true, parent=None, size=0); insert(parent_index: usize, node: FileNode) -> usize appends node to arena, sets node.parent, adds index to parent.children, returns new index; subtree_size(index) -> u64 computes recursive size sum; path(index) -> PathBuf reconstructs full path by walking parent chain to root (parent=None terminates); get(index) -> Option<&FileNode>; get_mut(index) -> Option<&mut FileNode>; children(index) -> slice of child indices; root() -> usize returns 0; len() -> usize returns total node count. Invariants: every non-root node has valid parent index; every parent.children contains each child that references it; arena indices are stable (no removal, append-only in MS2). Tests cover: insert and parent-child linking, subtree_size on nested tree, path reconstruction through 3+ levels, root node properties (index 0, parent None, is_dir true), empty children for leaf nodes. (refs: DL-002, DL-005, DL-008, DL-009)
- **CI-M-001-004** `crates/rds-core/src/scan.rs`: ScanEvent enum with variants: NodeDiscovered { node: FileNode, parent_index: Option<usize> }, Progress { files_scanned: u64, bytes_scanned: u64 }, DuplicateFound { hash: [u8; 32], node_indices: Vec<usize> }, ScanComplete { stats: ScanStats }, ScanError { path: PathBuf, error: String }. Derives Clone, Debug. ScanStats struct: total_files (u64), total_dirs (u64), total_bytes (u64), duration_ms (u64), errors (u64). Derives Clone, Debug, Serialize, Deserialize. ScanConfig struct: root (PathBuf), follow_symlinks (bool, default false), exclude_patterns (Vec<String>), hash_duplicates (bool, default true), max_nodes (Option<usize> with Default impl returning Some(10_000_000)). Derives Clone, Debug, Serialize, Deserialize. PathBuf chosen for root and path fields per DL-011; platform path edge cases (UNC, MAX_PATH, non-UTF8) deferred to scanner/error-handling milestones. Invariants: all ScanEvent variants are Send + Sync (compiler-enforced via static assertions in tests); ScanConfig::default().max_nodes == Some(10_000_000). Tests cover: ScanConfig default values, ScanEvent variant construction, Send+Sync static assertions for ScanEvent and ScanConfig. (refs: DL-004, DL-005, DL-007, DL-011)
- **CI-M-001-005** `crates/rds-core/src/config.rs`: AppConfig struct: exclude_patterns (Vec<String>), custom_commands (Vec<CustomCommand>), color_scheme (String, default "default"), default_sort (String, default "size_desc"), recent_paths (Vec<PathBuf>, default empty). CustomCommand struct: name (String), template (String with {path} placeholder). Both derive Clone, Debug, Serialize, Deserialize, Default. Tests cover: default construction, serde round-trip (serialize then deserialize produces equal value). (refs: DL-005)
- **CI-M-001-006** `crates/rds-core/src/stats.rs`: ExtensionStats struct: extension (String), count (u64), total_bytes (u64), percentage (f64), color (HslColor). HslColor struct: h (f64 0..360), s (f64 0..1), l (f64 0..1). Derives Clone, Debug, Serialize, Deserialize. Function color_for_extension(ext: &str) -> HslColor: computes hue by summing all bytes of the extension string and taking modulo 360, fixed saturation 0.7, fixed lightness 0.5. Must NOT use std::collections::hash_map::DefaultHasher (randomly seeded). Invariant: color_for_extension is a pure function — same input always produces same HslColor output across runs, platforms, and Rust versions. Function compute_extension_stats(tree: &DirTree) -> Vec<ExtensionStats>: iterates all file nodes, groups by extension, computes count/total_bytes/percentage per extension, assigns color via color_for_extension, returns sorted by total_bytes descending. Tests cover: color_for_extension determinism (same input same output across multiple calls), color_for_extension different extensions produce different hues (with high probability), compute_extension_stats on a small tree produces correct counts and percentages. (refs: DL-003, DL-005, DL-010)

#### Code Changes

**CC-M-001-001** (crates/rds-core/Cargo.toml) - implements CI-M-001-001

**Code:**

```diff
--- a/crates/rds-core/Cargo.toml
+++ b/crates/rds-core/Cargo.toml
@@ -1,11 +1,12 @@
-# rds-core: shared data types. Zero external dependencies beyond serde.
-# Keeps core types fast to compile and independently testable.
-#
 [package]
 name = "rds-core"
 version = "0.1.0"
 edition = "2024"
 
 [dependencies]
 serde = { workspace = true }
+
+[dev-dependencies]
+serde_json = { workspace = true }

```

**Documentation:**

```diff
--- a/crates/rds-core/Cargo.toml
+++ b/crates/rds-core/Cargo.toml
@@ -1,3 +1,6 @@
+# rds-core: shared data types for all rds crates. (DL-001)
+# Zero external dependencies beyond serde; keeps compilation fast and prevents
+# IO or GUI transitive dependencies from entering the shared type layer. (DL-001)
+# serde_json is dev-only for round-trip tests.
 [package]
 name = "rds-core"
 version = "0.1.0"

```


**CC-M-001-003** (crates/rds-core/src/lib.rs) - implements CI-M-001-002

**Code:**

```diff
--- a/crates/rds-core/src/lib.rs
+++ b/crates/rds-core/src/lib.rs
@@ -1,17 +1,16 @@
-//\! Core data types shared across all crates.
-//\!
-//\! Depends only on `serde` beyond `std` so it compiles fast and tests run
-//\! without pulling in IO or GUI dependencies.
-//\!
-//\! The file tree is represented as an arena-allocated `Vec<FileNode>` with
-//\! `usize` index references rather than `Rc`/`Box` pointers, giving cache-local
-//\! traversal and zero reference-counting overhead.
+pub mod tree;
+pub mod scan;
+pub mod config;
+pub mod stats;
 
-#[cfg(test)]
-mod tests {
-    #[test]
-    fn crate_compiles() {
-        let result = 2 + 2;
-        assert_eq\!(result, 4);
-    }
-}
+pub use tree::{FileNode, DirTree};
+pub use scan::{ScanEvent, ScanStats, ScanConfig};
+pub use config::{AppConfig, CustomCommand};
+pub use stats::{ExtensionStats, HslColor, color_for_extension, compute_extension_stats};

```

**Documentation:**

```diff
--- a/crates/rds-core/src/lib.rs
+++ b/crates/rds-core/src/lib.rs
@@ -1,4 +1,18 @@
+//! Core data types shared across all rds crates.
+//!
+//! Each concern lives in its own submodule rather than a single monolithic
+//! file; this keeps each module navigable as the type count grows. (DL-001)
+//!
+//! Depends only on `serde` beyond `std` so it compiles fast and tests run
+//! without pulling in IO or GUI dependencies.
+//!
+//! The file tree is represented as an arena-allocated `Vec<FileNode>` with
+//! `usize` index references rather than `Rc`/`Box` pointers, giving cache-local
+//! traversal and zero reference-counting overhead. (DL-002)
+//!
+//! Four submodules map to distinct type groups: `tree` (arena + node),
+//! `scan` (events, config, stats), `config` (app settings), `stats`
+//! (per-extension aggregation). (DL-006)
+//!
+//! Each module file contains its own `#[cfg(test)] mod tests` block. (DL-005)
 pub mod tree;
 pub mod scan;
 pub mod config;

```


**CC-M-001-004** (crates/rds-core/src/tree.rs) - implements CI-M-001-003

**Code:**

```diff
--- /dev/null
+++ b/crates/rds-core/src/tree.rs
@@ -0,0 +1,200 @@
+use std::path::PathBuf;
+use serde::{Serialize, Deserialize};
+
+#[derive(Clone, Debug, Serialize, Deserialize)]
+pub struct FileNode {
+    pub name: String,
+    pub size: u64,
+    pub is_dir: bool,
+    pub children: Vec<usize>,
+    pub parent: Option<usize>,
+    pub extension: Option<String>,
+    pub modified: Option<u64>,
+}
+
+#[derive(Debug)]
+pub struct DirTree {
+    nodes: Vec<FileNode>,
+}
+
+impl DirTree {
+    pub fn new(root_name: &str) -> Self {
+        let root = FileNode {
+            name: root_name.to_string(),
+            size: 0,
+            is_dir: true,
+            children: Vec::new(),
+            parent: None,
+            extension: None,
+            modified: None,
+        };
+        DirTree {
+            nodes: vec![root],
+        }
+    }
+
+    /// Inserts a node under `parent_index`. Panics if `parent_index` is out of bounds.
+    pub fn insert(&mut self, parent_index: usize, mut node: FileNode) -> usize {
+        assert!(parent_index < self.nodes.len(), "parent_index out of bounds");
+        let new_index = self.nodes.len();
+        node.parent = Some(parent_index);
+        self.nodes.push(node);
+        self.nodes[parent_index].children.push(new_index);
+        new_index
+    }
+
+    pub fn subtree_size(&self, index: usize) -> u64 {
+        assert!(index < self.nodes.len(), "index out of bounds");
+        let mut total: u64 = 0;
+        let mut stack = vec![index];
+        while let Some(i) = stack.pop() {
+            let node = &self.nodes[i];
+            total += node.size;
+            for &child_idx in &node.children {
+                stack.push(child_idx);
+            }
+        }
+        total
+    }
+
+    /// Reconstructs the path from root to the node at `index`. Panics if `index` is out of bounds.
+    pub fn path(&self, index: usize) -> PathBuf {
+        assert!(index < self.nodes.len(), "index out of bounds");
+        let mut components = Vec::new();
+        let mut current = index;
+        loop {
+            components.push(self.nodes[current].name.as_str());
+            match self.nodes[current].parent {
+                Some(parent_idx) => current = parent_idx,
+                None => break,
+            }
+        }
+        components.reverse();
+        let mut path = PathBuf::new();
+        for component in components {
+            path.push(component);
+        }
+        path
+    }
+
+    pub fn get(&self, index: usize) -> Option<&FileNode> {
+        self.nodes.get(index)
+    }
+
+    pub fn get_mut(&mut self, index: usize) -> Option<&mut FileNode> {
+        self.nodes.get_mut(index)
+    }
+
+    /// Returns the children indices of the node at `index`. Panics if `index` is out of bounds.
+    pub fn children(&self, index: usize) -> &[usize] {
+        assert!(index < self.nodes.len(), "index out of bounds");
+        &self.nodes[index].children
+    }
+
+    pub fn root(&self) -> usize {
+        0
+    }
+
+    pub fn len(&self) -> usize {
+        self.nodes.len()
+    }
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    fn make_file_node(name: &str, size: u64) -> FileNode {
+        FileNode {
+            name: name.to_string(),
+            size,
+            is_dir: false,
+            children: Vec::new(),
+            parent: None,
+            extension: None,
+            modified: None,
+        }
+    }
+
+    fn make_dir_node(name: &str) -> FileNode {
+        FileNode {
+            name: name.to_string(),
+            size: 0,
+            is_dir: true,
+            children: Vec::new(),
+            parent: None,
+            extension: None,
+            modified: None,
+        }
+    }
+
+    #[test]
+    fn root_node_properties() {
+        let tree = DirTree::new("/root");
+        assert_eq!(tree.root(), 0);
+        assert_eq!(tree.len(), 1);
+        let root = tree.get(0).unwrap();
+        assert_eq!(root.name, "/root");
+        assert!(root.is_dir);
+        assert_eq!(root.parent, None);
+        assert!(root.children.is_empty());
+        assert_eq!(root.size, 0);
+    }
+
+    #[test]
+    fn insert_and_parent_child_linking() {
+        let mut tree = DirTree::new("/root");
+        let file_a = make_file_node("a.txt", 100);
+        let idx_a = tree.insert(0, file_a);
+        assert_eq!(idx_a, 1);
+
+        let root = tree.get(0).unwrap();
+        assert_eq!(root.children, vec![1]);
+
+        let node_a = tree.get(idx_a).unwrap();
+        assert_eq!(node_a.parent, Some(0));
+        assert_eq!(node_a.name, "a.txt");
+        assert_eq!(node_a.size, 100);
+
+        let file_b = make_file_node("b.txt", 200);
+        let idx_b = tree.insert(0, file_b);
+        assert_eq!(idx_b, 2);
+
+        let root = tree.get(0).unwrap();
+        assert_eq!(root.children, vec![1, 2]);
+        assert_eq!(tree.len(), 3);
+    }
+
+    #[test]
+    fn subtree_size_nested() {
+        let mut tree = DirTree::new("/root");
+        let subdir = make_dir_node("subdir");
+        let idx_sub = tree.insert(0, subdir);
+
+        let file_a = make_file_node("a.txt", 100);
+        tree.insert(idx_sub, file_a);
+
+        let file_b = make_file_node("b.txt", 250);
+        tree.insert(idx_sub, file_b);
+
+        let file_c = make_file_node("c.txt", 50);
+        tree.insert(0, file_c);
+
+        assert_eq!(tree.subtree_size(idx_sub), 350);
+        assert_eq!(tree.subtree_size(0), 400);
+    }
+
+    #[test]
+    fn path_reconstruction_three_levels() {
+        let mut tree = DirTree::new("/root");
+        let dir_a = make_dir_node("dir_a");
+        let idx_a = tree.insert(0, dir_a);
+
+        let dir_b = make_dir_node("dir_b");
+        let idx_b = tree.insert(idx_a, dir_b);
+
+        let file = make_file_node("file.txt", 42);
+        let idx_file = tree.insert(idx_b, file);
+
+        let path = tree.path(idx_file);
+        assert_eq!(path, PathBuf::from("/root/dir_a/dir_b/file.txt"));
+
+        let path_b = tree.path(idx_b);
+        assert_eq!(path_b, PathBuf::from("/root/dir_a/dir_b"));
+
+        let path_root = tree.path(0);
+        assert_eq!(path_root, PathBuf::from("/root"));
+    }
+
+    #[test]
+    fn leaf_nodes_have_empty_children() {
+        let mut tree = DirTree::new("/root");
+        let file = make_file_node("leaf.txt", 10);
+        let idx = tree.insert(0, file);
+        assert!(tree.children(idx).is_empty());
+    }
+}

```

**Documentation:**

```diff
--- a/crates/rds-core/src/tree.rs
+++ b/crates/rds-core/src/tree.rs
@@ -1,8 +1,28 @@
+//! Arena-allocated directory tree. (DL-001)
+//!
+//! `DirTree` stores all nodes in a flat `Vec<FileNode>`. Parent and child
+//! relationships are expressed as `usize` indices into that vec. This avoids
+//! reference-counting overhead and keeps traversal cache-local. (DL-002)
+//!
+//! The arena is insert-only: nodes are appended and never removed. This keeps
+//! all previously returned indices valid for the lifetime of the tree, which
+//! enables safe index-based linking in `ScanEvent::NodeDiscovered` and across
+//! the channel boundary to the GUI thread.
 use std::path::PathBuf;
 use serde::{Serialize, Deserialize};

+/// A single filesystem entry (file or directory) stored inside a `DirTree`.
+///
+/// Fields are `pub` so downstream crates (`rds-gui`) can read them directly
+/// without accessor boilerplate. (DL-008)
 #[derive(Clone, Debug, Serialize, Deserialize)]
 pub struct FileNode {
+    /// Entry name (not full path). Combine with `DirTree::path` for the full path.
     pub name: String,
+    /// Disk size in bytes. For directories, this is 0; use `DirTree::subtree_size`.
     pub size: u64,
+    /// `true` if the entry is a directory.
     pub is_dir: bool,
+    /// Indices of child nodes in the owning `DirTree`'s arena. (DL-002)
     pub children: Vec<usize>,
+    /// Index of the parent node, or `None` for the root. (DL-009)
     pub parent: Option<usize>,
+    /// Lowercased file extension without leading dot, or `None` for entries with no extension.
     pub extension: Option<String>,
+    /// Last-modified time as a Unix timestamp (seconds), if available.
     pub modified: Option<u64>,
 }

+/// Arena-allocated file tree.
+///
+/// All nodes live in a single `Vec`. Parent/child links are `usize` indices
+/// into that vec. The arena is insert-only: nodes are never removed, so all
+/// previously returned indices remain valid. The root node is always at
+/// index `0` and is created by `DirTree::new`. The arena is never empty. (DL-009)
 #[derive(Debug)]
 pub struct DirTree {
     nodes: Vec<FileNode>,
 }

 impl DirTree {
+    /// Creates a new tree with a single root directory node named `root_name`.
+    ///
+    /// The root is at index `0` with `parent = None`. (DL-009)
     pub fn new(root_name: &str) -> Self {
@@ -38,14 +56,22 @@ impl DirTree {
     }

-    /// Inserts a node under `parent_index`. Panics if `parent_index` is out of bounds.
+    /// Inserts `node` as a child of `parent_index` and returns the new node's index.
+    ///
+    /// Sets `node.parent` to `parent_index` and appends the new index to the
+    /// parent's `children` list. Panics if `parent_index` is out of bounds.
     pub fn insert(&mut self, parent_index: usize, mut node: FileNode) -> usize {
@@ -53,6 +79,10 @@ impl DirTree {
     }

+    /// Sums `size` across all nodes in the subtree rooted at `index`.
+    ///
+    /// Uses an iterative stack traversal to avoid recursion limits on deep trees.
+    /// Panics if `index` is out of bounds.
     pub fn subtree_size(&self, index: usize) -> u64 {
@@ -68,8 +98,12 @@ impl DirTree {
     }

-    /// Reconstructs the path from root to the node at `index`. Panics if `index` is out of bounds.
+    /// Reconstructs the full path from the root to the node at `index`.
+    ///
+    /// Walks `parent` links from `index` up to the root (where `parent` is `None`),
+    /// then reverses the collected name components into a `PathBuf`. Panics if
+    /// `index` is out of bounds.
     pub fn path(&self, index: usize) -> PathBuf {
@@ -87,10 +121,14 @@ impl DirTree {
         path
     }

+    /// Returns a shared reference to the node at `index`, or `None` if out of bounds.
     pub fn get(&self, index: usize) -> Option<&FileNode> {
@@ -99,10 +137,12 @@ impl DirTree {
     }

+    /// Returns an exclusive reference to the node at `index`, or `None` if out of bounds.
     pub fn get_mut(&mut self, index: usize) -> Option<&mut FileNode> {
@@ -111,10 +153,12 @@ impl DirTree {
     }

-    /// Returns the children indices of the node at `index`. Panics if `index` is out of bounds.
+    /// Returns the child indices of the node at `index`. Panics if `index` is out of bounds.
     pub fn children(&self, index: usize) -> &[usize] {
@@ -122,8 +166,10 @@ impl DirTree {
     }

+    /// Returns the root index, which is always `0`.
     pub fn root(&self) -> usize {
@@ -132,6 +178,8 @@
     }

+    /// Returns the total number of nodes in the arena.
     pub fn len(&self) -> usize {

```


**CC-M-001-005** (crates/rds-core/src/scan.rs) - implements CI-M-001-004

**Code:**

```diff
--- /dev/null
+++ b/crates/rds-core/src/scan.rs
@@ -0,0 +1,107 @@
+use std::path::PathBuf;
+use serde::{Serialize, Deserialize};
+use crate::tree::FileNode;
+
+#[derive(Clone, Debug)]
+pub enum ScanEvent {
+    NodeDiscovered {
+        node: FileNode,
+        parent_index: Option<usize>,
+    },
+    Progress {
+        files_scanned: u64,
+        bytes_scanned: u64,
+    },
+    DuplicateFound {
+        hash: [u8; 32],
+        node_indices: Vec<usize>,
+    },
+    ScanComplete {
+        stats: ScanStats,
+    },
+    ScanError {
+        path: PathBuf,
+        error: String,
+    },
+}
+
+#[derive(Clone, Debug, Serialize, Deserialize)]
+pub struct ScanStats {
+    pub total_files: u64,
+    pub total_dirs: u64,
+    pub total_bytes: u64,
+    pub duration_ms: u64,
+    pub errors: u64,
+}
+
+#[derive(Clone, Debug, Serialize, Deserialize)]
+pub struct ScanConfig {
+    pub root: PathBuf,
+    pub follow_symlinks: bool,
+    pub exclude_patterns: Vec<String>,
+    pub hash_duplicates: bool,
+    pub max_nodes: Option<usize>,
+}
+
+impl Default for ScanConfig {
+    fn default() -> Self {
+        ScanConfig {
+            root: PathBuf::new(),
+            follow_symlinks: false,
+            exclude_patterns: Vec::new(),
+            hash_duplicates: true,
+            max_nodes: Some(10_000_000),
+        }
+    }
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[test]
+    fn scan_config_defaults() {
+        let config = ScanConfig::default();
+        assert\!(\!config.follow_symlinks);
+        assert\!(config.hash_duplicates);
+        assert_eq\!(config.max_nodes, Some(10_000_000));
+        assert\!(config.exclude_patterns.is_empty());
+    }
+
+    #[test]
+    fn scan_event_node_discovered_construction() {
+        let node = FileNode {
+            name: "test.txt".to_string(),
+            size: 1024,
+            is_dir: false,
+            children: Vec::new(),
+            parent: None,
+            extension: Some("txt".to_string()),
+            modified: None,
+        };
+        let event = ScanEvent::NodeDiscovered {
+            node: node.clone(),
+            parent_index: Some(0),
+        };
+        if let ScanEvent::NodeDiscovered { node: n, parent_index } = event {
+            assert_eq\!(n.name, "test.txt");
+            assert_eq\!(parent_index, Some(0));
+        } else {
+            panic\!("Expected NodeDiscovered variant");
+        }
+    }
+
+    #[test]
+    fn scan_event_is_send_and_sync() {
+        fn assert_send<T: Send>() {}
+        fn assert_sync<T: Sync>() {}
+        assert_send::<ScanEvent>();
+        assert_sync::<ScanEvent>();
+    }
+
+    #[test]
+    fn scan_config_is_send_and_sync() {
+        fn assert_send<T: Send>() {}
+        fn assert_sync<T: Sync>() {}
+        assert_send::<ScanConfig>();
+        assert_sync::<ScanConfig>();
+    }
+}

```

**Documentation:**

```diff
--- a/crates/rds-core/src/scan.rs
+++ b/crates/rds-core/src/scan.rs
@@ -1,8 +1,22 @@
+//! Scan events, configuration, and summary statistics. (DL-001)
+//!
+//! `ScanEvent` is designed to cross a channel from the scanner thread to the
+//! GUI thread, so all variants must be `Send + Sync`. `ScanError` stores the
+//! error as `String` because `io::Error` does not implement `Serialize`;
+//! the `Send` bound is a secondary benefit. (DL-004)
 use std::path::PathBuf;
 use serde::{Serialize, Deserialize};
 use crate::tree::FileNode;

+/// Events emitted by the scanner during a filesystem walk.
+///
+/// Sent over a channel from the scanner thread to the GUI thread. All variants
+/// are `Send + Sync` because `ScanError::error` is a `String` rather than
+/// `io::Error`. (DL-004)
 #[derive(Clone, Debug)]
 pub enum ScanEvent {
+    /// A new filesystem entry was discovered.
+    ///
+    /// Carries the full `FileNode` and the parent's arena index so the receiver
+    /// can insert the node into its own arena without re-scanning. (DL-002)
     NodeDiscovered {
         node: FileNode,
+        /// Arena index of the parent in the receiver's `DirTree`, or `None` for the root.
         parent_index: Option<usize>,
     },
+    /// Periodic progress update during a scan.
     Progress {
         files_scanned: u64,
         bytes_scanned: u64,
     },
+    /// Two or more files share the same SHA-256 content hash. (DL-007)
     DuplicateFound {
+        /// SHA-256 digest identifying the duplicated content. (DL-007)
         hash: [u8; 32],
+        /// Arena indices of all nodes with this hash in the receiver's `DirTree`.
         node_indices: Vec<usize>,
     },
+    /// The scan finished successfully.
     ScanComplete {
         stats: ScanStats,
     },
+    /// A non-fatal error occurred while scanning `path`.
+    ///
+    /// The scan continues after emitting this variant. The error message is
+    /// stored as `String` because `io::Error` is not `Serialize`; this also
+    /// satisfies the `Send` bound required for channel transfer. (DL-004)
     ScanError {
+        /// The path that triggered the error.
         path: PathBuf,
+        /// Human-readable error description.
         error: String,
     },
 }

+/// Aggregated statistics for a completed scan.
 #[derive(Clone, Debug, Serialize, Deserialize)]
 pub struct ScanStats {
     pub total_files: u64,
     pub total_dirs: u64,
     pub total_bytes: u64,
+    /// Wall-clock duration of the scan in milliseconds.
     pub duration_ms: u64,
+    /// Number of `ScanError` events emitted during the scan.
     pub errors: u64,
 }

+/// Parameters controlling a filesystem scan.
+///
+/// `root` and any matched paths are represented as `PathBuf` for full OS path
+/// support. Platform-specific edge cases (non-UTF-8, Windows UNC) are handled
+/// by rds-scanner, not at this type level. (DL-011)
 #[derive(Clone, Debug, Serialize, Deserialize)]
 pub struct ScanConfig {
+    /// Directory to scan.
     pub root: PathBuf,
+    /// Follow symbolic links. Default: `false` to avoid cycles.
     pub follow_symlinks: bool,
+    /// Glob patterns for paths to skip during the scan.
     pub exclude_patterns: Vec<String>,
+    /// Hash file contents with SHA-256 to detect duplicates. (DL-007)
     pub hash_duplicates: bool,
+    /// Abort the scan after this many nodes. `None` means unlimited.
+    /// Default is ten million nodes (~2 GB RAM at ~200 bytes per node).
     pub max_nodes: Option<usize>,
 }

```


**CC-M-001-006** (crates/rds-core/src/config.rs) - implements CI-M-001-005

**Code:**

```diff
--- /dev/null
+++ b/crates/rds-core/src/config.rs
@@ -0,0 +1,72 @@
+use std::path::PathBuf;
+use serde::{Serialize, Deserialize};
+
+#[derive(Clone, Debug, Serialize, Deserialize)]
+pub struct CustomCommand {
+    pub name: String,
+    pub template: String,
+}
+
+impl Default for CustomCommand {
+    fn default() -> Self {
+        CustomCommand {
+            name: String::new(),
+            template: String::new(),
+        }
+    }
+}
+
+#[derive(Clone, Debug, Serialize, Deserialize)]
+pub struct AppConfig {
+    pub exclude_patterns: Vec<String>,
+    pub custom_commands: Vec<CustomCommand>,
+    pub color_scheme: String,
+    pub default_sort: String,
+    pub recent_paths: Vec<PathBuf>,
+}
+
+impl Default for AppConfig {
+    fn default() -> Self {
+        AppConfig {
+            exclude_patterns: Vec::new(),
+            custom_commands: Vec::new(),
+            color_scheme: "default".to_string(),
+            default_sort: "size_desc".to_string(),
+            recent_paths: Vec::new(),
+        }
+    }
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[test]
+    fn app_config_defaults() {
+        let config = AppConfig::default();
+        assert\!(config.exclude_patterns.is_empty());
+        assert\!(config.custom_commands.is_empty());
+        assert_eq\!(config.color_scheme, "default");
+        assert_eq\!(config.default_sort, "size_desc");
+        assert\!(config.recent_paths.is_empty());
+    }
+
+    #[test]
+    fn app_config_serde_roundtrip() {
+        let config = AppConfig {
+            exclude_patterns: vec\!["*.tmp".to_string(), "node_modules".to_string()],
+            custom_commands: vec\![
+                CustomCommand {
+                    name: "Open Terminal".to_string(),
+                    template: "cd {path} && bash".to_string(),
+                },
+            ],
+            color_scheme: "dark".to_string(),
+            default_sort: "name_asc".to_string(),
+            recent_paths: vec\![PathBuf::from("/home/user/docs")],
+        };
+        let json = serde_json::to_string(&config).unwrap();
+        let deserialized: AppConfig = serde_json::from_str(&json).unwrap();
+        assert_eq\!(deserialized.exclude_patterns, config.exclude_patterns);
+        assert_eq\!(deserialized.color_scheme, config.color_scheme);
+        assert_eq\!(deserialized.default_sort, config.default_sort);
+        assert_eq\!(deserialized.recent_paths, config.recent_paths);
+        assert_eq\!(deserialized.custom_commands.len(), 1);
+        assert_eq\!(deserialized.custom_commands[0].name, "Open Terminal");
+        assert_eq\!(deserialized.custom_commands[0].template, "cd {path} && bash");
+    }
+}

```

**Documentation:**

```diff
--- a/crates/rds-core/src/config.rs
+++ b/crates/rds-core/src/config.rs
@@ -1,6 +1,13 @@
+//! Application-level configuration loaded from TOML. (DL-001)
+//!
+//! `AppConfig` is serde-deserializable so it can be read directly from a
+//! TOML file without custom parsing code.
 use std::path::PathBuf;
 use serde::{Serialize, Deserialize};

+/// A user-defined shell command that operates on a selected path.
+///
+/// `template` supports `{path}` as a substitution token (e.g. `"cd {path} && bash"`).
 #[derive(Clone, Debug, Serialize, Deserialize)]
 pub struct CustomCommand {
+    /// Display name shown in the UI.
     pub name: String,
+    /// Shell command template. Use `{path}` where the selected path should be substituted.
     pub template: String,
 }

+/// Application configuration, deserialized from a TOML config file.
 #[derive(Clone, Debug, Serialize, Deserialize)]
 pub struct AppConfig {
+    /// Glob patterns applied to every scan unless overridden by `ScanConfig`.
     pub exclude_patterns: Vec<String>,
+    /// User-defined commands available in the context menu.
     pub custom_commands: Vec<CustomCommand>,
+    /// Color scheme name. Default: `"default"`.
     pub color_scheme: String,
+    /// Default sort order for directory listings. Default: `"size_desc"`.
     pub default_sort: String,
+    /// Paths opened in previous sessions, used to populate the recents list.
     pub recent_paths: Vec<PathBuf>,
 }

```


**CC-M-001-008** (crates/rds-core/src/stats.rs) - implements CI-M-001-006

**Code:**

```diff
--- /dev/null
+++ b/crates/rds-core/src/stats.rs
@@ -0,0 +1,131 @@
+use std::collections::HashMap;
+use serde::{Serialize, Deserialize};
+use crate::tree::DirTree;
+
+#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
+pub struct HslColor {
+    pub h: f64,
+    pub s: f64,
+    pub l: f64,
+}
+
+#[derive(Clone, Debug, Serialize, Deserialize)]
+pub struct ExtensionStats {
+    pub extension: String,
+    pub count: u64,
+    pub total_bytes: u64,
+    pub percentage: f64,
+    pub color: HslColor,
+}
+
+pub fn color_for_extension(ext: &str) -> HslColor {
+    let hue: u64 = ext.as_bytes().iter().map(|&b| b as u64).sum();
+    HslColor {
+        h: (hue % 360) as f64,
+        s: 0.7,
+        l: 0.5,
+    }
+}
+
+pub fn compute_extension_stats(tree: &DirTree) -> Vec<ExtensionStats> {
+    let mut groups: HashMap<String, (u64, u64)> = HashMap::new();
+    let mut total_file_bytes: u64 = 0;
+
+    for i in 0..tree.len() {
+        if let Some(node) = tree.get(i) {
+            if !node.is_dir {
+                let ext = node.extension.clone().unwrap_or_default();
+                let entry = groups.entry(ext).or_insert((0, 0));
+                entry.0 += 1;
+                entry.1 += node.size;
+                total_file_bytes += node.size;
+            }
+        }
+    }
+
+    let mut stats: Vec<ExtensionStats> = groups
+        .into_iter()
+        .map(|(ext, (count, total_bytes))| {
+            let percentage = if total_file_bytes > 0 {
+                (total_bytes as f64 / total_file_bytes as f64) * 100.0
+            } else {
+                0.0
+            };
+            let color = color_for_extension(&ext);
+            ExtensionStats {
+                extension: ext,
+                count,
+                total_bytes,
+                percentage,
+                color,
+            }
+        })
+        .collect();
+
+    stats.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));
+    stats
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+    use crate::tree::FileNode;
+
+    #[test]
+    fn color_determinism() {
+        let c1 = color_for_extension("rs");
+        let c2 = color_for_extension("rs");
+        assert_eq!(c1, c2);
+
+        let c3 = color_for_extension("rs");
+        assert_eq!(c1, c3);
+    }
+
+    #[test]
+    fn color_different_extensions() {
+        let c_rs = color_for_extension("rs");
+        let c_py = color_for_extension("py");
+        let c_js = color_for_extension("js");
+        assert_ne!(c_rs.h, c_py.h);
+        assert_ne!(c_rs.h, c_js.h);
+    }
+
+    #[test]
+    fn color_fixed_saturation_lightness() {
+        let c = color_for_extension("txt");
+        assert_eq!(c.s, 0.7);
+        assert_eq!(c.l, 0.5);
+    }
+
+    #[test]
+    fn compute_stats_on_small_tree() {
+        let mut tree = DirTree::new("/root");
+
+        let file_a = FileNode {
+            name: "a.rs".to_string(),
+            size: 1000,
+            is_dir: false,
+            children: Vec::new(),
+            parent: None,
+            extension: Some("rs".to_string()),
+            modified: None,
+        };
+        tree.insert(0, file_a);
+
+        let file_b = FileNode {
+            name: "b.rs".to_string(),
+            size: 500,
+            is_dir: false,
+            children: Vec::new(),
+            parent: None,
+            extension: Some("rs".to_string()),
+            modified: None,
+        };
+        tree.insert(0, file_b);
+
+        let file_c = FileNode {
+            name: "c.txt".to_string(),
+            size: 500,
+            is_dir: false,
+            children: Vec::new(),
+            parent: None,
+            extension: Some("txt".to_string()),
+            modified: None,
+        };
+        tree.insert(0, file_c);
+
+        let stats = compute_extension_stats(&tree);
+        assert_eq!(stats.len(), 2);
+        assert_eq!(stats[0].extension, "rs");
+        assert_eq!(stats[0].count, 2);
+        assert_eq!(stats[0].total_bytes, 1500);
+        assert_eq!(stats[0].percentage, 75.0);
+        assert_eq!(stats[1].extension, "txt");
+        assert_eq!(stats[1].count, 1);
+        assert_eq!(stats[1].total_bytes, 500);
+        assert_eq!(stats[1].percentage, 25.0);
+    }
+}

```

**Documentation:**

```diff
--- a/crates/rds-core/src/stats.rs
+++ b/crates/rds-core/src/stats.rs
@@ -1,6 +1,14 @@
+//! Per-extension aggregation and deterministic color assignment.
+//!
+//! Colors are computed by hashing the extension string to a hue value in
+//! `0..360`. The same extension always produces the same color across runs
+//! because the algorithm is a simple byte-sum, not `DefaultHasher` (which
+//! uses randomly seeded SipHash). (DL-003, DL-010)
 use std::collections::HashMap;
 use serde::{Serialize, Deserialize};
 use crate::tree::DirTree;

+/// An HSL color with components in the ranges `h ∈ [0, 360)`, `s ∈ [0, 1]`, `l ∈ [0, 1]`.
 #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
 pub struct HslColor {
     pub h: f64,
     pub s: f64,
     pub l: f64,
 }

+/// Aggregated size and count for all files sharing a file extension.
 #[derive(Clone, Debug, Serialize, Deserialize)]
 pub struct ExtensionStats {
+    /// Extension string (without leading dot), or empty string for files with no extension.
     pub extension: String,
+    /// Number of files with this extension.
     pub count: u64,
+    /// Total size of all files with this extension in bytes.
     pub total_bytes: u64,
+    /// Fraction of total file bytes represented by this extension, in percent (`0.0..=100.0`).
     pub percentage: f64,
+    /// Deterministic display color for this extension. (DL-003)
     pub color: HslColor,
 }

+/// Returns a deterministic `HslColor` for `ext`.
+///
+/// Hue is computed as `(sum of ext bytes) % 360`. Saturation is fixed at
+/// `0.7` and lightness at `0.5` to produce visually distinct, readable colors.
+/// Uses byte-sum instead of `DefaultHasher` because `DefaultHasher` is
+/// randomly seeded per process. (DL-010)
 pub fn color_for_extension(ext: &str) -> HslColor {
@@ -32,6 +48,12 @@ pub fn color_for_extension(ext: &str) -> HslColor {
 }

+/// Aggregates `ExtensionStats` for every file extension present in `tree`.
+///
+/// Iterates all non-directory nodes, groups by extension (empty string for
+/// extensionless files), and computes byte percentage relative to total file
+/// bytes. Returns entries sorted by `total_bytes` descending. Directories are
+/// excluded from both counts and totals.
 pub fn compute_extension_stats(tree: &DirTree) -> Vec<ExtensionStats> {

```

