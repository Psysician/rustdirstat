# crates/rds-core/

Shared data types for all crates.

## Files

| File | What | When to read |
| ---- | ---- | ------------ |
| `Cargo.toml` | Crate manifest; only `serde` dependency | Verifying zero-dep invariant, adding dependencies (must not) |
| `src/lib.rs` | Library root; arena-allocated `Vec<FileNode>` tree design documented | Adding core types, understanding tree traversal model |
