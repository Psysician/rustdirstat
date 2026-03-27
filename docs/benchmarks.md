# Benchmarks and Memory Usage Audit

Performance characteristics of rustdirstat, measured on the current codebase.
All struct sizes are for x86-64 (64-bit pointers, 8-byte `usize`).

## Per-Node Memory Cost

### Struct sizes (stack only)

| Struct | Size (bytes) | Notes |
| ------ | ------------ | ----- |
| `FileNode` | 40 | Arena node; compact via string arena, u32 indices, extension interning, flags bitfield |
| `DirTree` | 72 | `Vec<FileNode>` + `Vec<Box<str>>` extension table + `Vec<u8>` name buffer |
| `ScanEvent` | 56 | Largest variant is `NodeDiscovered` (compact `FileNode` + `Box<str>` name + `Option<Box<str>>` extension) |
| `ScanStats` | 40 | Five `u64` fields |
| `ScanConfig` | 72 | Includes `PathBuf` + `Vec<String>` + `Option<usize>` |
| `TreemapRect` | 96 | Flat display rect with cushion coefficients + aggregation metadata |
| `CushionCoeffs` | 16 | Four `f32` parabolic ridge coefficients |
| `TreemapLayout` | 40 | `Vec<TreemapRect>` + `Vec2` + `usize` |
| `SubtreeStats` | 48 | `Vec<u64>` sizes + `Vec<u32>` file_counts |

### FileNode field breakdown

All per-node data is inline (no per-node heap allocations):

| Field | Size (bytes) | Notes |
| ----- | ------------ | ----- |
| `name_offset: u32` | 4 | Index into `DirTree::name_buffer` |
| `name_len: u16` | 2 | Name length (max 65535, sufficient for any path component) |
| `size: u64` | 8 | Disk size in bytes |
| `modified: u64` | 8 | Unix timestamp (0 = unknown) |
| `parent: u32` | 4 | Parent index (`u32::MAX` = root) |
| `first_child: u32` | 4 | First child index (`u32::MAX` = leaf) |
| `next_sibling: u32` | 4 | Next sibling index (`u32::MAX` = end of list) |
| `extension: u16` | 2 | Index into `DirTree::extensions` table (0 = none) |
| `flags: u8` | 1 | Bit 0 = is_dir, bit 1 = deleted |
| padding | 3 | Alignment |
| **Total** | **40** | Zero per-node heap allocations |

Names are stored in a contiguous `DirTree::name_buffer` (shared `Vec<u8>`). Extensions are interned in `DirTree::extensions` (shared `Vec<Box<str>>`). Children use an intrusive first-child/next-sibling linked list (zero per-node heap allocations). Typical per-node cost including shared buffer share is approximately **60 bytes per node**.

## Total Memory Projections

Projections for arena + auxiliary structures at various scales.

### Arena + shared buffers

| Node count | FileNode arena | Name buffer (~20 B avg) | Extension table | Total arena |
| ---------- | -------------- | ----------------------- | --------------- | ----------- |
| 100,000 | 4 MB | 2 MB | <1 KB | ~6 MB |
| 1,000,000 | 40 MB | 20 MB | <1 KB | ~60 MB |
| 10,000,000 | 400 MB | 200 MB | <1 KB | ~600 MB |

### Auxiliary structures

| Structure | Formula | At 1M nodes |
| --------- | ------- | ----------- |
| `path_to_index: HashMap<PathBuf, u32>` (scan only) | ~(76 + avg_path_len) per entry, ~1.5x capacity overhead | ~155 MB |
| `SubtreeStats` (`Vec<u64>` sizes + `Vec<u32>` file_counts) | 12 bytes per node | 12 MB |
| `TreemapLayout` (`Vec<TreemapRect>`) | 96 bytes per rect, capped at 50k | 4.8 MB (capped) |
| `TreemapMeshCache` (`Arc<egui::Mesh>`) | Vertex/index data for cushion shading | ~5–25 MB (depends on rect count) |
| Crossbeam channel buffer | ~56 bytes x 4096 slots | 0.2 MB (fixed) |

### Total estimated memory (post-scan steady state)

| Node count | Arena + buffers | SubtreeStats | TreemapLayout | Mesh cache | **Total** |
| ---------- | --------------- | ------------ | ------------- | ---------- | --------- |
| 100,000 | 6 MB | 1.2 MB | 4.8 MB | ~5 MB | ~17 MB |
| 1,000,000 | 60 MB | 12 MB | 4.8 MB | ~15 MB | ~92 MB |
| 10,000,000 | 600 MB | 120 MB | 4.8 MB | ~25 MB | ~750 MB |

Note: `path_to_index` is dropped after scanning completes. Peak scan memory is higher by ~155 MB per 1M nodes.

## Scan Throughput

Measured with `--scan-only` flag. Actual numbers depend on filesystem cache state, storage device, and directory structure.

| Directory | Files | Dirs | Bytes | Duration | Files/sec | Errors |
| --------- | ----- | ---- | ----- | -------- | --------- | ------ |
| `/usr` | 100,073 | 10,076 | 20.5 GB | 11.12s | 8,995 | 3 |

To collect scan throughput data:

```sh
cargo run --release -- --scan-only /usr
```

## Treemap Rendering

- **Max display rects**: 50,000 (`MAX_DISPLAY_RECTS` constant)
- **Aggregation**: When leaf file count exceeds `MAX_DISPLAY_RECTS`, excess items are merged into "other" buckets per directory. Each aggregated rect stores `(file_count, total_bytes)` for tooltip display.
- **Aggregated rect sentinel**: `node_index = usize::MAX` distinguishes aggregated buckets from real nodes.
- **Cushion shading**: Rects smaller than 4x4 pixels receive flat fills instead of cushion mesh to avoid per-pixel overhead on tiny rectangles.
- **Mesh caching**: The cushion mesh (`Arc<egui::Mesh>`) is built once per layout and reused across frames via cheap Arc clone. Rebuilds only on layout change, extension filter change, or panel offset change.

### Criterion benchmark results (treemap)

Run with `cargo bench -p rds-gui --features bench-internals`.

| Benchmark | Scale | Median |
| --------- | ----- | ------ |
| `treemap_layout_compute` | 1,000 | 68.5 us |
| `treemap_layout_compute` | 10,000 | 778 us |
| `treemap_layout_compute` | 50,000 | 6.09 ms |
| `treemap_layout_compute` | 100,000 | 6.54 ms |
| `treemap_layout_compute` | 500,000 | 17.0 ms |
| `subtree_stats_compute` | 1,000 | 3.24 us |
| `subtree_stats_compute` | 10,000 | 35.0 us |
| `subtree_stats_compute` | 50,000 | 237 us |
| `subtree_stats_compute` | 100,000 | 1.33 ms |
| `subtree_stats_compute` | 500,000 | 11.3 ms |
| `treemap_with_aggregation` | 100,000 | 6.44 ms |
| `treemap_with_aggregation` | 500,000 | 16.6 ms |

Note: at 100k+ nodes the treemap layout time plateaus around 6-7ms due to the 50k rect cap -- aggregation prevents super-linear scaling.

### Criterion benchmark results (tree operations)

Run with `cargo bench -p rds-core`.

| Benchmark | Scale | Median |
| --------- | ----- | ------ |
| `insert_nodes` | 1,000 | 134 us |
| `insert_nodes` | 10,000 | 1.35 ms |
| `insert_nodes` | 100,000 | 15.3 ms |
| `insert_nodes` | 1,000,000 | 286 ms |
| `subtree_size` | 10,000 | 29.2 us |
| `subtree_size` | 100,000 | 859 us |
| `subtree_size` | 1,000,000 | 13.5 ms |
| `compute_extension_stats` | 10,000 | 508 us |
| `compute_extension_stats` | 100,000 | 5.64 ms |
| `compute_extension_stats` | 1,000,000 | 53.8 ms |

## Comparison with External Tools

Comparison benchmarks against `dust` and `dua` are available via:

```sh
just bench-compare /usr
```

This requires [hyperfine](https://github.com/sharkdp/hyperfine), [dust](https://github.com/bootandy/dust), and [dua](https://github.com/Byron/dua-cli) to be installed. The script is at `scripts/benchmark-comparison.sh`.

Results are optional and not included here by default since they depend on external tool availability.
