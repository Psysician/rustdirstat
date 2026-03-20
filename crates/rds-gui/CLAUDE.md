# crates/rds-gui/

egui/eframe GUI shell with directory picker, scanner integration, directory tree view, and panel layout.

## Files

| File | What | When to read |
| ---- | ---- | ------------ |
| `Cargo.toml` | Crate manifest; depends on `eframe`, `egui`, `streemap`, `crossbeam-channel`, `tracing`, `rds-core`, `rds-scanner`, `rfd` | Modifying GUI dependencies |
| `src/lib.rs` | `RustDirStatApp` with ScanPhase state machine, directory picker (rfd), scanner spawning, event drain loop, 3-panel layout with tree view, format_bytes utility | Modifying app state, scan lifecycle, layout, adding panel implementations |
| `src/tree_view.rs` | `SubtreeStats` (cached subtree sizes/file counts), `TreeViewState` (expanded nodes), `sorted_children`, tree view rendering (show/render_node) | Modifying tree view display, adding tree interactions, understanding size caching strategy |
