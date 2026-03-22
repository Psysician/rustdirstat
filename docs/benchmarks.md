# Benchmarks and Memory Usage Audit

Performance characteristics of rustdirstat, measured on the current codebase.
All struct sizes are for x86-64 (64-bit pointers, 8-byte `usize`).

## Per-Node Memory Cost

### Struct sizes (stack only)

| Struct | Size (bytes) | Notes |
| ------ | ------------ | ----- |
| `FileNode` | 120 | Arena node; largest per-node cost |
| `DirTree` | 24 | Wrapper around `Vec<FileNode>` (ptr + len + cap) |
| `ScanEvent` | 136 | Largest variant is `NodeDiscovered` (embeds `FileNode`) |
| `ScanStats` | 40 | Five `u64` fields |
| `ScanConfig` | 72 | Includes `PathBuf` + `Vec<String>` + `Option<usize>` |
| `TreemapRect` | 72 | Flat display rect with cushion coefficients |
| `CushionCoeffs` | 16 | Four `f32` parabolic ridge coefficients |
| `TreemapLayout` | 40 | `Vec<TreemapRect>` + `Vec2` + `usize` |
| `SubtreeStats` | 48 | Two `Vec<u64>` (sizes + file_counts) |

### FileNode heap allocation breakdown

Each `FileNode` in the arena also allocates heap memory:

| Field | Stack (bytes) | Heap estimate (bytes) | Notes |
| ----- | ------------- | --------------------- | ----- |
| `name: String` | 24 | ~20 avg | Filename only (root stores full path) |
| `children: Vec<usize>` | 24 | 8 per child | 0 for leaf files; directories grow dynamically |
| `extension: Option<String>` | 24 | ~4 avg | Short extensions like "rs", "txt", "json" |
| `size: u64` | 8 | 0 | Inline |
| `is_dir: bool` | 1 | 0 | Inline |
| `parent: Option<usize>` | 16 | 0 | Inline |
| `modified: Option<u64>` | 16 | 0 | Inline |
| `deleted: bool` | 1 | 0 | Inline |
| **Total (leaf file)** | **120** | **~24** | ~144 bytes per leaf node |
| **Total (directory, 10 children)** | **120** | **~104** | ~224 bytes per directory |

Typical filesystem ratio is ~90% files, ~10% directories, so the weighted average is approximately **152 bytes per node**.

## Total Memory Projections

Projections for arena + auxiliary structures at various scales.

### Arena only (`Vec<FileNode>`)

| Node count | Arena stack+heap | Notes |
| ---------- | ---------------- | ----- |
| 100,000 | ~15 MB | Small project scan |
| 1,000,000 | ~152 MB | Medium system scan |
| 10,000,000 | ~1.52 GB | Full system scan (at `max_nodes` limit) |

### Auxiliary structures

| Structure | Formula | At 1M nodes |
| --------- | ------- | ----------- |
| `path_to_index: HashMap<PathBuf, usize>` | ~(80 + avg_path_len) per entry, ~1.5x capacity overhead | ~160 MB |
| `SubtreeStats` (two `Vec<u64>`) | 16 bytes per node | 16 MB |
| `TreemapLayout` (`Vec<TreemapRect>`) | 72 bytes per rect, capped at 50k | 3.6 MB (capped) |
| Crossbeam channel buffer | 136 bytes x 4096 slots | 0.5 MB (fixed) |

### Total estimated memory

| Node count | Arena | path_to_index | SubtreeStats | TreemapLayout | Channel | **Total** |
| ---------- | ----- | ------------- | ------------ | ------------- | ------- | --------- |
| 100,000 | 15 MB | 16 MB | 1.6 MB | 3.6 MB | 0.5 MB | ~37 MB |
| 1,000,000 | 152 MB | 160 MB | 16 MB | 3.6 MB | 0.5 MB | ~332 MB |
| 10,000,000 | 1.52 GB | 1.6 GB | 160 MB | 3.6 MB | 0.5 MB | ~3.3 GB |

Note: `path_to_index` is dropped after scanning completes. Post-scan memory is significantly lower (arena + SubtreeStats + TreemapLayout).

## Scan Throughput

Measured with `--scan-only` flag. Actual numbers depend on filesystem cache state, storage device, and directory structure.

| Directory | Node count | Duration | Throughput | Notes |
| --------- | ---------- | -------- | ---------- | ----- |
| (pending) | — | — | — | Run `cargo run -- --scan-only /usr` to measure |

To collect scan throughput data:

```sh
cargo run --release -- --scan-only /usr
```

## Treemap Rendering

- **Max display rects**: 50,000 (`MAX_DISPLAY_RECTS` constant)
- **Aggregation**: When leaf file count exceeds `MAX_DISPLAY_RECTS`, excess items are merged into "other" buckets per directory. Each aggregated rect stores `(file_count, total_bytes)` for tooltip display.
- **Aggregated rect sentinel**: `node_index = usize::MAX` distinguishes aggregated buckets from real nodes.
- **Cushion shading**: Rects smaller than 4x4 pixels receive flat fills instead of cushion mesh to avoid per-pixel overhead on tiny rectangles.

### Criterion benchmark results (treemap)

| Benchmark | Scale | Median | Notes |
| --------- | ----- | ------ | ----- |
| `treemap_layout_compute` | (pending) | — | Run `cargo bench -p rds-gui --features bench-internals` |
| `subtree_stats_compute` | (pending) | — | |
| `treemap_with_aggregation` | (pending) | — | |

### Criterion benchmark results (tree operations)

| Benchmark | Scale | Median | Notes |
| --------- | ----- | ------ | ----- |
| `insert_nodes` | (pending) | — | Run `cargo bench -p rds-core` |
| `subtree_size` | (pending) | — | |
| `compute_extension_stats` | (pending) | — | |

## Comparison with External Tools

Comparison benchmarks against `dust` and `dua` are available via:

```sh
just bench-compare /usr
```

This requires [hyperfine](https://github.com/sharkdp/hyperfine), [dust](https://github.com/bootandy/dust), and [dua](https://github.com/Byron/dua-cli) to be installed. The script is at `scripts/benchmark-comparison.sh`.

Results are optional and not included here by default since they depend on external tool availability.
