# crates/rds-scanner/

Single-threaded filesystem traversal via `walkdir`; emits `ScanEvent` stream over bounded crossbeam-channel.

## Files

| File | What | When to read |
| ---- | ---- | ------------ |
| `Cargo.toml` | Crate manifest; depends on `walkdir`, `crossbeam-channel`, `tracing`, `rds-core` | Modifying scanner dependencies |
| `src/lib.rs` | Library root; module declaration and public re-exports | Implementing scan logic, modifying public API |
| `src/scanner.rs` | `Scanner` struct, `scan()` entry point, walk loop, helper functions | Implementing traversal, modifying event emission, debugging scan behaviour |
| `tests/scan_integration.rs` | Integration tests; real filesystem fixtures via `tempfile` | Adding scan tests, verifying DirTree correctness, debugging event ordering |
