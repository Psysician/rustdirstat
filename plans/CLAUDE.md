# plans/

Milestone implementation plans with decision logs.

## Files

| File | What | When to read |
| ---- | ---- | ------------ |
| `ms1-workspace-scaffold-ci.md` | MS1 plan: workspace setup, CI, justfile (completed) | Reviewing scaffold decisions, understanding CI setup rationale |
| `ms2-core-data-types.md` | MS2 plan: core types, arena tree, scan events (completed) | Reviewing type design decisions, understanding arena invariants |
| `ms3-single-threaded-scanner.md` | MS3 plan: walkdir scanner, event streaming, integration tests (completed) | Reviewing scanner design decisions, understanding event protocol |
| `ms4-parallel-scanner-jwalk.md` | MS4 plan: jwalk parallel scanner, cancel/max_nodes in process_read_dir (completed) | Reviewing jwalk migration decisions, understanding abort mechanism, skip_hidden parity, error mapping |
| `ms5-gui-shell-directory-picker.md` | MS5 plan: GUI shell, rfd directory picker, scanner integration, progress bar (completed) | Reviewing GUI architecture decisions, understanding scan lifecycle |
| `ms6-directory-tree-view.md` | MS6 plan: interactive tree view with expand/collapse, size-sorted children, cached SubtreeStats, selection state (completed) | Reviewing tree view architecture, understanding SubtreeStats caching, understanding selection state design |
| `ms7-extension-statistics-panel.md` | MS7 plan: extension stats panel with HSL→Color32, stacked bar chart, sorted Grid table (completed) | Reviewing color conversion approach, understanding extension stats rendering, understanding panel wiring pattern |
| `ms8-treemap-layout-flat.md` | MS8 plan: flat treemap layout with recursive squarify, cached layout, click/hover interaction (completed) | Reviewing treemap layout approach, understanding caching strategy, understanding rendering pipeline |
| `ms9-treemap-cushion-shading.md` | MS9 plan: hierarchical cushion shading with Lambertian lighting, per-vertex mesh, adaptive grid, 50k+ performance (completed) | Reviewing cushion algorithm, understanding shading constants, understanding mesh tessellation approach |
| `ms10-panel-synchronization.md` | MS10 plan: cross-panel selection sync, extension filter, treemap drill-down with breadcrumb, auto-expand ancestors (completed) | Reviewing panel synchronization design, understanding selection state model, understanding drill-down navigation |
| `ms11-incremental-scan-display.md` | MS11 plan: live tree/treemap updates during scan, throttled recompute, progress bar with scan rate (completed) | Reviewing incremental display design, understanding live recompute throttling |
| `ms12-duplicate-detection.md` | MS12 plan: 3-phase duplicate detection (size grouping, partial hash, full SHA-256), DuplicateDetector, duplicates bottom panel (completed) | Reviewing duplicate detection pipeline, understanding hashing phases, understanding GUI panel wiring |
| `ms13-delete-action-trash.md` | MS13 plan: delete via trash crate, arena tombstone, context menus, confirmation dialog, freed space tracking (completed) | Reviewing tombstone design, understanding delete action flow, understanding context menu wiring |
