# crates/rds-core/

Shared data types for all crates. Zero external dependencies beyond `serde`.

## Files

| File | What | When to read |
| ---- | ---- | ------------ |
| `Cargo.toml` | Crate manifest; `serde` runtime dep, `serde_json` + `criterion` dev-deps only | Verifying zero-dep invariant, adding dependencies (must not) |
| `src/lib.rs` | Library root; re-exports all public types from the four submodules | Adding core types, understanding module structure |
| `src/tree.rs` | `FileNode` (40 bytes: string arena name, u32 indices, first-child/next-sibling linked list, u16 interned extension, flags bitfield) + `DirTree` (arena `Vec<FileNode>` + `name_buffer: Vec<u8>` + `extensions: Vec<Box<str>>`, `ChildIter` iterator, `tombstone()`, `shrink_to_fit()`, `name()`/`extension_str()` accessors) | Modifying tree structure, adding traversal methods, understanding index invariants, implementing delete operations |
| `src/scan.rs` | `ScanEvent` enum (`NodeDiscovered` carries `FileNode` + `node_name: Box<str>` + `extension_name: Option<Box<str>>`), `ScanStats`, `ScanConfig` | Modifying scan events, changing scan configuration defaults |
| `src/config.rs` | `AppConfig` + `CustomCommand` + `ColorScheme` enum (`Default` system auto, `Dark`, `Light`); TOML-deserializable application settings | Modifying app configuration, adding new settings fields |
| `src/stats.rs` | `ExtensionStats`, `HslColor`, `color_for_extension`, `compute_extension_stats` (filters deleted nodes) | Modifying per-extension aggregation or color assignment logic |
| `benches/tree_bench.rs` | Criterion benchmarks for tree insert, subtree_size, and compute_extension_stats at 1k-1M scale | Modifying tree benchmarks, understanding performance characteristics |
| `README.md` | Invariants, arena design rationale, index stability contract | Understanding why the tree is append-only, why indices must never be removed |
