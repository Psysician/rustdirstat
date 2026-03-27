# crates/rds-core/src/

Core data types shared across all rds crates. Four submodules by concern.

## Files

| File | What | When to read |
| ---- | ---- | ------------ |
| `lib.rs` | Module declarations and public re-exports | Adding new submodules, modifying public API surface |
| `tree.rs` | `FileNode` (40 bytes, flags bitfield, u32 indices, first-child/next-sibling linked list) + `DirTree` arena (`Vec<FileNode>` + `name_buffer: Vec<u8>` string arena + `extensions: Vec<Box<str>>` intern table, `ChildIter` iterator, `tombstone()` for safe deletion) | Modifying tree structure, adding traversal methods, understanding index invariants, implementing delete operations |
| `scan.rs` | `ScanEvent` enum (`NodeDiscovered` carries `FileNode` + `node_name` + `extension_name` separately), `ScanStats`, `ScanConfig` | Modifying scan events, changing scan configuration defaults, adding new event variants |
| `config.rs` | `AppConfig` + `CustomCommand` + `ColorScheme` enum (`Default` system auto, `Dark`, `Light`) (TOML-deserializable) | Modifying app configuration, adding new settings fields |
| `stats.rs` | `ExtensionStats`, `HslColor`, `color_for_extension()`, `compute_extension_stats()` (filters deleted nodes) | Modifying per-extension aggregation, changing color assignment logic |
