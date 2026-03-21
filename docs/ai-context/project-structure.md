# Project Structure

```
rustdirstat/
  Cargo.toml                  # Workspace root; all dep versions in [workspace.dependencies]
  Cargo.lock                  # Committed (binary crate — reproducible builds)
  src/
    main.rs                   # CLI parsing (clap), tracing init, eframe window launch
  crates/
    rds-core/                 # Shared data types — ZERO deps beyond serde
      Cargo.toml
      CLAUDE.md               # Crate-level AI context
      README.md               # Arena invariants and index stability contract
      src/
        lib.rs                # Re-exports all public types from submodules
        tree.rs               # FileNode + DirTree arena (Vec<FileNode> with usize indices)
        scan.rs               # ScanEvent enum, ScanConfig, ScanStats
        config.rs             # AppConfig + CustomCommand (TOML-deserializable)
        stats.rs              # ExtensionStats, HslColor, color_for_extension(), compute_extension_stats()
    rds-scanner/              # Parallel jwalk scanner + SHA-2 hashing (MS4 done)
      Cargo.toml              # Deps: jwalk, crossbeam-channel, tracing, rds-core
      CLAUDE.md
      README.md               # Ordering invariant, abort design, skip_hidden parity
      src/
        lib.rs                # Module declarations and re-exports
        scanner.rs            # Scanner::scan() — jwalk traversal, cancel flag, max_nodes, event streaming
      tests/
        scan_integration.rs   # Integration tests with temp directory fixtures
    rds-gui/                  # egui/eframe GUI: dir picker, tree view, ext stats, treemap (MS9 done)
      Cargo.toml              # Deps: eframe, egui, streemap, crossbeam-channel, rds-core, rds-scanner, rfd
      CLAUDE.md
      src/
        lib.rs                # RustDirStatApp: ScanPhase state machine, dir picker, scanner integration, 3-panel layout
        tree_view.rs           # SubtreeStats cache, TreeViewState, sorted tree rendering
        ext_stats.rs           # hsl_to_color32, extension stats panel with stacked bar + Grid table
        treemap.rs             # CushionCoeffs, TreemapLayout, recursive squarify, cushion mesh render
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
    ms10-panel-synchronization.md  # MS10 plan
  justfile                    # Task runner: build, test, lint, fmt, run, check, clean
  .gitignore                  # Ignores /target; Cargo.lock NOT ignored
  CLAUDE.md                   # Root AI context with file/directory guide
  README.md                   # Architecture, design decisions, invariants
```

## Key Architectural Patterns

### Arena-Allocated Tree (rds-core/src/tree.rs)
- `DirTree` = `Vec<FileNode>` with `usize` index references (not Rc/Box)
- Insert-only: nodes never removed, indices stable for lifetime of tree
- Cache-local traversal, zero reference-counting overhead
- Root always at index 0

### Scanner-GUI Channel Protocol (rds-core/src/scan.rs)
- Bounded `crossbeam-channel` carries `ScanEvent` variants from scanner thread to GUI
- GUI drains with `try_recv()` per frame (never blocks)
- `NodeDiscovered` carries full `FileNode` + `parent_index` (GUI-side arena index)
- Scanner thread runs duplicate detection before sending `ScanComplete`

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
| 10-21 | See docs/milestones.md | Pending |
