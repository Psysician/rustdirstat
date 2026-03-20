# crates/rds-scanner/

Parallel filesystem traversal and SHA-2 duplicate detection.

## Files

| File | What | When to read |
| ---- | ---- | ------------ |
| `Cargo.toml` | Crate manifest; depends on `jwalk`, `rayon`, `sha2`, `crossbeam-channel`, `tracing`, `rds-core` | Modifying scanner dependencies |
| `src/lib.rs` | Library root; bounded `crossbeam-channel` event streaming to GUI documented | Implementing scan logic, modifying scanner-GUI event protocol |
