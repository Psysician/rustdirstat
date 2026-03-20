# rds-core

Shared data types for all crates in the rustdirstat workspace.

## Invariants

- **Zero dependencies beyond `serde`**: rds-core must not depend on anything
  beyond `std` and `serde`. This keeps core types fast to compile and
  independently testable. Enforced by CI via `cargo tree` check.

- **Arena indices are stable**: `DirTree` is append-only. Nodes are pushed onto
  the internal `Vec<FileNode>` and never removed. An index returned by
  `DirTree::insert` is valid for the lifetime of the tree. Delete operations
  MUST tombstone nodes (zero their `size`, mark as deleted) rather than remove
  them from the `Vec`; removing any node invalidates all existing indices
  across all crates.

- **Root is always index 0**: `DirTree::new` places the root `FileNode` at
  index 0. `DirTree::root()` returns `0`. The tree is never empty after
  construction.

- **`color_for_extension` is a pure function**: Same input always produces the
  same `HslColor` output across runs, platforms, and Rust versions. Implemented
  via byte-sum modulo 360. `std::collections::hash_map::DefaultHasher` is
  prohibited because it is randomly seeded per process since Rust 1.36.

- **`ScanConfig::default().max_nodes` is `Some(10_000_000)`**: The scanner
  aborts with `ScanEvent::ScanError` when the node count reaches this limit to
  prevent arena memory overflow (~2 GB at ~200 bytes per node).

- **All `ScanEvent` variants are `Send + Sync`**: `ScanEvent` crosses a
  crossbeam channel from the scanner thread to the GUI thread. Compiler-verified
  by static assertions in `scan.rs` tests.

## Design Decisions

**Arena allocation over `Rc`/`Box` tree**: `DirTree` stores all nodes in a flat
`Vec<FileNode>`. Parent and child relationships are `usize` indices into that
vec. This gives cache-local traversal, zero reference-counting overhead, and
index values that are `Copy` with no borrow lifetime constraints — enabling safe
cross-thread use via `ScanEvent::NodeDiscovered`.

**Raw `usize` indices, not a newtype**: `DirTree` methods and `ScanEvent`
variants expose `usize` directly. A `NodeIndex(usize)` newtype would add type
safety but increases boilerplate and was not specified. If index-misuse bugs
emerge in later milestones, wrapping is backward-compatible.

**`ScanError.error` is `String`, not `io::Error`**: `std::io::Error` does not
implement `serde::Serialize`, making it incompatible with `#[derive(Serialize)]`
on `ScanEvent`. Converting to `String` at error creation preserves the message
and enables serde on the enum.

**`ScanConfig.root` and `ScanError.path` use `PathBuf`**: `PathBuf` is the
idiomatic Rust owned path type and handles non-UTF-8 paths on Unix and long
paths on Windows. Platform-specific edge cases (UNC paths, MAX_PATH, non-UTF-8)
are an rds-scanner (MS3/MS4) and polish (MS18/MS20) concern, not a type-layer
concern.

**`FileNode` fields are `pub`**: rds-gui reads `name`, `size`, `extension`, and
`children` directly for tree rendering and treemap layout. `pub(crate)` would
require accessor boilerplate for every field across the crate boundary.

**Four submodules**: `tree`, `scan`, `config`, `stats` each own a distinct type
group. `src/lib.rs` re-exports all public types so callers use `rds_core::FileNode`
rather than `rds_core::tree::FileNode`.
