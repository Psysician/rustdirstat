# crates/rds-gui/

egui/eframe GUI shell with directory picker, scanner integration, and 3-panel layout.

## Files

| File | What | When to read |
| ---- | ---- | ------------ |
| `Cargo.toml` | Crate manifest; depends on `eframe`, `egui`, `streemap`, `crossbeam-channel`, `tracing`, `rds-core`, `rds-scanner`, `rfd` | Modifying GUI dependencies |
| `src/lib.rs` | `RustDirStatApp` with ScanPhase state machine, directory picker (rfd), scanner spawning, event drain loop, 3-panel layout with placeholders | Modifying app state, scan lifecycle, layout, adding panel implementations |
