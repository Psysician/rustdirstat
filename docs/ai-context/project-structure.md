# Project Structure

```
rustdirstat/
  Cargo.toml                  # Workspace root; all dep versions in [workspace.dependencies]
  Cargo.lock                  # Committed (binary crate — reproducible builds)
  src/
    main.rs                   # CLI parsing (clap), tracing init, config load/save (TOML via directories), eframe window launch
  crates/
    rds-core/                 # Shared data types — ZERO deps beyond serde
      Cargo.toml
      CLAUDE.md               # Crate-level AI context
      README.md               # Arena invariants and index stability contract
      src/
        lib.rs                # Re-exports all public types from submodules
        tree.rs               # FileNode (with deleted flag) + DirTree arena (Vec<FileNode> with usize indices, tombstone())
        scan.rs               # ScanEvent enum, ScanConfig, ScanStats
        config.rs             # AppConfig + CustomCommand (TOML-deserializable)
        stats.rs              # ExtensionStats, HslColor, color_for_extension(), compute_extension_stats() (filters deleted nodes)
    rds-scanner/              # Parallel jwalk scanner + SHA-256 duplicate detection + glob-based exclude filtering
      Cargo.toml              # Deps: jwalk, crossbeam-channel, sha2, rayon, glob, tracing, rds-core
      CLAUDE.md
      README.md               # Ordering invariant, abort design, skip_hidden parity
      src/
        lib.rs                # Module declarations and re-exports
        scanner.rs            # Scanner::scan() — jwalk traversal, cancel flag, max_nodes, exclude pattern filtering via glob::Pattern, event streaming
        duplicate.rs          # DuplicateDetector — 3-phase pipeline (size grouping, partial hash, full SHA-256)
      tests/
        scan_integration.rs   # Integration tests with temp directory fixtures
        exclude_integration.rs # Exclude pattern filtering integration tests
        duplicate_integration.rs # Duplicate detection integration tests
    rds-gui/                  # egui/eframe GUI: dir picker, tree view, ext stats, treemap, duplicates, actions, export, settings, config persistence
      Cargo.toml              # Deps: eframe, egui, streemap, crossbeam-channel, rds-core, rds-scanner, rfd, trash, open, serde, serde_json, csv
      CLAUDE.md
      src/
        lib.rs                # RustDirStatApp: ScanPhase state machine, dir picker, scanner integration, 3-panel layout, config persistence (collect_config/save_config/on_exit), recent paths tracking, PendingDelete, confirm_delete, CommandEditorState, ExportDialogState, SettingsDialogState
        tree_view.rs           # SubtreeStats cache (filters deleted), TreeViewState, sorted tree rendering with context menu
        ext_stats.rs           # hsl_to_color32, extension stats panel with stacked bar + Grid table
        treemap.rs             # CushionCoeffs, TreemapLayout, recursive squarify, cushion mesh render, right-click context menu
        duplicates.rs          # Duplicates bottom panel with collapsible groups, wasted space, context menu
        actions.rs             # execute_delete (trash + tombstone), cleanup_duplicate_groups, open_in_file_manager, execute_custom_command
        command_editor.rs      # Command editor window: inline editing of custom commands, add/remove controls
        export.rs              # CSV/JSON export: ExportFormat/ExportScope enums, export_tree (DFS + serialize), export_duplicates, export dialog UI
        settings.rs            # Settings dialog: SettingsDialogState, exclude patterns editor, sort order/color scheme ComboBox, Apply/Cancel
  .github/
    workflows/
      ci.yml                  # Build + test + clippy + fmt on ubuntu/macos/windows
                              # Enforces rds-core zero-dep invariant via cargo tree
  docs/
    milestones.md             # MS1-MS21 roadmap with dependency graph
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
  justfile                    # Task runner: build, test, lint, fmt, run, check, clean
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
- Scanner thread runs duplicate detection before sending `ScanComplete`

### Configuration Persistence (src/main.rs + rds-core/src/config.rs)
- `AppConfig` defined in rds-core (serde-deserializable, `#[serde(default)]` for forward compatibility)
- File I/O owned by `main.rs` via `directories` (platform config dir) + `toml` crate
- Save callback (`Box<dyn Fn(&AppConfig) + Send>`) passed to `RustDirStatApp` to avoid coupling rds-gui to filesystem crates
- Auto-saves on settings dialog Apply, command editor changes, recent path tracking, and app exit (`on_exit`)
- Missing or corrupt config gracefully defaults (log warning, use `AppConfig::default()`)

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
| 17 | Configuration & Persistence | In Dev |
| 18-21 | See docs/milestones.md | Pending |
