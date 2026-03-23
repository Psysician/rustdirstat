# crates/rds-core/

Shared data types for all crates. Zero external dependencies beyond `serde`.

## Files

| File | What | When to read |
| ---- | ---- | ------------ |
| `Cargo.toml` | Crate manifest; `serde` runtime dep, `serde_json` + `criterion` dev-deps only | Verifying zero-dep invariant, adding dependencies (must not) |
| `src/lib.rs` | Library root; re-exports all public types from the four submodules | Adding core types, understanding module structure |
| `src/tree.rs` | `FileNode` (with `deleted` flag) + `DirTree` (arena-allocated `Vec<FileNode>` with `usize` indices, `tombstone()` for safe deletion, `new_with_capacity`/`from_root_with_capacity` constructors for pre-allocation) | Modifying tree structure, adding traversal methods, understanding index invariants, implementing delete operations |
| `src/scan.rs` | `ScanEvent` enum, `ScanStats`, `ScanConfig`; scanner-to-GUI channel types | Modifying scan events, changing scan configuration defaults |
| `src/config.rs` | `AppConfig` + `CustomCommand`; TOML-deserializable application settings | Modifying app configuration, adding new settings fields |
| `src/stats.rs` | `ExtensionStats`, `HslColor`, `color_for_extension`, `compute_extension_stats` (filters deleted nodes) | Modifying per-extension aggregation or color assignment logic |
| `benches/tree_bench.rs` | Criterion benchmarks for tree insert, subtree_size, and compute_extension_stats at 1k-1M scale | Modifying tree benchmarks, understanding performance characteristics |
| `README.md` | Invariants, arena design rationale, index stability contract | Understanding why the tree is append-only, why indices must never be removed |
