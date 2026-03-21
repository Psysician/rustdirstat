# crates/rds-gui/

egui/eframe GUI shell with directory picker, scanner integration, directory tree view, extension statistics, cushion-shaded treemap renderer, panel synchronization, and drill-down navigation.

## Files

| File | What | When to read |
| ---- | ---- | ------------ |
| `Cargo.toml` | Crate manifest; depends on `eframe`, `egui`, `streemap`, `crossbeam-channel`, `tracing`, `rds-core`, `rds-scanner`, `rfd` | Modifying GUI dependencies |
| `src/lib.rs` | `RustDirStatApp` with ScanPhase state machine, directory picker (rfd), scanner spawning, event drain loop, 3-panel layout with tree view + treemap + ext stats, breadcrumb navigation, format_bytes utility | Modifying app state, scan lifecycle, layout, adding panel implementations |
| `src/tree_view.rs` | `SubtreeStats` (cached subtree sizes/file counts), `TreeViewState` (expanded nodes, selection sync, scroll-to), `expand_ancestors`, `sorted_children`, tree view rendering (show/render_node) | Modifying tree view display, adding tree interactions, understanding size caching strategy, understanding auto-expand/scroll behavior |
| `src/ext_stats.rs` | `hsl_to_color32` (HSL→Color32 conversion), `show` (extension stats panel with stacked bar chart, scrollable Grid table, click-to-select extension filter) | Modifying extension statistics display, adding bar chart interactions, reusing hsl_to_color32 for treemap coloring |
| `src/treemap.rs` | `CushionCoeffs` (parabolic ridge accumulation, Lambertian intensity), `TreemapRect`, `TreemapLayout` (cached layout with root tracking and cushion coefficients), `compute_recursive` (recursive squarify with depth/cushion tracking), `find_drill_target` (double-click navigation), `breadcrumb_chain` (ancestor path), `shade_color`, `grid_subdivisions`, `build_cushion_mesh`, `show` (cushion mesh render + flat fallback + extension dimming + tooltip + click selection + double-click drill-down) | Modifying treemap rendering, tuning cushion constants, understanding drill-down navigation |
