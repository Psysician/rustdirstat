# rustdirstat — Milestones

Cross-platform disk usage analyzer with interactive treemap visualization.
Rust rewrite of WinDirStat. Each milestone gets its own detailed implementation plan when work begins.

**Spec:** [docs/superpowers/specs/2026-03-19-rustdirstat-design.md](superpowers/specs/2026-03-19-rustdirstat-design.md)
**Repo:** https://github.com/Psysician/rustdirstat

---

## ~~MS1 — Workspace Scaffold & CI~~ DONE

Set up Cargo workspace with 4 crates (rds-core, rds-scanner, rds-gui, binary). Cargo.toml with all dependencies declared. Basic `main.rs` that opens an empty egui window. GitHub Actions CI with build + test matrix (ubuntu, macos, windows). Makefile or justfile with common commands.

**Deliverable:** `cargo build` and `cargo test` pass on all 3 platforms. Empty egui window opens.

---

## ~~MS2 — Core Data Types~~ DONE

Implement `rds-core`: `FileNode`, `DirTree` (arena allocator), `ScanEvent` enum, `ScanConfig`, `ScanStats`, `AppConfig`. Unit tests for tree operations (insert, subtree size, path reconstruction, parent-child linking).

**Deliverable:** `cargo test -p rds-core` passes with full coverage of tree operations.

---

## MS3 — Single-Threaded Scanner

Implement basic `Scanner::scan()` in `rds-scanner` using `walkdir` (single-threaded first). Sends `NodeDiscovered` events through crossbeam channel. Integration test: scan a temp directory fixture, verify file counts, sizes, tree structure.

**Deliverable:** Scanner can scan a directory and produce a correct `DirTree` via events.

---

## MS4 — Parallel Scanner (jwalk)

Replace walkdir with jwalk for parallel traversal. Add `Arc<AtomicBool>` cancel flag checked in `process_read_dir` callback. Add `max_nodes` abort. Test: verify scan is faster than MS3 on large directories, verify cancellation works.

**Deliverable:** Parallel scanning with cancellation support. Benchmark showing speedup.

---

## MS5 — GUI Shell & Directory Picker

Wire up `RustDirStatApp` in `rds-gui`. Layout with 3 panel placeholders (tree, treemap, ext stats). Directory picker dialog (via `rfd` or egui built-in). On path selection, spawn scanner, consume events via `try_recv()`, display progress bar (files scanned, bytes scanned).

**Deliverable:** User can pick a directory, see a progress bar during scan.

---

## MS6 — Directory Tree View

Implement `TreeView` panel. Expandable tree sorted by size descending. Shows name, size (human-readable), file count per directory. Click to select node (selection state shared across panels).

**Deliverable:** After scan completes, user can browse the full directory tree with expand/collapse.

---

## MS7 — Extension Statistics Panel

Implement `ExtStatsPanel`. Compute `ExtensionStats` from `DirTree`. Assign deterministic HSL colors per extension. Display sorted table: extension, count, total size, percentage, color swatch. Horizontal bar chart.

**Deliverable:** Extension breakdown visible after scan. Colors assigned and consistent.

---

## MS8 — Treemap Layout (Flat)

Implement `TreemapRenderer` with flat colored rectangles (no cushion shading yet). Use `streemap::Squarified` for layout. Render via `Painter::rect_filled()`. Colors from `ExtensionStats`. Click to select, hover tooltip (path, size). Cache layout, recompute on resize.

**Deliverable:** Flat colored treemap visible after scan. Click/hover interaction works.

---

## MS9 — Treemap Cushion Shading

Add cushion shading to treemap renderer. Implement cushion function `I(x,y)` with per-vertex color mesh via `Shape::Mesh`. Depth-based intensity. Fallback to flat for rectangles <4px. Performance test with 50k+ rectangles.

**Deliverable:** Treemap looks like WinDirStat with 3D cushion effect. Smooth at 60fps for typical scans.

---

## MS10 — Panel Synchronization

Wire up cross-panel selection: click in tree view highlights in treemap and ext stats. Click treemap rectangle selects in tree view. Click extension in stats highlights all matching files in treemap. Double-click treemap to drill into subdirectory, breadcrumb to navigate back.

**Deliverable:** All 3 panels fully synchronized. Drill-down and navigation working.

---

## MS11 — Incremental Scan Display

Show tree and treemap updating in real-time during scan (not just after completion). Throttle treemap re-layout to max 2x/second during scan. Progress bar with ETA. Tree view auto-expands root during scan.

**Deliverable:** User sees results building up live during scan, not a blank screen until done.

---

## MS12 — Duplicate Detection

Implement `DuplicateDetector` in `rds-scanner`. 3-phase: group by size, partial hash (4KB), full SHA-256 via rayon. Sends `DuplicateFound` events. GUI shows duplicates panel/tab with groups, total wasted space, and per-group file list.

**Deliverable:** Duplicate files detected and displayed. Wasted space calculated.

---

## MS13 — Delete Action (Trash)

Implement delete via `trash` crate (cross-platform recycle bin). Context menu on tree/treemap nodes. Confirmation dialog. After delete: remove node from tree, update sizes, re-layout treemap. Track freed space in session.

**Deliverable:** User can delete files/folders to recycle bin from the app. Tree updates correctly.

---

## MS14 — Open in File Manager

Implement "Open in File Manager" action via `open` crate. Context menu option on any node. Opens containing folder with the file selected (platform-dependent behavior).

**Deliverable:** Right-click -> Open in File Manager works on all 3 platforms.

---

## MS15 — Custom Commands

User-configurable shell commands with `{path}` placeholder. Config UI to add/edit/remove commands. Execute via `std::process::Command` with platform shell. Commands appear in context menu.

**Deliverable:** User can define and run custom commands on selected files/folders.

---

## MS16 — CSV/JSON Export

Export scan results to CSV and JSON via serde. Export options: full tree, current view, duplicates only. File save dialog. Progress indicator for large exports.

**Deliverable:** User can export scan results in both formats.

---

## MS17 — Configuration & Persistence

Implement `AppConfig` loading/saving via TOML. Platform config dir via `directories` crate. Persist: exclude patterns, custom commands, UI preferences (color scheme, sort order), recent scan paths. Settings dialog in GUI.

**Deliverable:** App remembers settings and recent paths across restarts.

---

## MS18 — Error Handling & Edge Cases

Harden all error paths: permission denied directories, files disappearing mid-scan, symlink cycles (jwalk handles), max_nodes abort with user-friendly message, empty directories, drives with no free space. Tracing-based structured logging.

**Deliverable:** App handles all error cases gracefully without crashes.

---

## MS19 — Performance Optimization

Profile and optimize for large scans (1M+ files). Treemap aggregation for >50k rectangles ("other" bucket). Arena allocator tuning. Memory usage audit. Benchmark suite comparing against dust/dua-cli scan times.

**Deliverable:** Documented benchmarks. Smooth 60fps rendering with 1M file trees.

---

## MS20 — Cross-Platform Polish

Platform-specific fixes: native file dialogs, keyboard shortcuts, HiDPI scaling, dark/light theme following OS preference. Test on Windows 10/11, macOS (arm64 + x86), Ubuntu/Fedora. Fix any platform-specific rendering issues.

**Deliverable:** App looks and feels native on all 3 platforms.

---

## MS21 — Packaging & Distribution

GitHub Releases with pre-built binaries (cross-compiled via `cross` or GitHub Actions matrix). Install instructions for cargo, winget, brew, AUR. README with screenshots. LICENSE (MIT).

**Deliverable:** Users can install rustdirstat on any platform without building from source.

---

## Milestone Dependency Graph

```
MS1 (scaffold)
 └─ MS2 (core types)
     ├─ MS3 (single-thread scanner)
     │   └─ MS4 (parallel scanner)
     │       └─ MS12 (duplicate detection)
     └─ MS5 (GUI shell)
         ├─ MS6 (tree view)
         ├─ MS7 (ext stats)
         ├─ MS8 (flat treemap)
         │   └─ MS9 (cushion shading)
         └─ MS11 (incremental display)

MS6 + MS7 + MS8 ─── MS10 (synchronization)
MS10 ──────────────── MS13 (delete)
MS10 ──────────────── MS14 (open in FM)
MS13 + MS14 ───────── MS15 (custom commands)
MS10 ──────────────── MS16 (export)
MS15 + MS16 ───────── MS17 (config)
MS17 ──────────────── MS18 (error handling)
MS18 ──────────────── MS19 (performance)
MS19 ──────────────── MS20 (cross-platform polish)
MS20 ──────────────── MS21 (packaging)
```
