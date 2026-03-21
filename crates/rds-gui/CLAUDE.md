# crates/rds-gui/

egui/eframe GUI shell with directory picker, scanner integration, directory tree view, extension statistics, treemap renderer, and panel layout.

## Files

| File | What | When to read |
| ---- | ---- | ------------ |
| `Cargo.toml` | Crate manifest; depends on `eframe`, `egui`, `streemap`, `crossbeam-channel`, `tracing`, `rds-core`, `rds-scanner`, `rfd` | Modifying GUI dependencies |
| `src/lib.rs` | `RustDirStatApp` with ScanPhase state machine, directory picker (rfd), scanner spawning, event drain loop, 3-panel layout with tree view + treemap + ext stats, format_bytes utility | Modifying app state, scan lifecycle, layout, adding panel implementations |
| `src/tree_view.rs` | `SubtreeStats` (cached subtree sizes/file counts), `TreeViewState` (expanded nodes), `sorted_children`, tree view rendering (show/render_node) | Modifying tree view display, adding tree interactions, understanding size caching strategy |
| `src/ext_stats.rs` | `hsl_to_color32` (HSL→Color32 conversion), `show` (extension stats panel with stacked bar chart and scrollable Grid table) | Modifying extension statistics display, adding bar chart interactions, reusing hsl_to_color32 for treemap coloring |
| `src/treemap.rs` | `CushionCoeffs` (parabolic ridge accumulation, Lambertian intensity), `TreemapRect`, `TreemapLayout` (cached layout with cushion coefficients), `compute_recursive` (recursive squarify with depth/cushion tracking), `shade_color`, `grid_subdivisions`, `build_cushion_mesh`, `show` (cushion mesh render + flat fallback + tooltip + click selection) | Modifying treemap rendering, tuning cushion constants, adding drill-down navigation (MS10) |
