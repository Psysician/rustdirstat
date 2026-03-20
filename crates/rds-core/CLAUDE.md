# crates/rds-core/

Shared data types for all crates. Zero external dependencies beyond `serde`.

## Files

| File | What | When to read |
| ---- | ---- | ------------ |
| `Cargo.toml` | Crate manifest; `serde` runtime dep, `serde_json` dev-dep only | Verifying zero-dep invariant, adding dependencies (must not) |
| `src/lib.rs` | Library root; re-exports all public types from the four submodules | Adding core types, understanding module structure |
| `src/tree.rs` | `FileNode` + `DirTree`; arena-allocated `Vec<FileNode>` with `usize` indices | Modifying tree structure, adding traversal methods, understanding index invariants |
| `src/scan.rs` | `ScanEvent` enum, `ScanStats`, `ScanConfig`; scanner-to-GUI channel types | Modifying scan events, changing scan configuration defaults |
| `src/config.rs` | `AppConfig` + `CustomCommand`; TOML-deserializable application settings | Modifying app configuration, adding new settings fields |
| `src/stats.rs` | `ExtensionStats`, `HslColor`, `color_for_extension`, `compute_extension_stats` | Modifying per-extension aggregation or color assignment logic |
| `README.md` | Invariants, arena design rationale, index stability contract | Understanding why the tree is append-only, why indices must never be removed |
