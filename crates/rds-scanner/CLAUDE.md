# crates/rds-scanner/

Parallel filesystem traversal via `jwalk` and duplicate file detection via `sha2`/`rayon`; emits `ScanEvent` stream over bounded crossbeam-channel.

## Files

| File | What | When to read |
| ---- | ---- | ------------ |
| `Cargo.toml` | Crate manifest; depends on `jwalk`, `crossbeam-channel`, `tracing`, `sha2`, `rayon`, `rds-core` | Modifying scanner dependencies |
| `src/lib.rs` | Library root; module declaration and public re-exports | Implementing scan logic, modifying public API |
| `src/scanner.rs` | `Scanner` struct, `scan()` entry point, walk loop with exclude pattern filtering via `glob::Pattern` in `process_read_dir`, `FileEntry` collection, helper functions | Implementing traversal, modifying event emission, modifying exclude patterns, debugging scan behaviour |
| `src/duplicate.rs` | `DuplicateDetector` 3-phase pipeline (size grouping, partial 4KB SHA-256, full SHA-256 via rayon) | Modifying duplicate detection logic, understanding hashing pipeline |
| `tests/scan_integration.rs` | Integration tests; real filesystem fixtures via `tempfile` | Adding scan tests, verifying DirTree correctness, debugging event ordering |
| `tests/duplicate_integration.rs` | Integration tests for `DuplicateDetector` with real tempfile fixtures (7 scenarios) | Adding duplicate detection tests, debugging detection pipeline |
| `README.md` | Ordering invariant, two-level abort design, max_nodes rationale, skip_hidden parity, follow_links difference, error sources | Understanding why traversal is structured this way, debugging abort behaviour |
