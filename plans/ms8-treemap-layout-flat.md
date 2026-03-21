# MS8: Flat Treemap Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a flat-colored treemap renderer that lays out file rectangles via `streemap::squarify`, renders them with `Painter::rect_filled()`, colors by extension, supports click-to-select and hover tooltips, and caches the layout to avoid per-frame recomputation.

**Architecture:** `treemap.rs` is extracted from the central-panel placeholder (as planned in MS5 DL-006). Layout is computed once after scan completes and cached in a `TreemapLayout` struct on `RustDirStatApp`. A recursive `compute_recursive` function walks the `DirTree` top-down: at each directory level it gathers children, sorts by subtree size descending, runs `streemap::squarify` to assign proportional rectangles, then recurses into directory children or records file children as colored `TreemapRect` entries. The `show` function renders all cached rects, handles hover tooltips (file path + size), click-to-select (sets shared `selected_node`), and highlights the selected rectangle. Layout uses relative (0,0)-origin coordinates; rendering offsets by the panel's allocated position. Recomputation triggers only when the panel size changes by more than 1px.

**Tech Stack:** egui 0.33 (Painter, allocate_painter, show_tooltip_at_pointer), streemap 0.1 (squarify, Rect), rds-core (DirTree, FileNode, color_for_extension, HslColor), rds-gui (SubtreeStats from tree_view, hsl_to_color32 from ext_stats, format_bytes)

---

## Planning Context

### Decision Log

| ID | Decision | Reasoning |
|---|---|---|
| DL-001 | `treemap.rs` extracted as a separate module from lib.rs | MS5 DL-006 explicitly planned this: "MS6/MS7/MS8 will extract tree_view.rs, ext_stats.rs, treemap.rs when those panels get real implementations." Keeps lib.rs focused on app state and layout orchestration. |
| DL-002 | Layout computed in relative (0,0)-origin coordinates, offset during rendering | Decouples layout cache from panel position. Side-panel resizes move the central panel but don't change its size — relative coordinates avoid unnecessary recomputes. Cache invalidation based on size only. |
| DL-003 | Recursive top-down squarify for hierarchical layout | Standard treemap approach: at each directory level, children get rectangles proportional to their subtree size. Directories subdivide recursively; files become colored leaf rectangles. Recursion depth is bounded by filesystem depth (typically 10-30 levels), not node count — no stack overflow risk. |
| DL-004 | Colors from `color_for_extension` (rds-core) + `hsl_to_color32` (ext_stats.rs) | Reuses existing infrastructure. MS7 DL-001 noted `hsl_to_color32` would serve MS8 treemap coloring. Same deterministic byte-sum hashing gives consistent colors across panels. |
| DL-005 | Layout cached as `Option<TreemapLayout>` on `RustDirStatApp`, recomputed on size change | `compute_recursive` is O(n) over the arena. Computing per-frame at 60fps would be wasteful. Compute lazily on first render after scan, recompute only when panel size changes (>1px tolerance). Matches how `subtree_stats` is cached. |
| DL-006 | Zero-size files and empty directories filtered from layout | Zero-size items produce zero-area rectangles, which are invisible and waste layout computation. Filter at collection time before squarify. |
| DL-007 | 0.5px rect inset for visual separation between adjacent rectangles | Creates a 1px gap between adjacent rectangles using the panel's background color. Minimal visual overhead, no extra draw calls. Matches WinDirStat's thin-border aesthetic. |
| DL-008 | Selected node highlighted with 2px white stroke (StrokeKind::Outside) | Visually distinguishes the selected file. White stroke is visible against any extension color. Outside stroke avoids covering the colored fill. Matches planned MS10 cross-panel selection pattern. |
| DL-009 | Tooltip shows full path and human-readable size via `show_tooltip_at_pointer` | `show_tooltip_at_pointer` is called conditionally (only when a rect is hovered), not on every frame. Path reconstruction via `tree.path(index)` is O(depth) — negligible cost for tooltips. |
| DL-010 | `MIN_RECT_DIM` constant (1.0px) skips containers too small to subdivide | Prevents sub-pixel rectangle allocation. When a directory's assigned rect is smaller than 1px in either dimension, its children would be invisible anyway. Stops recursion early, improving performance on deep narrow branches. |

### Constraints

- rds-core zero-dep invariant (beyond serde) must not be violated; no changes to rds-core in this milestone
- All existing tests must pass (`cargo test --workspace`)
- No new crate dependencies required (`streemap` already declared in rds-gui Cargo.toml)
- Real filesystem I/O only, no mocks
- `SubtreeStats` from tree_view.rs used as-is for subtree sizes
- `hsl_to_color32` from ext_stats.rs used as-is for color conversion
- MS7 may or may not be fully wired when this plan executes; only `hsl_to_color32` (Task 1 of MS7, already implemented) is required

### Known Risks

- **Very large trees (100k+ files) may produce many TreemapRects**: For MS8, render all rectangles. Performance optimization (aggregation into "other" bucket) is MS19 scope.
- **Deep filesystem hierarchies cause deep recursion**: Recursion depth equals filesystem depth (typically <30). Safe for all practical directory structures. If a pathological case arises, convert to iterative with explicit stack in MS19.
- **Floating-point rounding in squarify may produce sub-pixel rects**: `MIN_RECT_DIM` filter catches these. Rects smaller than 1px are dropped.

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/rds-gui/src/treemap.rs` | Create | `TreemapRect` (node index + rect + color), `TreemapLayout` (cached rects + size), `compute_recursive` (recursive squarify), `show` (render + tooltip + click), unit tests for layout properties |
| `crates/rds-gui/src/lib.rs` | Modify | Add `mod treemap`, add `treemap_layout` field, reset in `start_scan`, replace central panel placeholder with lazy compute + treemap::show, update `default_is_idle` test |

---

### Task 1: Create treemap.rs with types and layout computation (TDD)

**Files:**
- Create: `crates/rds-gui/src/treemap.rs`
- Modify: `crates/rds-gui/src/lib.rs` (add `mod treemap;` declaration)

- [ ] **Step 1: Add module declaration to lib.rs**

In `crates/rds-gui/src/lib.rs`, add after the existing module declarations (`mod ext_stats;` and `mod tree_view;`):

```rust
mod treemap;
```

- [ ] **Step 2: Create treemap.rs with type stubs and failing tests**

Create `crates/rds-gui/src/treemap.rs`:

```rust
//! Flat-colored treemap renderer.
//!
//! Computes a squarified treemap layout from a `DirTree` using `streemap::squarify`,
//! renders colored rectangles via `egui::Painter`, and handles click/hover interaction.
//! Layout is cached and recomputed only when the panel size changes. (ref: DL-001, DL-003)

use rds_core::tree::DirTree;

use crate::ext_stats;
use crate::tree_view::SubtreeStats;

/// Minimum rectangle dimension (width or height) below which a container
/// is too small to subdivide. Prevents invisible sub-pixel rectangles
/// from bloating the layout list. (ref: DL-010)
const MIN_RECT_DIM: f32 = 1.0;

/// A single rectangle in the treemap, representing a leaf file node.
pub(crate) struct TreemapRect {
    pub node_index: usize,
    pub rect: egui::Rect,
    pub color: egui::Color32,
}

/// Cached treemap layout. Stored on `RustDirStatApp` and recomputed
/// when the panel size changes by more than 1px. (ref: DL-005)
pub(crate) struct TreemapLayout {
    pub rects: Vec<TreemapRect>,
    pub last_size: egui::Vec2,
}

/// Intermediate item passed to `streemap::squarify`. Holds the node's
/// subtree size (for proportional layout) and receives the computed rect.
struct LayoutItem {
    size: f32,
    node_index: usize,
    rect: streemap::Rect<f32>,
}

impl TreemapLayout {
    /// Computes a squarified treemap layout for all file nodes in `tree`.
    ///
    /// Coordinates are relative to a (0,0) origin with the given `size`.
    /// The caller offsets by the panel's actual position during rendering.
    /// (ref: DL-002)
    pub fn compute(tree: &DirTree, stats: &SubtreeStats, size: egui::Vec2) -> Self {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rds_core::tree::FileNode;

    fn make_file(name: &str, size: u64, ext: Option<&str>) -> FileNode {
        FileNode {
            name: name.to_string(),
            size,
            is_dir: false,
            children: Vec::new(),
            parent: None,
            extension: ext.map(|e| e.to_string()),
            modified: None,
        }
    }

    fn make_dir(name: &str) -> FileNode {
        FileNode {
            name: name.to_string(),
            size: 0,
            is_dir: true,
            children: Vec::new(),
            parent: None,
            extension: None,
            modified: None,
        }
    }

    #[test]
    fn layout_single_file() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file("a.rs", 1000, Some("rs")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(100.0, 100.0));

        assert_eq!(layout.rects.len(), 1);
        let r = &layout.rects[0];
        assert_eq!(r.node_index, 1);
        // Single file should fill the entire bounds.
        assert!(r.rect.width() > 99.0);
        assert!(r.rect.height() > 99.0);
    }

    #[test]
    fn layout_three_files_all_within_bounds() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file("a.rs", 1000, Some("rs")));
        tree.insert(0, make_file("b.txt", 500, Some("txt")));
        tree.insert(0, make_file("c.py", 300, Some("py")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(200.0, 100.0));

        assert_eq!(layout.rects.len(), 3);
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 100.0));
        for r in &layout.rects {
            assert!(
                bounds.contains_rect(r.rect),
                "rect {:?} not within bounds {:?}",
                r.rect,
                bounds,
            );
        }
    }

    #[test]
    fn layout_largest_file_gets_largest_area() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file("big.rs", 1000, Some("rs")));
        tree.insert(0, make_file("small.txt", 100, Some("txt")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(200.0, 100.0));

        assert_eq!(layout.rects.len(), 2);
        let big = layout.rects.iter().find(|r| r.node_index == 1).unwrap();
        let small = layout.rects.iter().find(|r| r.node_index == 2).unwrap();
        let big_area = big.rect.width() * big.rect.height();
        let small_area = small.rect.width() * small.rect.height();
        assert!(big_area > small_area);
    }

    #[test]
    fn layout_nested_directory_produces_leaf_rects() {
        let mut tree = DirTree::new("/root");
        let sub = tree.insert(0, make_dir("sub"));
        tree.insert(sub, make_file("a.rs", 1000, Some("rs")));
        tree.insert(0, make_file("b.txt", 500, Some("txt")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(200.0, 100.0));

        // Two leaf files: a.rs inside sub, and b.txt at root.
        assert_eq!(layout.rects.len(), 2);
        let indices: Vec<usize> = layout.rects.iter().map(|r| r.node_index).collect();
        assert!(indices.contains(&2)); // a.rs
        assert!(indices.contains(&3)); // b.txt
    }

    #[test]
    fn layout_zero_size_files_excluded() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file("a.rs", 1000, Some("rs")));
        tree.insert(0, make_file("empty.txt", 0, Some("txt")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(100.0, 100.0));

        // Only non-zero file appears.
        assert_eq!(layout.rects.len(), 1);
        assert_eq!(layout.rects[0].node_index, 1);
    }

    #[test]
    fn layout_empty_directory_produces_no_rects() {
        let tree = DirTree::new("/root");
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(100.0, 100.0));

        assert_eq!(layout.rects.len(), 0);
    }

    #[test]
    fn layout_colors_match_extension() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file("a.rs", 1000, Some("rs")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(100.0, 100.0));

        let expected_color = ext_stats::hsl_to_color32(
            &rds_core::stats::color_for_extension("rs"),
        );
        assert_eq!(layout.rects[0].color, expected_color);
    }

    #[test]
    fn layout_no_extension_uses_empty_string_color() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file("Makefile", 500, None));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(100.0, 100.0));

        let expected_color = ext_stats::hsl_to_color32(
            &rds_core::stats::color_for_extension(""),
        );
        assert_eq!(layout.rects[0].color, expected_color);
    }

    #[test]
    fn layout_no_zero_area_rects() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file("a.rs", 1000, Some("rs")));
        tree.insert(0, make_file("b.txt", 500, Some("txt")));
        tree.insert(0, make_file("c.py", 1, Some("py")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(100.0, 100.0));

        for r in &layout.rects {
            assert!(
                r.rect.width() > 0.0 && r.rect.height() > 0.0,
                "rect at index {} has zero area: {:?}",
                r.node_index,
                r.rect,
            );
        }
    }

    #[test]
    fn layout_zero_size_bounds_produces_no_rects() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file("a.rs", 1000, Some("rs")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(0.0, 0.0));

        assert_eq!(layout.rects.len(), 0);
    }

    #[test]
    fn layout_stores_last_size() {
        let tree = DirTree::new("/root");
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(300.0, 200.0));

        assert_eq!(layout.last_size, egui::vec2(300.0, 200.0));
    }

    #[test]
    fn layout_deeply_nested_files() {
        let mut tree = DirTree::new("/root");
        let d1 = tree.insert(0, make_dir("d1"));
        let d2 = tree.insert(d1, make_dir("d2"));
        let d3 = tree.insert(d2, make_dir("d3"));
        tree.insert(d3, make_file("deep.rs", 500, Some("rs")));
        tree.insert(0, make_file("top.txt", 500, Some("txt")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(200.0, 100.0));

        assert_eq!(layout.rects.len(), 2);
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 100.0));
        for r in &layout.rects {
            assert!(bounds.contains_rect(r.rect));
        }
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p rds-gui treemap -- --no-capture`
Expected: FAIL — `todo!()` panics in `TreemapLayout::compute`

- [ ] **Step 4: Implement TreemapLayout::compute and compute_recursive**

Replace the `todo!()` stub in `TreemapLayout::compute` and add the `compute_recursive` function. Replace the entire `impl TreemapLayout` block and add the helper function before `#[cfg(test)]`:

```rust
impl TreemapLayout {
    /// Computes a squarified treemap layout for all file nodes in `tree`.
    ///
    /// Coordinates are relative to a (0,0) origin with the given `size`.
    /// The caller offsets by the panel's actual position during rendering.
    /// (ref: DL-002)
    pub fn compute(tree: &DirTree, stats: &SubtreeStats, size: egui::Vec2) -> Self {
        let mut rects = Vec::new();
        if size.x > 0.0 && size.y > 0.0 {
            let bounds = streemap::Rect {
                x: 0.0,
                y: 0.0,
                w: size.x,
                h: size.y,
            };
            compute_recursive(tree, stats, tree.root(), bounds, &mut rects);
        }
        TreemapLayout {
            rects,
            last_size: size,
        }
    }
}

/// Recursively computes squarified layout for all descendants of `dir_index`.
///
/// At each directory level: gathers children with non-zero subtree size,
/// sorts descending (required by squarify), assigns proportional rectangles,
/// then recurses into directory children or records file children as
/// `TreemapRect` entries. (ref: DL-003, DL-006)
fn compute_recursive(
    tree: &DirTree,
    stats: &SubtreeStats,
    dir_index: usize,
    bounds: streemap::Rect<f32>,
    result: &mut Vec<TreemapRect>,
) {
    // Skip containers too small to produce visible rectangles. (ref: DL-010)
    if bounds.w < MIN_RECT_DIM || bounds.h < MIN_RECT_DIM {
        return;
    }

    let child_indices = tree.children(dir_index);
    let mut items: Vec<LayoutItem> = child_indices
        .iter()
        .filter_map(|&idx| {
            let size = stats.size(idx) as f32;
            if size > 0.0 {
                Some(LayoutItem {
                    size,
                    node_index: idx,
                    rect: streemap::Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 },
                })
            } else {
                None
            }
        })
        .collect();

    if items.is_empty() {
        return;
    }

    // Squarify requires items sorted by size descending.
    items.sort_by(|a, b| b.size.partial_cmp(&a.size).unwrap_or(std::cmp::Ordering::Equal));

    streemap::squarify(
        bounds,
        &mut items,
        |item| item.size,
        |item, r| item.rect = r,
    );

    for item in &items {
        let node = match tree.get(item.node_index) {
            Some(n) => n,
            None => continue,
        };

        if node.is_dir {
            // Recurse into subdirectory's assigned rectangle.
            compute_recursive(tree, stats, item.node_index, item.rect, result);
        } else {
            // Leaf file: record colored rectangle. (ref: DL-004)
            let ext = node.extension.as_deref().unwrap_or("");
            let color = ext_stats::hsl_to_color32(
                &rds_core::stats::color_for_extension(ext),
            );
            result.push(TreemapRect {
                node_index: item.node_index,
                rect: egui::Rect::from_min_size(
                    egui::pos2(item.rect.x, item.rect.y),
                    egui::vec2(item.rect.w, item.rect.h),
                ),
                color,
            });
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p rds-gui treemap -- --no-capture`
Expected: all 11 `layout_*` tests PASS

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings

- [ ] **Step 7: Commit**

```bash
git add crates/rds-gui/src/treemap.rs crates/rds-gui/src/lib.rs
git commit -m "MS8: add treemap layout computation with recursive squarify (TDD)"
```

---

### Task 2: Add show function with rendering, tooltip, and click selection

**Files:**
- Modify: `crates/rds-gui/src/treemap.rs`

- [ ] **Step 1: Add show function after compute_recursive, before `#[cfg(test)]`**

```rust
/// Renders the cached treemap layout with hover tooltips and click-to-select.
///
/// `layout` coordinates are relative to (0,0); this function offsets them
/// by the panel's allocated position. (ref: DL-002, DL-007, DL-008, DL-009)
pub(crate) fn show(
    layout: &TreemapLayout,
    tree: &DirTree,
    selected: &mut Option<usize>,
    ui: &mut egui::Ui,
) {
    let (response, painter) = ui.allocate_painter(
        layout.last_size,
        egui::Sense::click(),
    );
    let offset = response.rect.min.to_vec2();

    // Draw all rectangles. (ref: DL-007)
    for rect_info in &layout.rects {
        let abs_rect = rect_info.rect.translate(offset);
        // Inset by 0.5px for visual separation between adjacent rectangles.
        painter.rect_filled(abs_rect.shrink(0.5), 0.0, rect_info.color);
    }

    // Highlight selected node with a white border. (ref: DL-008)
    if let Some(sel_idx) = *selected {
        if let Some(hit) = layout.rects.iter().find(|r| r.node_index == sel_idx) {
            let abs_rect = hit.rect.translate(offset);
            painter.rect_stroke(
                abs_rect,
                0.0,
                egui::Stroke::new(2.0, egui::Color32::WHITE),
                egui::StrokeKind::Outside,
            );
        }
    }

    // Find which rectangle the pointer is hovering over.
    let hovered_index = response.hover_pos().and_then(|pos| {
        let rel = pos - offset;
        layout
            .rects
            .iter()
            .find(|r| r.rect.contains(rel))
            .map(|r| r.node_index)
    });

    // Hover tooltip: full path + human-readable size. (ref: DL-009)
    if let Some(idx) = hovered_index {
        if let Some(node) = tree.get(idx) {
            let path = tree.path(idx);
            egui::show_tooltip_at_pointer(
                ui.ctx(),
                ui.layer_id(),
                response.id.with("tip"),
                |ui| {
                    ui.label(path.display().to_string());
                    ui.label(crate::format_bytes(node.size));
                },
            );
        }
    }

    // Click to select node.
    if response.clicked() {
        *selected = hovered_index;
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p rds-gui`
Expected: compiles without errors

- [ ] **Step 3: Run all tests**

Run: `cargo test -p rds-gui -- --no-capture`
Expected: all tests pass (existing lib.rs tests + tree_view tests + ext_stats tests + treemap tests)

- [ ] **Step 4: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings

- [ ] **Step 5: Commit**

```bash
git add crates/rds-gui/src/treemap.rs
git commit -m "MS8: add treemap::show with rendering, tooltip, and click selection"
```

---

### Task 3: Wire TreemapLayout into RustDirStatApp

**Files:**
- Modify: `crates/rds-gui/src/lib.rs`

- [ ] **Step 1: Add treemap_layout field to RustDirStatApp struct**

In `crates/rds-gui/src/lib.rs`, add one field to the `RustDirStatApp` struct after `subtree_stats`:

```rust
    /// Cached treemap layout, computed after scan completes. Recomputed
    /// when the central panel resizes. (ref: DL-005)
    treemap_layout: Option<treemap::TreemapLayout>,
```

- [ ] **Step 2: Initialize new field in new()**

In the `Self { ... }` block inside `RustDirStatApp::new()`, add after `subtree_stats: None,`:

```rust
            treemap_layout: None,
```

- [ ] **Step 3: Reset treemap_layout in start_scan()**

In `start_scan()`, add after `self.subtree_stats = None;`:

```rust
        self.treemap_layout = None;
```

- [ ] **Step 4: Replace central panel placeholder with treemap rendering**

Replace the entire central panel block:

```rust
        // --- Central panel: treemap placeholder (MS8) ---
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Treemap");
            ui.separator();
            ui.colored_label(
                egui::Color32::GRAY,
                "Implemented in MS8.",
            );
        });
```

with:

```rust
        // --- Central panel: treemap (MS8) ---
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.tree.is_some() && self.subtree_stats.is_some() {
                // Recompute layout if panel size changed or layout not yet computed.
                // (ref: DL-005)
                let available_size = ui.available_size();
                let needs_recompute = self.treemap_layout.as_ref().map_or(true, |l| {
                    (l.last_size.x - available_size.x).abs() > 1.0
                        || (l.last_size.y - available_size.y).abs() > 1.0
                });

                if needs_recompute {
                    self.treemap_layout = Some(treemap::TreemapLayout::compute(
                        self.tree.as_ref().unwrap(),
                        self.subtree_stats.as_ref().unwrap(),
                        available_size,
                    ));
                }

                treemap::show(
                    self.treemap_layout.as_ref().unwrap(),
                    self.tree.as_ref().unwrap(),
                    &mut self.selected_node,
                    ui,
                );
            } else {
                ui.heading("Treemap");
                ui.separator();
                if matches!(self.phase, ScanPhase::Scanning) {
                    ui.colored_label(
                        egui::Color32::GRAY,
                        "Scan in progress\u{2026}",
                    );
                } else {
                    ui.colored_label(egui::Color32::GRAY, "No scan data.");
                }
            }
        });
```

- [ ] **Step 5: Update default_is_idle test**

In the `default_is_idle` test, add an assertion for the new field after the existing `assert!(app.subtree_stats.is_none());`:

```rust
        assert!(app.treemap_layout.is_none());
```

- [ ] **Step 6: Run all tests**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 7: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings

- [ ] **Step 8: Commit**

```bash
git add crates/rds-gui/src/lib.rs
git commit -m "MS8: wire treemap layout into central panel with lazy recompute"
```

---

### Task 4: Manual verification

- [ ] **Step 1: Launch the app and scan a directory with mixed file types**

Run: `cargo run -- /usr/share` (or another directory with many file types)
Expected:
- During scan: central panel shows "Treemap" heading with "Scan in progress..." label
- After scan completes: central panel fills with colored rectangles
- Rectangles cover the full panel area with thin gaps between them

- [ ] **Step 2: Verify rectangle colors**

Expected:
- Files with the same extension have the same color
- Different extensions have visibly different colors
- Colors match the swatches in the extension statistics panel (if MS7 is wired)

- [ ] **Step 3: Verify hover tooltip**

Expected:
- Hovering over a rectangle shows a tooltip at the pointer position
- Tooltip displays the full file path (e.g., `/usr/share/doc/readme.txt`)
- Tooltip displays the human-readable file size (e.g., `4.2 KB`)
- Moving to a different rectangle updates the tooltip
- Moving off all rectangles hides the tooltip

- [ ] **Step 4: Verify click selection**

Expected:
- Clicking a rectangle highlights it with a white border (2px)
- Clicking a different rectangle moves the highlight
- The selected node index updates (visible in tree view if MS6 selection is wired)

- [ ] **Step 5: Verify layout caching on resize**

Expected:
- Resizing the window causes the treemap to re-layout smoothly
- Layout adapts to the new panel dimensions
- No visual flicker or delay during resize

- [ ] **Step 6: Verify re-scan**

Expected:
- After scan completes, pick a different directory and scan again
- Treemap resets during new scan (shows "Scan in progress...")
- After new scan completes, fresh treemap appears for the new directory

- [ ] **Step 7: Commit if any fixes were needed**

```bash
git add -u
git commit -m "MS8: fix issues found during manual verification"
```

---

### Task 5: Update documentation

**Files:**
- Modify: `crates/rds-gui/CLAUDE.md`
- Modify: `plans/CLAUDE.md`
- Modify: `docs/milestones.md`

- [ ] **Step 1: Update rds-gui CLAUDE.md**

Replace the content of `crates/rds-gui/CLAUDE.md` with:

```markdown
# crates/rds-gui/

egui/eframe GUI shell with directory picker, scanner integration, directory tree view, extension statistics, treemap renderer, and panel layout.

## Files

| File | What | When to read |
| ---- | ---- | ------------ |
| `Cargo.toml` | Crate manifest; depends on `eframe`, `egui`, `streemap`, `crossbeam-channel`, `tracing`, `rds-core`, `rds-scanner`, `rfd` | Modifying GUI dependencies |
| `src/lib.rs` | `RustDirStatApp` with ScanPhase state machine, directory picker (rfd), scanner spawning, event drain loop, 3-panel layout with tree view + treemap + ext stats, format_bytes utility | Modifying app state, scan lifecycle, layout, adding panel implementations |
| `src/tree_view.rs` | `SubtreeStats` (cached subtree sizes/file counts), `TreeViewState` (expanded nodes), `sorted_children`, tree view rendering (show/render_node) | Modifying tree view display, adding tree interactions, understanding size caching strategy |
| `src/ext_stats.rs` | `hsl_to_color32` (HSL→Color32 conversion), `show` (extension stats panel with stacked bar chart and scrollable Grid table) | Modifying extension statistics display, adding bar chart interactions, reusing hsl_to_color32 for treemap coloring |
| `src/treemap.rs` | `TreemapRect`, `TreemapLayout` (cached layout), `compute_recursive` (recursive squarify), `show` (render + tooltip + click selection) | Modifying treemap rendering, adding cushion shading (MS9), adding drill-down navigation (MS10) |
```

- [ ] **Step 2: Update plans/CLAUDE.md**

Add entry to the file table:

```markdown
| `ms8-treemap-layout-flat.md` | MS8 plan: flat treemap layout with recursive squarify, cached layout, click/hover interaction (completed) | Reviewing treemap layout approach, understanding caching strategy, understanding rendering pipeline |
```

- [ ] **Step 3: Update milestone status in docs/milestones.md**

Change `## MS8 — Treemap Layout (Flat)` from active to done:

```markdown
## ~~MS8 — Treemap Layout (Flat)~~ DONE
```

- [ ] **Step 4: Commit**

```bash
git add crates/rds-gui/CLAUDE.md plans/CLAUDE.md plans/ms8-treemap-layout-flat.md docs/milestones.md
git commit -m "MS8: update docs for flat treemap layout completion"
```
