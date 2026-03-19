# rustdirstat — Design Specification

Cross-platform disk usage analyzer with interactive treemap visualization. A Rust rewrite of WinDirStat targeting Windows, macOS, and Linux.

## Goals

1. Scan any directory or drive and display disk usage as an interactive squarified treemap with cushion shading
2. Provide three synchronized views: directory tree, treemap, and extension statistics
3. Scan in parallel using all available CPU cores with real-time incremental results
4. Detect duplicate files via parallel SHA-256 hashing
5. Offer cleanup actions: delete (recycle bin), open in file manager, custom shell commands
6. Export scan results to CSV and JSON
7. Run on Windows, macOS, and Linux from a single codebase

## Non-Goals

- Network drive optimization (works but no special handling)
- NTFS MFT direct scanning (would require Windows-only unsafe code)
- Cloud storage integration
- File content preview
- Scheduled/automated scanning

## Architecture

### Workspace Layout

```
rustdirstat/
  Cargo.toml              # workspace root
  src/
    main.rs               # binary: CLI args (clap), launches eframe
  crates/
    rds-core/
      src/lib.rs          # FileNode, DirTree (arena), ExtensionStats, ScanEvent
    rds-scanner/
      src/lib.rs          # parallel scanner (jwalk), duplicate detector (rayon+sha2)
    rds-gui/
      src/
        lib.rs            # egui app struct, panel layout
        treemap.rs        # squarified layout (streemap) + cushion shading renderer
        tree_view.rs      # directory tree panel (expandable, sorted by size)
        ext_stats.rs      # extension statistics panel (color legend, bar chart)
        actions.rs        # delete, open, custom commands UI
```

### Crate Responsibilities

**rds-core** — Zero dependencies beyond std and serde. Defines:
- `FileNode` — represents a file or directory: name, size, metadata, children indices, parent index
- `DirTree` — arena-allocated tree (`Vec<FileNode>` with index-based references). Provides traversal, subtree size computation, path reconstruction
- `ExtensionStats` — per-extension aggregation: count, total size, percentage, assigned color
- `ScanEvent` — enum streamed from scanner to GUI: `DirScanned { path, node_index }`, `Progress { files_scanned, bytes_scanned }`, `DuplicateFound { hash, indices }`, `ScanComplete`, `ScanError { path, error }`
- `ScanConfig` — scan parameters: root path, follow symlinks, exclude patterns, hash duplicates toggle

**rds-scanner** — Depends on rds-core, jwalk, rayon, sha2, crossbeam-channel. Provides:
- `Scanner::scan(config, sender)` — spawns jwalk parallel walk on a background thread. Builds `DirTree` incrementally, sends `ScanEvent`s through crossbeam channel. Returns `JoinHandle`
- `DuplicateDetector::find_duplicates(tree)` — groups files by size, then hashes candidates in parallel with rayon. Sends `DuplicateFound` events
- Respects permission errors gracefully (logs and continues)

**rds-gui** — Depends on rds-core, eframe/egui, streemap. Provides:
- `RustDirStatApp` — main egui app. Holds `DirTree`, receives `ScanEvent`s each frame via `try_recv()` on crossbeam channel
- `TreemapRenderer` — takes a `DirTree` subtree, computes squarified layout via `streemap`, renders cushion-shaded colored rectangles using egui `Painter`. Handles click (select node), hover (tooltip with path/size), right-click (context menu), zoom (double-click to drill into subdirectory)
- `TreeView` — renders expandable directory tree sorted by size descending. Clicking a node selects it in treemap and ext stats
- `ExtStatsPanel` — shows extension breakdown: colored bars, file count, total size, percentage. Clicking an extension highlights all files of that type in treemap
- `ActionPanel` — delete (via `trash` crate), open in file manager (via `open` crate), run custom command, export to CSV/JSON

### Data Flow

```
User selects path
       |
       v
Scanner::scan(config, tx)          # background thread
       |
  jwalk parallel walk
       |
  builds DirTree + sends ScanEvents --> crossbeam channel
                                              |
                                              v
                                    RustDirStatApp::update()  # GUI thread, each frame
                                              |
                                         try_recv() loop
                                              |
                                    merge events into DirTree
                                              |
                                    recompute treemap layout (if dirty)
                                              |
                                    render 3 panels
```

### Treemap Rendering

1. **Layout:** `streemap::Squarified` computes rectangles from `DirTree` children sizes
2. **Cushion shading:** Each rectangle gets a gradient based on directory depth. Deeper = darker edges, lighter center. Implemented via egui `Painter::rect_filled()` with computed color gradients
3. **Colors:** Each file extension gets a deterministic HSL color (hash extension string to hue, fixed saturation/lightness). Colors stored in `ExtensionStats`
4. **Interaction:** Click selects node (highlights in tree + ext stats). Hover shows tooltip (path, size, modified date). Double-click drills into directory. Right-click opens context menu (delete, open, copy path)
5. **Performance:** Cache treemap layout. Only recompute when tree changes (during scan) or window resizes. For trees with >50k visible rectangles, aggregate small files into "other" bucket

### Duplicate Detection

1. Group all files by size (files with unique sizes cannot be duplicates)
2. For each size group with 2+ files, read first 4KB and hash (quick filter)
3. For matching partial hashes, compute full SHA-256 in parallel via rayon
4. Report duplicate groups with total wasted space

### Platform Abstraction

- **File operations:** `std::fs` for metadata, `trash` crate for cross-platform recycle bin
- **Open file manager:** `open` crate (uses `xdg-open` on Linux, `open` on macOS, `explorer` on Windows)
- **Custom commands:** `std::process::Command` with platform-appropriate shell (`sh -c` on Unix, `cmd /c` on Windows)
- **File sizes:** `std::fs::metadata().len()` for logical size. Platform-specific for allocated size (blocks on Unix, compressed size on Windows) — deferred to later milestone

## Key Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| eframe/egui | latest | GUI framework + native window |
| jwalk | 0.8+ | Parallel directory traversal |
| rayon | 1.x | Parallel hash computation |
| crossbeam-channel | 0.5+ | Scanner-to-GUI event streaming |
| streemap | latest | Squarified treemap layout algorithm |
| sha2 | 0.10+ | SHA-256 for duplicate detection |
| clap | 4.x | CLI argument parsing |
| serde + serde_json + csv | latest | Export functionality |
| trash | 5.x | Cross-platform recycle bin delete |
| open | 5.x | Open file manager / URLs |
| directories | 5.x | Platform config/cache dirs |
| tracing | 0.1+ | Structured logging |

## Testing Strategy

- **rds-core:** Unit tests for DirTree operations (insert, subtree size, path reconstruction), ExtensionStats aggregation
- **rds-scanner:** Integration tests scanning a temp directory fixture. Verify event stream correctness, file counts, size totals. Test permission errors, symlinks, empty dirs
- **rds-gui:** No GUI unit tests. Manual testing + screenshot regression via CI (optional later milestone)
- **Duplicate detection:** Unit tests with known duplicate files in temp dir
- **Cross-platform CI:** GitHub Actions matrix (ubuntu, macos, windows)

## Error Handling

- Permission denied on directory: log warning via `tracing::warn!`, skip directory, continue scan
- File disappeared during scan (race condition): log, skip, continue
- Hash I/O error: skip file for duplicate detection, report in UI
- Out of memory: not handled explicitly (in-memory tree assumption). Document recommended max scan sizes

## Configuration

Stored in platform-appropriate config directory via `directories` crate:
- Exclude patterns (glob)
- Custom cleanup commands (name + shell command template with `{path}` placeholder)
- UI preferences: treemap color scheme, default sort order
- Last scanned paths (recent history)

Format: TOML via `toml` crate
