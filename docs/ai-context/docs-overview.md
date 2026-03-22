# Documentation Overview

Registry of all project documentation, classified by tier for navigation.

## Tier 1 — Foundational (Project-Wide)

| Path | Purpose |
|------|---------|
| `CLAUDE.md` | Root AI context: file/directory guide, build/test commands, dev requirements |
| `README.md` | Architecture decisions, arena design rationale, scanner-GUI event flow, dependency strategy |
| `docs/milestones.md` | MS1-MS21 roadmap with deliverables and dependency graph |
| `docs/superpowers/specs/2026-03-19-rustdirstat-design.md` | Full design specification: goals, architecture, data flow, dependencies, testing strategy |
| `docs/ai-context/project-structure.md` | Annotated file tree, architectural patterns, milestone status |
| `docs/ai-context/docs-overview.md` | This file — documentation registry |

## Tier 2 — Component-Level

| Path | Purpose |
|------|---------|
| `crates/CLAUDE.md` | Workspace crates directory index |
| `crates/rds-core/CLAUDE.md` | Core crate AI context: file guide, module structure, zero-dep constraint |
| `crates/rds-core/README.md` | Arena invariants, index stability contract, append-only design rationale |
| `crates/rds-core/src/CLAUDE.md` | Core source files index: tree, scan, config, stats modules |
| `crates/rds-scanner/CLAUDE.md` | Scanner crate AI context: jwalk parallel traversal, exclude pattern filtering, event streaming |
| `crates/rds-scanner/README.md` | Ordering invariant, two-level abort design, skip_hidden parity, error sources |
| `crates/rds-gui/CLAUDE.md` | GUI crate AI context: dir picker, scanner integration, tree view, panel layout, settings dialog, config persistence, toast notifications, error log, max-nodes dialog |
| `crates/rds-gui/src/CLAUDE.md` | GUI source files index: lib, notifications, error_log, tree_view, ext_stats, treemap, duplicates, actions, command_editor, export, settings |
| `docs/CLAUDE.md` | Documentation directory index |
| `plans/CLAUDE.md` | Implementation plans directory index |

## Tier 3 — Feature-Specific / Implementation Plans

| Path | Purpose |
|------|---------|
| `plans/ms1-workspace-scaffold-ci.md` | MS1 implementation plan with decision log (completed) |
| `plans/ms2-core-data-types.md` | MS2 implementation plan with decision log (completed) |
| `plans/ms3-single-threaded-scanner.md` | MS3 implementation plan with decision log (completed) |
| `plans/ms4-parallel-scanner-jwalk.md` | MS4 implementation plan with decision log (completed) |
| `plans/ms5-gui-shell-directory-picker.md` | MS5 implementation plan with decision log (completed) |
| `plans/ms6-directory-tree-view.md` | MS6 implementation plan with decision log (completed) |
| `plans/ms7-extension-statistics-panel.md` | MS7 implementation plan with decision log (completed) |
| `plans/ms8-treemap-layout-flat.md` | MS8 implementation plan with decision log (completed) |
| `plans/ms9-treemap-cushion-shading.md` | MS9 implementation plan with decision log (completed) |
| `plans/ms10-panel-synchronization.md` | MS10 implementation plan (completed) |
| `plans/ms11-incremental-scan-display.md` | MS11 implementation plan (completed) |
| `plans/ms12-duplicate-detection.md` | MS12 implementation plan (completed) |
| `plans/ms13-delete-action-trash.md` | MS13 implementation plan: tombstone design, trash crate, context menus, confirmation dialog (completed) |
| `docs/superpowers/plans/2026-03-21-ms14-open-in-file-manager.md` | MS14 implementation plan: open crate, platform-specific file reveal (completed) |
| `docs/superpowers/plans/2026-03-21-ms15-custom-commands.md` | MS15 implementation plan: custom shell commands, command editor UI, context menu integration (completed) |
| `docs/superpowers/plans/2026-03-22-ms16-csv-json-export.md` | MS16 implementation plan: CSV/JSON export, export dialog, format/scope selection, rfd save dialog (completed) |
| `docs/superpowers/plans/2026-03-22-ms17-configuration-persistence.md` | MS17 implementation plan: TOML config persistence, settings dialog, recent paths, exclude pattern filtering, auto-save (completed) |
| `docs/superpowers/plans/2026-03-22-ms18-error-handling-edge-cases.md` | MS18 implementation plan: toast notifications, scan error log, max-nodes dialog, action error surfacing, structured tracing, empty dir hints (in dev) |

## Cross-Reference Map

| Topic | Primary Source | Supporting Docs |
|-------|---------------|-----------------|
| Arena tree design | `crates/rds-core/README.md` | `tree.rs`, design spec |
| Scanner-GUI protocol | `README.md` (Architecture) | `scan.rs`, design spec |
| Dependency versions | `Cargo.toml` ([workspace.dependencies]) | `README.md` (Dependency management) |
| CI pipeline | `.github/workflows/ci.yml` | `README.md` (CI section) |
| Milestones & scope | `docs/milestones.md` | Design spec (Goals/Non-Goals) |
| Config schema | `crates/rds-core/src/config.rs` | Design spec (Configuration) |
| Extension colors | `crates/rds-core/src/stats.rs` | Design spec (Treemap Rendering) |
| Scanner implementation | `crates/rds-scanner/src/scanner.rs` | Design spec (Data Flow), `scan.rs` events |
| Scanner abort design | `crates/rds-scanner/README.md` | `scanner.rs`, design spec (cancellation) |
| GUI scan lifecycle | `crates/rds-gui/src/lib.rs` | Design spec (Data Flow) |
| Tree view rendering | `crates/rds-gui/src/tree_view.rs` | `lib.rs` (panel layout) |
| Extension stats panel | `crates/rds-gui/src/ext_stats.rs` | `stats.rs` (core types), `lib.rs` (wiring) |
| Treemap + cushion shading | `crates/rds-gui/src/treemap.rs` | Design spec (Treemap Rendering), `stats.rs` (colors) |
| Delete action + tombstone | `crates/rds-gui/src/actions.rs` | `tree.rs` (tombstone), `lib.rs` (PendingDelete/confirm_delete) |
| Duplicate detection | `crates/rds-scanner/src/duplicate.rs` | `scan.rs` (DuplicateFound event), `duplicates.rs` (GUI panel) |
| CSV/JSON export | `crates/rds-gui/src/export.rs` | `lib.rs` (ExportDialogState), design spec (export to CSV/JSON) |
| Custom commands | `crates/rds-gui/src/actions.rs` | `command_editor.rs` (editor UI), `lib.rs` (CommandEditorState) |
| Config persistence | `src/main.rs` (load/save) | `config.rs` (AppConfig), `lib.rs` (collect_config/save_config/on_exit) |
| Settings dialog | `crates/rds-gui/src/settings.rs` | `lib.rs` (SettingsDialogState), `config.rs` (AppConfig fields) |
| Exclude pattern filtering | `crates/rds-scanner/src/scanner.rs` | `config.rs` (exclude_patterns), `lib.rs` (pass to ScanConfig) |
| Recent paths | `crates/rds-gui/src/lib.rs` | `config.rs` (recent_paths, max_recent_paths) |
| Toast notifications | `crates/rds-gui/src/notifications.rs` | `lib.rs` (overlay render), `actions.rs`/`duplicates.rs`/`export.rs` (error surfacing) |
| Scan error log | `crates/rds-gui/src/error_log.rs` | `lib.rs` (drain_events, error log panel), `scan.rs` (ScanError event) |
| Structured tracing | `crates/rds-scanner/src/scanner.rs` | `duplicate.rs` (pipeline span), `main.rs` (RUST_LOG env filter) |
