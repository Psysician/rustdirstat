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
    rds-scanner/              # Single-threaded walkdir scanner + SHA-2 hashing (MS3 done)
      Cargo.toml              # Deps: walkdir, crossbeam-channel, tracing, rds-core
      CLAUDE.md
      src/
        lib.rs                # Module declarations and re-exports
        scanner.rs            # Scanner::scan() — walkdir traversal, cancel flag, max_nodes, event streaming
      tests/
        scan_integration.rs   # Integration tests with temp directory fixtures
    rds-gui/                  # egui/eframe GUI shell + treemap rendering
      Cargo.toml              # Deps: eframe, egui, streemap, crossbeam-channel, tracing, rds-core
      CLAUDE.md
      src/
        lib.rs                # RustDirStatApp implementing eframe::App (empty panel shell)
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
    ms1-workspace-scaffold-ci.md  # MS1 implementation plan (completed)
    ms2-core-data-types.md        # MS2 implementation plan (completed)
    ms3-single-threaded-scanner.md # MS3 implementation plan (completed)
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
| 4-21 | See docs/milestones.md | Pending |
