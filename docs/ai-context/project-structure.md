# Project Structure

```
rustdirstat/
  Cargo.toml                  # Workspace root; all dep versions in [workspace.dependencies]
  Cargo.lock                  # Committed (binary crate — reproducible builds)
  src/
    main.rs                   # CLI parsing (clap), --scan-only headless mode, tracing init with RUST_LOG env filter, config load/save (TOML via directories), eframe window launch (1024x768 default, 640x480 minimum)
  crates/
    rds-core/                 # Shared data types — ZERO deps beyond serde
      Cargo.toml
      CLAUDE.md               # Crate-level AI context
      README.md               # Arena invariants and index stability contract
      src/
        lib.rs                # Re-exports all public types from submodules
        tree.rs               # FileNode (with deleted flag) + DirTree arena (Vec<FileNode> with usize indices, tombstone(), new_with_capacity/from_root_with_capacity)
        scan.rs               # ScanEvent enum, ScanConfig, ScanStats
        config.rs             # AppConfig + CustomCommand + ColorScheme enum (Default/Dark/Light) (TOML-deserializable)
        stats.rs              # ExtensionStats, HslColor, color_for_extension(), compute_extension_stats() (filters deleted nodes)
      benches/
        tree_bench.rs         # Criterion benchmarks: insert_nodes, subtree_size, compute_extension_stats at 1k-1M scale
    rds-scanner/              # Parallel jwalk scanner + SHA-256 duplicate detection + glob-based exclude filtering
      Cargo.toml              # Deps: jwalk, crossbeam-channel, sha2, rayon, glob, tracing, rds-core
      CLAUDE.md
      README.md               # Ordering invariant, abort design, skip_hidden parity
      src/
        lib.rs                # Module declarations and re-exports
        scanner.rs            # Scanner::scan() — jwalk traversal, cancel flag, max_nodes, exclude pattern filtering via glob::Pattern, event streaming, structured tracing spans (scan/walk)
        duplicate.rs          # DuplicateDetector — 3-phase pipeline (size grouping, partial hash, full SHA-256), tracing span
      tests/
        scan_integration.rs   # Integration tests with temp directory fixtures
        exclude_integration.rs # Exclude pattern filtering integration tests
        duplicate_integration.rs # Duplicate detection integration tests
    rds-gui/                  # egui/eframe GUI: dir picker, tree view, ext stats, treemap (with aggregation), duplicates, actions, export, settings, config persistence, error handling
      Cargo.toml              # Deps: eframe, egui, egui-notify, streemap, crossbeam-channel, rds-core, rds-scanner, rfd, trash, open, serde, serde_json, csv; bench-internals feature
      CLAUDE.md
      src/
        lib.rs                # RustDirStatApp: ScanPhase state machine, dir picker, scanner integration, event drain (DRAIN_BATCH_SIZE=5000), tree_capacity_hint pre-allocation, 3-panel layout, keyboard shortcuts (Ctrl+O, Escape, F5, Backspace), theme application via ThemePreference, config persistence, recent paths, PendingDelete, confirm_delete, toast notifications, scan error log panel, max-nodes abort dialog; bench-internals re-exports
        notifications.rs       # Notifications wrapper around egui_notify::Toasts — info/warning/error toasts with auto-dismiss
        error_log.rs           # ScanErrorLog (capped Vec of path+error pairs, overflow counter), error log panel rendering
        tree_view.rs           # SubtreeStats cache (filters deleted), TreeViewState, sorted tree rendering with context menu, empty dir "(empty)" hint
        ext_stats.rs           # hsl_to_color32, extension stats panel with stacked bar + Grid table
        treemap.rs             # CushionCoeffs, TreemapRect (aggregated_count), TreemapLayout, MAX_DISPLAY_RECTS (50k cap), recursive squarify with aggregation, cushion mesh render, right-click context menu
        duplicates.rs          # Duplicates bottom panel with collapsible groups, wasted space, context menu
        actions.rs             # execute_delete (trash + tombstone), cleanup_duplicate_groups, open_in_file_manager (D-Bus FileManager1.ShowItems on Linux, Explorer /select on Windows, open -R on macOS), execute_custom_command
        command_editor.rs      # Command editor window: inline editing of custom commands, add/remove controls
        export.rs              # CSV/JSON export: ExportFormat/ExportScope enums, export_tree (DFS + serialize), export_duplicates, export dialog UI
        settings.rs            # Settings dialog: SettingsDialogState, exclude patterns editor, sort order/color scheme ComboBox, Apply/Cancel
      benches/
        treemap_bench.rs      # Criterion benchmarks: treemap layout, subtree stats, aggregation at 1k-500k scale
  .github/
    workflows/
      ci.yml                  # Build + test + clippy + fmt on ubuntu/macos/windows
                              # Enforces rds-core zero-dep invariant via cargo tree
  scripts/
    benchmark-comparison.sh   # hyperfine-based comparison of rustdirstat --scan-only vs dust vs dua
  docs/
    milestones.md             # MS1-MS21 roadmap with dependency graph
    benchmarks.md             # Memory usage audit, struct sizes, scan throughput, treemap rendering budget
    ai-context/               # AI-optimized project documentation
      project-structure.md    # This file
      docs-overview.md        # Documentation registry and tier classification
    superpowers/
      specs/
        2026-03-19-rustdirstat-design.md  # Full design specification
  plans/
    ms1-workspace-scaffold-ci.md  # MS1 plan (completed)
    ms2-core-data-types.md        # MS2 plan (completed)
    ms3-single-threaded-scanner.md # MS3 plan (completed)
    ms4-parallel-scanner-jwalk.md  # MS4 plan (completed)
    ms5-gui-shell-directory-picker.md # MS5 plan (completed)
    ms6-directory-tree-view.md     # MS6 plan (completed)
    ms7-extension-statistics-panel.md # MS7 plan (completed)
    ms8-treemap-layout-flat.md     # MS8 plan (completed)
    ms9-treemap-cushion-shading.md # MS9 plan (completed)
    ms10-panel-synchronization.md  # MS10 plan (completed)
    ms11-incremental-scan-display.md # MS11 plan (completed)
    ms12-duplicate-detection.md      # MS12 plan (completed)
    ms13-delete-action-trash.md      # MS13 plan (completed)
  justfile                    # Task runner: build, test, lint, fmt, run, check, clean, bench, bench-report, bench-compare
  .gitignore                  # Ignores /target; Cargo.lock NOT ignored
  CLAUDE.md                   # Root AI context with file/directory guide
  README.md                   # Architecture, design decisions, invariants
```

## Key Architectural Patterns

### Arena-Allocated Tree (rds-core/src/tree.rs)
- `DirTree` = `Vec<FileNode>` with `usize` index references (not Rc/Box)
- Insert-only: nodes never removed, indices stable for lifetime of tree
- Delete via `tombstone()`: sets `deleted=true`, `size=0`, removes from parent's children; node remains in Vec
- All stats/rendering code filters deleted nodes (`SubtreeStats`, `ExtensionStats`, `sorted_children`)
- Cache-local traversal, zero reference-counting overhead
- Root always at index 0

### Scanner-GUI Channel Protocol (rds-core/src/scan.rs)
- Bounded `crossbeam-channel` carries `ScanEvent` variants from scanner thread to GUI
- GUI drains with `try_recv()` per frame (never blocks)
- `NodeDiscovered` carries full `FileNode` + `parent_index` (GUI-side arena index)
- `ScanError` carries path + error string — GUI accumulates in `ScanErrorLog` (capped at 1000 entries)
- Scanner thread runs duplicate detection before sending `ScanComplete`

### Error Handling & User Feedback (crates/rds-gui/src/)
- Toast notifications (`egui-notify`) for transient action feedback — errors from file manager, custom commands, export
- Scan error log panel shows accumulated error details (path + message) after scan completes
- Max-nodes abort triggers a dedicated dialog with suggestion to scan a subdirectory
- Structured tracing spans in scanner (`scan`, `walk`, `duplicate_detection`) with `RUST_LOG` env filter support
- DirTree `assert!()` guards are intentional invariants for scanner logic bugs — documented, not removed

### Configuration Persistence (src/main.rs + rds-core/src/config.rs)
- `AppConfig` defined in rds-core (serde-deserializable, `#[serde(default)]` for forward compatibility)
- File I/O owned by `main.rs` via `directories` (platform config dir) + `toml` crate
- Save callback (`Box<dyn Fn(&AppConfig) + Send>`) passed to `RustDirStatApp` to avoid coupling rds-gui to filesystem crates
- Auto-saves on settings dialog Apply, command editor changes, recent path tracking, and app exit (`on_exit`)
- Missing or corrupt config gracefully defaults (log warning, use `AppConfig::default()`)

### Performance & Treemap Aggregation (crates/rds-gui/src/treemap.rs)
- `MAX_DISPLAY_RECTS` (50,000) caps rendered rectangles; excess items merged into "other" buckets per directory
- Aggregated rects use `node_index = usize::MAX` sentinel, neutral gray color, `aggregated_count: Option<(u64, u64)>` for item count + bytes
- `DirTree::new_with_capacity`/`from_root_with_capacity` pre-allocate arena; capacity capped at 100k via `.min(100_000)`
- Scanner pre-allocates `HashMap` and `Vec` with same 100k cap
- `DRAIN_BATCH_SIZE = 5000` events per frame (bounded channel is 4096 slots)
- `--scan-only` CLI flag runs headless scan for benchmarking
- Criterion benchmarks in `crates/rds-core/benches/` and `crates/rds-gui/benches/` (gui requires `bench-internals` feature)

### Dependency Management (Cargo.toml)
- All versions pinned once in `[workspace.dependencies]`
- Crates opt in with `{ workspace = true }`
- rds-core: zero deps beyond serde (CI-enforced invariant)
- Edition 2024, Rust 1.85+ required

## Milestone Status

| MS | Name | Status |
|----|------|--------|
| 1 | Workspace Scaffold & CI | Done |
| 2 | Core Data Types | Done |
| 3 | Single-Threaded Scanner | Done |
| 4 | Parallel Scanner (jwalk) | Done |
| 5 | GUI Shell & Directory Picker | Done |
| 6 | Directory Tree View | Done |
| 7 | Extension Statistics Panel | Done |
| 8 | Treemap Layout (Flat) | Done |
| 9 | Treemap Cushion Shading | Done |
| 10 | Panel Synchronization | Done |
| 11 | Incremental Scan Display | Done |
| 12 | Duplicate Detection | Done |
| 13 | Delete Action (Trash) | Done |
| 14 | Open in File Manager | Done |
| 15 | Custom Commands | Done |
| 16 | CSV/JSON Export | Done |
| 17 | Configuration & Persistence | Done |
| 18 | Error Handling & Edge Cases | Done |
| 19 | Performance Optimization | Done |
| 20 | Cross-Platform Polish | Done |
| 21 | See docs/milestones.md | Pending |
