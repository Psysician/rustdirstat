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
- `ScanEvent` — enum streamed from scanner to GUI:
  - `NodeDiscovered { node: FileNode, parent_index: Option<usize> }` — carries full node data so the GUI can insert it into its own arena. The `parent_index` refers to the GUI-side arena index (scanner maps its own indices to GUI indices via a lookup table sent back through a response channel, or the GUI assigns indices on insertion and the scanner tracks the mapping internally)
  - `Progress { files_scanned: u64, bytes_scanned: u64 }`
  - `DuplicateFound { hash: [u8; 32], node_indices: Vec<usize> }` — indices refer to GUI-side arena
  - `ScanComplete { stats: ScanStats }` — final summary with total files, dirs, bytes, duration
  - `ScanError { path: PathBuf, error: String }` — error serialized to String for Send safety
- `ScanConfig` — scan parameters: root path, follow symlinks, exclude patterns, hash duplicates toggle, `max_nodes: Option<usize>` (default: 10_000_000 — scan aborts with `ScanError` if exceeded)

**rds-scanner** — Depends on rds-core, jwalk, rayon, sha2, crossbeam-channel. Provides:
- `Scanner::scan(config, sender, cancel: Arc<AtomicBool>)` — spawns jwalk parallel walk on a background thread. For each discovered entry, sends a `NodeDiscovered` event with full `FileNode` data through the crossbeam channel. The cancel flag is checked in jwalk's `process_read_dir` callback; when set to `true`, the walk stops and the thread exits cleanly. Returns `JoinHandle`
- `DuplicateDetector::find_duplicates(tree, sender)` — called on the same scanner thread after the walk completes (before the sender is dropped). Groups files by size, hashes candidates in parallel via rayon, sends `DuplicateFound` events through the same channel. Only after this completes does the thread send `ScanComplete` and exit
- Respects permission errors gracefully (logs via tracing and sends `ScanError` event, continues scan)

**rds-gui** — Depends on rds-core, eframe/egui, streemap, crossbeam-channel. Provides:
- `RustDirStatApp` — main egui app. Owns a `DirTree` arena and a crossbeam `Receiver<ScanEvent>`. Each frame calls `try_recv()` in a loop (bounded to ~100 events per frame to avoid blocking rendering). On `NodeDiscovered`, inserts the `FileNode` into its own arena and updates parent-child links via `parent_index`. On re-scan, sets the cancel flag on the old scanner, drains/discards remaining events, resets the tree, and launches a new scan
- `TreemapRenderer` — takes a `DirTree` subtree, computes squarified layout via `streemap`, renders cushion-shaded colored rectangles using egui `Painter`. Handles click (select node), hover (tooltip with path/size), right-click (context menu), zoom (double-click to drill into subdirectory)
- `TreeView` — renders expandable directory tree sorted by size descending. Clicking a node selects it in treemap and ext stats
- `ExtStatsPanel` — shows extension breakdown: colored bars, file count, total size, percentage. Clicking an extension highlights all files of that type in treemap
- `ActionPanel` — delete (via `trash` crate), open in file manager (via `open` crate), run custom command, export to CSV/JSON

### Data Flow

```
User selects path
       |
       v
main.rs creates crossbeam channel (tx, rx)
main.rs creates Arc<AtomicBool> cancel flag
       |
       v
Scanner::scan(config, tx, cancel)     # spawns background thread
       |
  jwalk parallel walk (checks cancel flag in process_read_dir)
       |
  sends NodeDiscovered events ------> crossbeam channel
       |                                      |
  walk completes                              v
       |                            RustDirStatApp::update()  # GUI thread
  DuplicateDetector runs                      |
  (same thread, same tx)             try_recv() loop (max 100/frame)
       |                                      |
  sends DuplicateFound events ------> inserts FileNode into GUI-side DirTree
       |                                      |
  sends ScanComplete                 recompute treemap layout (if dirty)
       |                                      |
  thread exits, tx dropped           render 3 panels

On re-scan:
  1. Set cancel flag on old scanner
  2. Drain/discard old channel
  3. Reset DirTree
  4. Create new channel + cancel flag
  5. Launch new Scanner::scan
```

### Treemap Rendering

1. **Layout:** `streemap::Squarified` computes rectangles from `DirTree` children sizes
2. **Cushion shading:** Each rectangle gets a per-pixel intensity gradient based on directory depth using the cushion function `I(x,y) = Ix*(x-cx)^2 + Iy*(y-cy)^2` attenuated by a light vector. Implemented via egui `Painter::add(Shape::Mesh(...))` — each treemap rectangle is decomposed into a grid of triangulated quads with per-vertex colors computed from the cushion function. This produces the characteristic convex 3D highlight effect matching WinDirStat. For small rectangles (<4px), falls back to flat `rect_filled()` for performance
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
| trash | 4.x | Cross-platform recycle bin delete |
| open | 5.x | Open file manager / URLs |
| directories | 5.x | Platform config/cache dirs |
| toml | 0.8+ | Config file parsing (TOML format) |
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
- Node limit exceeded: scan aborts with `ScanError` when `max_nodes` (default 10M) is reached. GUI shows error message with suggestion to scan a subdirectory instead. Estimated memory per node: ~200 bytes, so 10M nodes ~ 2GB RAM

## Configuration

Stored in platform-appropriate config directory via `directories` crate:
- Exclude patterns (glob)
- Custom cleanup commands (name + shell command template with `{path}` placeholder)
- UI preferences: treemap color scheme, default sort order
- Last scanned paths (recent history)

Format: TOML via `toml` crate. Config loading owned by the binary crate (`main.rs`), passed to `RustDirStatApp` at initialization. rds-core defines the `AppConfig` struct (serde-deserializable); main.rs handles file I/O via `toml` + `directories` crates
