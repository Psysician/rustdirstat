# crates/rds-gui/

egui/eframe immediate-mode GUI shell and treemap rendering crate.

## Files

| File | What | When to read |
| ---- | ---- | ------------ |
| `Cargo.toml` | Crate manifest; depends on `eframe`, `egui`, `streemap`, `crossbeam-channel`, `tracing`, `rds-core` | Modifying GUI dependencies, understanding `streemap` upfront declaration |
| `src/lib.rs` | `RustDirStatApp` struct implementing `eframe::App`; empty central panel bootstrap | Implementing UI panels, modifying the main app struct |
