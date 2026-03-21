# MS10: Panel Synchronization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire up cross-panel selection so all three panels (tree view, treemap, extension stats) are fully synchronized, with treemap drill-down navigation via double-click and breadcrumb.

**Architecture:** Two independent selection mechanisms coexist: `selected_node: Option<usize>` (individual node, shared between tree view and treemap) and `selected_extension: Option<String>` (extension filter from ext stats clicks, highlights matching files in treemap). Tree view auto-expands ancestors and scrolls to the selected node when selection changes externally (e.g., treemap click). Treemap supports drill-down via `treemap_root: usize` (defaults to `tree.root()`), changed by double-click and navigated via a breadcrumb bar. Layout caching adds `last_root` to invalidation checks alongside the existing size check.

**Tech Stack:** egui 0.33 (selectable_label, ScrollArea::scroll_to_me, Response::double_clicked, Sense::click), rds-core (DirTree, FileNode, ExtensionStats), rds-gui (TreeViewState, TreemapLayout, SubtreeStats, ext_stats, treemap, format_bytes)

---

## Planning Context

### Decision Log

| ID | Decision | Reasoning |
|---|---|---|
| DL-001 | `selected_node` and `selected_extension` are independent, both can be active simultaneously | Node selection (white border) and extension filter (dimming) serve different purposes. Keeping them independent lets the user select a specific file while also filtering by extension. Mutual exclusivity would force clearing the extension filter on every node click, which is disruptive during exploration. **Deliberate spec deviation:** the milestone description says "click in tree view highlights in ext stats" — this plan does NOT auto-highlight the selected node's extension row in ext stats. Automatically coupling node selection to extension highlighting would be surprising (selecting a file shouldn't feel like activating a filter). If this proves unintuitive, a future enhancement can derive a "soft highlight" in ext stats from the selected node's extension without setting `selected_extension`. |
| DL-002 | Extension highlighting dims non-matching rects to 30% brightness instead of overlaying | Modifying rect color before painting is simpler and more performant than drawing transparent overlays. 30% brightness keeps non-matching files visible but clearly subordinate. Compatible with future cushion shading (MS9) since the color modification happens at paint time. |
| DL-003 | Tree view auto-expands ancestors and scrolls to selection on external change only | When the user clicks in the tree view, ancestors are already expanded (the node was visible to be clicked). Expand + scroll is only needed when selection comes from treemap. Detecting "external change" via `last_synced_selection` tracking avoids redundant work without needing an event system. |
| DL-004 | `expand_ancestors` is idempotent — safe to call on tree view's own selections | Even if called unnecessarily, it only expands nodes that are already expanded. O(depth) cost (~20 parent lookups) is negligible. This avoids complex "who changed the selection" tracking. |
| DL-005 | Treemap drill-down finds the direct child of `treemap_root` that is an ancestor of the clicked file | Progressive one-level-at-a-time drill-down matches WinDirStat behavior. Double-clicking a deeply nested file doesn't jump multiple levels — it enters the first subdirectory containing that file. Repeated double-clicks go deeper. |
| DL-006 | `treemap_root: usize` defaults to 0 (tree root), stored on `RustDirStatApp` | Simple integer reference into the arena. Reset to 0 on new scan. Treemap layout caching adds `last_root` field for invalidation. No need for a wrapper type — `usize` matches the arena index convention used everywhere. |
| DL-007 | Breadcrumb rendered as horizontal clickable labels in the central panel, above the treemap | Only visible when `treemap_root != tree.root()` (drilled in). Clicking a breadcrumb component sets treemap_root to that ancestor. Simple egui horizontal layout — no custom widget needed. |
| DL-008 | Ext stats click uses `selectable_label` for table rows and `Sense::click()` for bar segments | `selectable_label` provides built-in visual feedback (highlight on selection) matching the tree view pattern. Bar chart segments already have `allocate_rect` — changing sense from hover-only to click adds click detection without breaking existing tooltips. |
| DL-009 | `TreemapLayout::compute` takes `root_index` parameter instead of hardcoding `tree.root()` | Clean API change. Existing callers pass `tree.root()` for backward compatibility. Drill-down callers pass the subdirectory index. Layout cache stores `last_root` alongside `last_size` for invalidation. |

### Constraints

- rds-core zero-dep invariant (beyond serde) must not be violated; no changes to rds-core in this milestone
- All existing tests must pass (`cargo test --workspace`)
- No new crate dependencies required
- Real filesystem I/O only in integration tests, no mocks
- MS8 code (treemap.rs, lib.rs, tree_view.rs, ext_stats.rs) must exist as the starting point
- `selected_node: Option<usize>` already exists and is shared between tree view and treemap — extend, don't replace

### Known Risks

- **`scroll_to_me` may not scroll precisely on the first frame after expansion**: egui's scroll area might need one extra frame to lay out newly expanded nodes before scrolling to the target. If this occurs, the scroll will land correctly on the second frame — acceptable for this use case.
- **Bar chart segments with `Sense::click()` may steal focus from tooltips**: If hover tooltips disappear when click sense is added, fall back to `Sense::click() | Sense::hover()` or separate the tooltip onto a hover-only overlay.
- **Double-click fires `clicked()` on both first and second clicks**: The first click selects the node (via `clicked()`), the second click drills down (via `double_clicked()`) and also fires `clicked()` again (re-selecting the same node). This is harmless — the node stays selected during drill-down.

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/rds-gui/src/lib.rs` | Modify | Add `selected_extension`, `treemap_root` fields; update init/reset; update ext_stats/treemap wiring with new params; add breadcrumb rendering in central panel; detect treemap_root changes for layout invalidation |
| `crates/rds-gui/src/tree_view.rs` | Modify | Add `expand_ancestors` function; add `last_synced_selection`/`pending_scroll` fields to `TreeViewState`; update `show`/`render_node` for auto-expand and scroll-to-selection |
| `crates/rds-gui/src/ext_stats.rs` | Modify | Update `show` signature to accept `&mut Option<String>`; add click handlers to bar chart segments and table rows; highlight selected extension row |
| `crates/rds-gui/src/treemap.rs` | Modify | Update `show` signature for extension highlighting + treemap_root; add rect dimming for extension filter; add double-click handler; update `TreemapLayout::compute` with `root_index` param; add `last_root` field; add `find_drill_target` and `breadcrumb_chain` helpers with unit tests |

---

### Task 1: Add new state fields to RustDirStatApp

**Files:**
- Modify: `crates/rds-gui/src/lib.rs`

- [ ] **Step 1: Add `selected_extension` and `treemap_root` fields to the struct**

In `crates/rds-gui/src/lib.rs`, add two fields to `RustDirStatApp` after `treemap_layout`:

```rust
    /// Extension filter: when set, treemap dims files not matching this extension.
    /// Set by clicking in ext stats panel, independent of `selected_node`. (ref: DL-001)
    selected_extension: Option<String>,
    /// Root node index for treemap drill-down. Defaults to `tree.root()` (0).
    /// Changed by double-click in treemap, navigated via breadcrumb. (ref: DL-006)
    treemap_root: usize,
```

- [ ] **Step 2: Initialize new fields in `new()`**

In the `Self { ... }` block inside `RustDirStatApp::new()`, add after `treemap_layout: None,`:

```rust
            selected_extension: None,
            treemap_root: 0,
```

- [ ] **Step 3: Reset new fields in `start_scan()`**

In `start_scan()`, add after `self.treemap_layout = None;`:

```rust
        self.selected_extension = None;
        self.treemap_root = 0;
```

- [ ] **Step 4: Update `default_is_idle` test**

Add assertions for the new fields after `assert!(app.treemap_layout.is_none());`:

```rust
        assert!(app.selected_extension.is_none());
        assert_eq!(app.treemap_root, 0);
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p rds-gui -- --no-capture`
Expected: all tests pass including updated `default_is_idle`

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings

- [ ] **Step 7: Commit**

```bash
git add crates/rds-gui/src/lib.rs
git commit -m "MS10: add selected_extension and treemap_root state fields"
```

---

### Task 2: Tree view auto-expand ancestors and scroll-to-selection (TDD)

**Files:**
- Modify: `crates/rds-gui/src/tree_view.rs`

- [ ] **Step 1: Add `last_synced_selection` and `pending_scroll` fields to `TreeViewState`**

Update the struct definition:

```rust
pub(crate) struct TreeViewState {
    expanded: HashSet<usize>,
    /// Tracks the last selection value synced from external sources (e.g., treemap click).
    /// Used to detect when selection changed externally. (ref: DL-003)
    last_synced_selection: Option<usize>,
    /// When true, the next render of the selected node calls `scroll_to_me`. (ref: DL-003)
    pending_scroll: bool,
}
```

- [ ] **Step 2: Update `new()` and `reset()` for new fields**

```rust
    pub fn new() -> Self {
        Self {
            expanded: HashSet::new(),
            last_synced_selection: None,
            pending_scroll: false,
        }
    }

    pub fn reset(&mut self) {
        self.expanded.clear();
        self.last_synced_selection = None;
        self.pending_scroll = false;
    }
```

- [ ] **Step 3: Add `expand_ancestors` function and write failing tests**

Add the function before `show()`:

```rust
/// Expands all ancestor directories of `index` so the node becomes visible
/// in the tree view. Walks from `index` up to the root via parent pointers,
/// expanding each parent. Idempotent — already-expanded nodes stay expanded.
/// (ref: DL-004)
fn expand_ancestors(tree: &DirTree, state: &mut TreeViewState, index: usize) {
    let mut current = index;
    while let Some(node) = tree.get(current) {
        if let Some(parent) = node.parent {
            state.expand(parent);
            current = parent;
        } else {
            break;
        }
    }
}
```

Add tests in the `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn expand_ancestors_deep_node() {
        let mut tree = DirTree::new("/root");
        let d1 = tree.insert(0, make_dir("d1"));
        let d2 = tree.insert(d1, make_dir("d2"));
        let d3 = tree.insert(d2, make_dir("d3"));
        let file = tree.insert(d3, make_file("deep.txt", 100));

        let mut state = TreeViewState::new();
        expand_ancestors(&tree, &mut state, file);

        assert!(state.is_expanded(0));   // root
        assert!(state.is_expanded(d1));  // d1
        assert!(state.is_expanded(d2));  // d2
        assert!(state.is_expanded(d3));  // d3
    }

    #[test]
    fn expand_ancestors_root_is_noop() {
        let tree = DirTree::new("/root");
        let mut state = TreeViewState::new();
        expand_ancestors(&tree, &mut state, 0);
        // Root has no parent — nothing to expand.
        assert!(!state.is_expanded(0));
    }

    #[test]
    fn expand_ancestors_direct_child_of_root() {
        let mut tree = DirTree::new("/root");
        let file = tree.insert(0, make_file("a.txt", 100));

        let mut state = TreeViewState::new();
        expand_ancestors(&tree, &mut state, file);

        // Only root should be expanded (parent of the file).
        assert!(state.is_expanded(0));
    }

    #[test]
    fn expand_ancestors_idempotent() {
        let mut tree = DirTree::new("/root");
        let d1 = tree.insert(0, make_dir("d1"));
        let file = tree.insert(d1, make_file("a.txt", 100));

        let mut state = TreeViewState::new();
        expand_ancestors(&tree, &mut state, file);
        expand_ancestors(&tree, &mut state, file);

        assert!(state.is_expanded(0));
        assert!(state.is_expanded(d1));
    }
```

- [ ] **Step 4: Run tests to verify expand_ancestors works**

Run: `cargo test -p rds-gui tree_view -- --no-capture`
Expected: all tests pass including the new expand_ancestors tests

- [ ] **Step 5: Update `show()` to detect external selection changes**

Replace the `show` function body:

```rust
pub(crate) fn show(
    tree: &DirTree,
    stats: &SubtreeStats,
    state: &mut TreeViewState,
    selected: &mut Option<usize>,
    ui: &mut egui::Ui,
) {
    // Detect external selection change (e.g., treemap click).
    // Expand ancestors and queue scroll so the selected node becomes visible. (ref: DL-003)
    if *selected != state.last_synced_selection {
        if let Some(idx) = *selected {
            expand_ancestors(tree, state, idx);
            state.pending_scroll = true;
        }
        state.last_synced_selection = *selected;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        render_node(tree, tree.root(), stats, state, selected, ui, 0);
    });
}
```

- [ ] **Step 6: Update `render_node()` click handler to sync `last_synced_selection`**

In `render_node`, update the click handler to also update `last_synced_selection`, preventing the expand_ancestors call on the next frame for the tree view's own clicks:

Replace:

```rust
        let response = ui.selectable_label(is_selected, &label_text);
        if response.clicked() {
            *selected = Some(index);
        }
```

With:

```rust
        let response = ui.selectable_label(is_selected, &label_text);
        if response.clicked() {
            *selected = Some(index);
            // Update sync tracker immediately so show() doesn't treat this as
            // an external change on the next frame. (ref: DL-004)
            state.last_synced_selection = Some(index);
        }
        // Scroll to the selected node when selection changed externally.
        if is_selected && state.pending_scroll {
            response.scroll_to_me(Some(egui::Align::Center));
            state.pending_scroll = false;
        }
```

- [ ] **Step 7: Run all tests**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 8: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings

- [ ] **Step 9: Commit**

```bash
git add crates/rds-gui/src/tree_view.rs
git commit -m "MS10: tree view auto-expands ancestors and scrolls to externally selected node"
```

---

### Task 3: Extension stats click handling

**Files:**
- Modify: `crates/rds-gui/src/ext_stats.rs`
- Modify: `crates/rds-gui/src/lib.rs`

- [ ] **Step 1: Update `ext_stats::show()` signature to accept selected_extension**

Change the function signature in `crates/rds-gui/src/ext_stats.rs`:

```rust
pub(crate) fn show(
    ext_stats: &[ExtensionStats],
    selected_extension: &mut Option<String>,
    ui: &mut egui::Ui,
)
```

- [ ] **Step 2: Add click handling to bar chart segments**

Replace the bar chart segment interaction block. Change `egui::Sense::hover()` to `egui::Sense::click()` on the segment response, and add a click handler:

Replace:

```rust
        let segment_response =
            ui.allocate_rect(segment, egui::Sense::hover());
        segment_response.on_hover_text(hover_text);
```

With:

```rust
        let segment_response =
            ui.allocate_rect(segment, egui::Sense::click());
        segment_response.on_hover_text(&hover_text);
        if segment_response.clicked() {
            let ext = stat.extension.clone();
            if *selected_extension == Some(ext.clone()) {
                *selected_extension = None; // toggle off
            } else {
                *selected_extension = Some(ext);
            }
        }
```

- [ ] **Step 3: Add click handling to table rows with `selectable_label`**

In the data rows section of the Grid, replace `ui.label(display_name);` with a clickable selectable_label:

Replace:

```rust
                    ui.label(display_name);
```

With:

```rust
                    let is_ext_selected =
                        selected_extension.as_deref() == Some(stat.extension.as_str());
                    if ui.selectable_label(is_ext_selected, display_name).clicked() {
                        if is_ext_selected {
                            *selected_extension = None;
                        } else {
                            *selected_extension = Some(stat.extension.clone());
                        }
                    }
```

- [ ] **Step 4: Update lib.rs wiring to pass `selected_extension` to ext_stats**

In `crates/rds-gui/src/lib.rs`, update the ext_stats::show call in the right panel:

Replace:

```rust
                        ext_stats::show(stats, ui);
```

With:

```rust
                        ext_stats::show(stats, &mut self.selected_extension, ui);
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p rds-gui`
Expected: compiles without errors

- [ ] **Step 6: Run all tests**

Run: `cargo test --workspace`
Expected: all tests pass (ext_stats tests don't call `show()` so they're unaffected)

- [ ] **Step 7: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings

- [ ] **Step 8: Commit**

```bash
git add crates/rds-gui/src/ext_stats.rs crates/rds-gui/src/lib.rs
git commit -m "MS10: add click-to-select extension in stats panel with toggle"
```

---

### Task 4: Treemap extension highlighting

**Files:**
- Modify: `crates/rds-gui/src/treemap.rs`
- Modify: `crates/rds-gui/src/lib.rs`

- [ ] **Step 1: Update `treemap::show()` signature to accept highlighted extension**

In `crates/rds-gui/src/treemap.rs`, change the `show` function signature:

```rust
pub(crate) fn show(
    layout: &TreemapLayout,
    tree: &DirTree,
    selected: &mut Option<usize>,
    highlighted_extension: &Option<String>,
    ui: &mut egui::Ui,
)
```

- [ ] **Step 2: Add extension dimming to the rect rendering loop**

The rendering loop has two branches (MS9 cushion mesh and flat fill). Add a `dim_color` helper before the loop and apply dimming in both branches.

Add a helper closure at the start of the `show` function body, after `let offset = ...;`:

```rust
    // Helper: dim a color to 30% brightness when extension filter is active
    // and the node doesn't match. (ref: DL-002)
    let effective_color = |rect_info: &TreemapRect| -> egui::Color32 {
        if let Some(ext) = highlighted_extension {
            let matches = tree
                .get(rect_info.node_index)
                .map_or(false, |n| n.extension.as_deref().unwrap_or("") == ext.as_str());
            if matches {
                rect_info.color
            } else {
                let [r, g, b, a] = rect_info.color.to_array();
                egui::Color32::from_rgba_premultiplied(
                    (r as f32 * 0.3) as u8,
                    (g as f32 * 0.3) as u8,
                    (b as f32 * 0.3) as u8,
                    a,
                )
            }
        } else {
            rect_info.color
        }
    };
```

Then replace the rendering loop:

```rust
    for rect_info in &layout.rects {
        let w = rect_info.rect.width();
        let h = rect_info.rect.height();

        if w >= MIN_CUSHION_DIM && h >= MIN_CUSHION_DIM {
            // Cushion shading via mesh. (ref: DL-002, DL-005)
            build_cushion_mesh(
                &mut cushion_mesh,
                rect_info.rect.shrink(0.5),
                offset,
                &rect_info.cushion,
                rect_info.color,
            );
        } else {
            // Flat fill for tiny rects. (ref: DL-005)
            let abs_rect = rect_info.rect.translate(offset);
            painter.rect_filled(abs_rect.shrink(0.5), 0.0, rect_info.color);
        }
    }
```

With:

```rust
    for rect_info in &layout.rects {
        let w = rect_info.rect.width();
        let h = rect_info.rect.height();
        let color = effective_color(rect_info);

        if w >= MIN_CUSHION_DIM && h >= MIN_CUSHION_DIM {
            // Cushion shading via mesh. (ref: MS9 DL-002, MS10 DL-002)
            build_cushion_mesh(
                &mut cushion_mesh,
                rect_info.rect.shrink(0.5),
                offset,
                &rect_info.cushion,
                color,
            );
        } else {
            // Flat fill for tiny rects. (ref: MS9 DL-005)
            let abs_rect = rect_info.rect.translate(offset);
            painter.rect_filled(abs_rect.shrink(0.5), 0.0, color);
        }
    }
```

- [ ] **Step 3: Update lib.rs wiring to pass `selected_extension` to treemap**

In `crates/rds-gui/src/lib.rs`, update the treemap::show call:

Replace:

```rust
                    treemap::show(layout, tree, &mut self.selected_node, ui);
```

With:

```rust
                    treemap::show(
                        layout,
                        tree,
                        &mut self.selected_node,
                        &self.selected_extension,
                        ui,
                    );
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p rds-gui`
Expected: compiles without errors

- [ ] **Step 5: Run all tests**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings

- [ ] **Step 7: Commit**

```bash
git add crates/rds-gui/src/treemap.rs crates/rds-gui/src/lib.rs
git commit -m "MS10: treemap dims non-matching files when extension filter is active"
```

---

### Task 5: Treemap drill-down — custom root and drill target helper (TDD)

**Files:**
- Modify: `crates/rds-gui/src/treemap.rs`

- [ ] **Step 1: Add `last_root` field to `TreemapLayout`**

Update the struct:

```rust
pub(crate) struct TreemapLayout {
    pub rects: Vec<TreemapRect>,
    pub last_size: egui::Vec2,
    /// Root node used for this layout computation. Used for cache invalidation
    /// when treemap_root changes via drill-down. (ref: DL-009)
    pub last_root: usize,
}
```

- [ ] **Step 2: Update `TreemapLayout::compute` to accept `root_index` parameter**

Change the signature and implementation. The internal `compute_recursive` call already has cushion params from MS9 — only change `tree.root()` to `root_index`:

```rust
    pub fn compute(
        tree: &DirTree,
        stats: &SubtreeStats,
        size: egui::Vec2,
        root_index: usize,
    ) -> Self {
        let mut rects = Vec::new();
        if size.x > 0.0 && size.y > 0.0 {
            let bounds = streemap::Rect {
                x: 0.0,
                y: 0.0,
                w: size.x,
                h: size.y,
            };
            compute_recursive(
                tree, stats, root_index, bounds,
                CushionCoeffs::default(), INITIAL_HEIGHT, 0, &mut rects,
            );
        }
        TreemapLayout {
            rects,
            last_size: size,
            last_root: root_index,
        }
    }
```

- [ ] **Step 3: Update all existing test calls to pass `tree.root()`**

In the `#[cfg(test)] mod tests` block, update every `TreemapLayout::compute(...)` call. Add `tree.root()` as the fourth argument. For example:

```rust
// Before:
let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0));
// After:
let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0), tree.root());
```

Apply this to all existing test functions that call `TreemapLayout::compute`:
- `layout_single_file`
- `layout_three_files_within_bounds`
- `layout_largest_gets_largest_area`
- `layout_nested_directory`
- `layout_zero_size_excluded`
- `layout_empty_directory`
- `layout_color_matches_extension`
- `layout_no_extension_color`
- `layout_no_zero_area_rects`
- `layout_zero_size_bounds`
- `layout_deeply_nested_files`
- `layout_stores_last_size`
- `layout_tracks_depth` (MS9)
- `layout_deeply_nested_depth` (MS9)
- `layout_cushion_accumulates_across_levels` (MS9)
- `layout_cushion_coefficients_nonzero` (MS9)
- `performance_50k_layout_and_mesh` (MS9)

- [ ] **Step 4: Write new tests for custom root and drill target**

Add these tests to the existing test module:

```rust
    #[test]
    fn layout_with_custom_root_shows_subtree_only() {
        let mut tree = DirTree::new("/root");
        let sub = tree.insert(0, make_dir("sub"));
        tree.insert(sub, make_file("a.rs", 500, Some("rs")));
        tree.insert(sub, make_file("b.rs", 300, Some("rs")));
        tree.insert(0, make_file("top.txt", 200, Some("txt")));

        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0), sub);

        // Only files inside `sub` appear.
        assert_eq!(layout.rects.len(), 2);
        let indices: Vec<usize> = layout.rects.iter().map(|r| r.node_index).collect();
        assert!(indices.contains(&2)); // a.rs
        assert!(indices.contains(&3)); // b.rs
        assert!(!indices.contains(&4)); // top.txt excluded
        assert_eq!(layout.last_root, sub);
    }

    #[test]
    fn layout_custom_root_stores_last_root() {
        let mut tree = DirTree::new("/root");
        let sub = tree.insert(0, make_dir("sub"));
        tree.insert(sub, make_file("a.rs", 100, Some("rs")));
        let stats = SubtreeStats::compute(&tree);

        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0), sub);
        assert_eq!(layout.last_root, sub);
    }

    #[test]
    fn drill_target_file_inside_subdir() {
        let mut tree = DirTree::new("/root");
        let sub = tree.insert(0, make_dir("sub"));
        let file = tree.insert(sub, make_file("a.rs", 100, Some("rs")));

        // File is inside `sub`, which is a direct child of root.
        // Drilling from root should target `sub`.
        assert_eq!(find_drill_target(&tree, file, 0), Some(sub));
    }

    #[test]
    fn drill_target_file_at_top_level_returns_none() {
        let mut tree = DirTree::new("/root");
        let file = tree.insert(0, make_file("a.rs", 100, Some("rs")));

        // File is a direct child of root — nothing deeper to drill into.
        assert_eq!(find_drill_target(&tree, file, 0), None);
    }

    #[test]
    fn drill_target_deeply_nested_file() {
        let mut tree = DirTree::new("/root");
        let d1 = tree.insert(0, make_dir("d1"));
        let d2 = tree.insert(d1, make_dir("d2"));
        let file = tree.insert(d2, make_file("deep.rs", 100, Some("rs")));

        // From root: direct child ancestor of file is d1.
        assert_eq!(find_drill_target(&tree, file, 0), Some(d1));
        // From d1: direct child ancestor of file is d2.
        assert_eq!(find_drill_target(&tree, file, d1), Some(d2));
        // From d2: file is a direct child — can't drill deeper.
        assert_eq!(find_drill_target(&tree, file, d2), None);
    }

    #[test]
    fn breadcrumb_chain_at_root() {
        let tree = DirTree::new("/root");
        let chain = breadcrumb_chain(&tree, 0);
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0], (0, "/root".to_string()));
    }

    #[test]
    fn breadcrumb_chain_one_level_deep() {
        let mut tree = DirTree::new("/root");
        let sub = tree.insert(0, make_dir("sub"));
        let chain = breadcrumb_chain(&tree, sub);
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0], (0, "/root".to_string()));
        assert_eq!(chain[1], (sub, "sub".to_string()));
    }

    #[test]
    fn breadcrumb_chain_three_levels() {
        let mut tree = DirTree::new("/root");
        let d1 = tree.insert(0, make_dir("d1"));
        let d2 = tree.insert(d1, make_dir("d2"));
        let d3 = tree.insert(d2, make_dir("d3"));
        let chain = breadcrumb_chain(&tree, d3);
        assert_eq!(chain.len(), 4);
        assert_eq!(chain[0], (0, "/root".to_string()));
        assert_eq!(chain[1], (d1, "d1".to_string()));
        assert_eq!(chain[2], (d2, "d2".to_string()));
        assert_eq!(chain[3], (d3, "d3".to_string()));
    }
```

- [ ] **Step 5: Add `find_drill_target` and `breadcrumb_chain` helper functions**

Add before the `show` function:

```rust
/// Finds the directory to drill into when the user double-clicks a file.
///
/// Walks from `file_idx` up toward `current_root` to find the direct child
/// of `current_root` that is an ancestor of the file. Returns `Some(dir_index)`
/// if that child is a directory (drill target), or `None` if the file is a
/// direct child of the current root (nothing deeper to drill into). (ref: DL-005)
pub(crate) fn find_drill_target(
    tree: &DirTree,
    file_idx: usize,
    current_root: usize,
) -> Option<usize> {
    let mut current = file_idx;
    loop {
        let node = tree.get(current)?;
        let parent = node.parent?;
        if parent == current_root {
            // `current` is a direct child of the treemap root.
            if tree.get(current)?.is_dir {
                return Some(current);
            } else {
                return None;
            }
        }
        current = parent;
    }
}

/// Builds the ancestor chain from the tree root down to `treemap_root`.
///
/// Returns a list of `(node_index, node_name)` pairs ordered from root to
/// `treemap_root`. Used for rendering the breadcrumb navigation bar. (ref: DL-007)
pub(crate) fn breadcrumb_chain(tree: &DirTree, treemap_root: usize) -> Vec<(usize, String)> {
    let mut chain = Vec::new();
    let mut current = treemap_root;
    loop {
        let node = match tree.get(current) {
            Some(n) => n,
            None => break,
        };
        chain.push((current, node.name.clone()));
        match node.parent {
            Some(parent) => current = parent,
            None => break,
        }
    }
    chain.reverse();
    chain
}
```

- [ ] **Step 6: Run tests to verify everything passes**

Run: `cargo test -p rds-gui treemap -- --no-capture`
Expected: all existing tests pass (with updated signatures) + all new tests pass

- [ ] **Step 7: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings

- [ ] **Step 8: Commit**

```bash
git add crates/rds-gui/src/treemap.rs
git commit -m "MS10: TreemapLayout with custom root, drill target, and breadcrumb helpers (TDD)"
```

---

### Task 6: Double-click handler, breadcrumb rendering, and wiring

**Files:**
- Modify: `crates/rds-gui/src/treemap.rs`
- Modify: `crates/rds-gui/src/lib.rs`

- [ ] **Step 1: Update `treemap::show()` to accept `treemap_root` and handle double-click**

Update the `show` function signature:

```rust
pub(crate) fn show(
    layout: &TreemapLayout,
    tree: &DirTree,
    selected: &mut Option<usize>,
    highlighted_extension: &Option<String>,
    treemap_root: &mut usize,
    ui: &mut egui::Ui,
)
```

Add double-click handling after the existing click handler:

Replace:

```rust
    // Click to select node.
    if response.clicked() {
        *selected = hovered_index;
    }
```

With:

```rust
    // Click to select node.
    if response.clicked() {
        *selected = hovered_index;
    }

    // Double-click to drill into subdirectory. (ref: DL-005)
    if response.double_clicked() {
        if let Some(idx) = hovered_index {
            if let Some(target) = find_drill_target(tree, idx, *treemap_root) {
                *treemap_root = target;
            }
        }
    }
```

- [ ] **Step 2: Update lib.rs central panel to pass `treemap_root` and handle root changes**

Replace the entire central panel block in `update()`:

```rust
        // --- Central panel: treemap (MS8) + breadcrumb (MS10) ---
        egui::CentralPanel::default().show(ctx, |ui| {
            if let (Some(tree), Some(stats)) =
                (self.tree.as_ref(), self.subtree_stats.as_ref())
            {
                // Breadcrumb navigation — only visible when drilled in. (ref: DL-007)
                if self.treemap_root != tree.root() {
                    ui.horizontal(|ui| {
                        let chain = treemap::breadcrumb_chain(tree, self.treemap_root);
                        for (i, (idx, name)) in chain.iter().enumerate() {
                            if i > 0 {
                                ui.label("\u{203A}"); // › separator
                            }
                            if *idx == self.treemap_root {
                                // Current directory: non-clickable label.
                                ui.strong(name);
                            } else if ui.link(name).clicked() {
                                self.treemap_root = *idx;
                                self.treemap_layout = None;
                            }
                        }
                    });
                    ui.separator();
                }

                // Recompute layout if panel size or root changed. (ref: MS8 DL-005, MS10 DL-009)
                let available_size = ui.available_size();
                let needs_recompute = self.treemap_layout.as_ref().is_none_or(|l| {
                    l.last_root != self.treemap_root
                        || (l.last_size.x - available_size.x).abs() > 1.0
                        || (l.last_size.y - available_size.y).abs() > 1.0
                });

                if needs_recompute {
                    self.treemap_layout = Some(treemap::TreemapLayout::compute(
                        tree,
                        stats,
                        available_size,
                        self.treemap_root,
                    ));
                }

                if let Some(layout) = self.treemap_layout.as_ref() {
                    let prev_root = self.treemap_root;
                    treemap::show(
                        layout,
                        tree,
                        &mut self.selected_node,
                        &self.selected_extension,
                        &mut self.treemap_root,
                        ui,
                    );
                    // Invalidate layout if drill-down changed the root.
                    if self.treemap_root != prev_root {
                        self.treemap_layout = None;
                    }
                }
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

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p rds-gui`
Expected: compiles without errors

- [ ] **Step 4: Run all tests**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings

- [ ] **Step 6: Commit**

```bash
git add crates/rds-gui/src/treemap.rs crates/rds-gui/src/lib.rs
git commit -m "MS10: treemap double-click drill-down with breadcrumb navigation"
```

---

### Task 7: Manual verification

- [ ] **Step 1: Launch the app and scan a directory with mixed file types**

Run: `cargo run -- /usr/share` (or another directory with varied file types and subdirectories)
Expected: scan completes, all three panels display data

- [ ] **Step 2: Verify tree view → treemap synchronization**

Action: Click a file in the directory tree
Expected:
- The file is highlighted in the tree view (selectable_label highlight)
- The corresponding rectangle in the treemap gets a 2px white border
- Clicking a different file moves both highlights

- [ ] **Step 3: Verify treemap → tree view synchronization**

Action: Click a rectangle in the treemap
Expected:
- The treemap rectangle gets a white border
- The tree view auto-expands ancestor directories to make the selected node visible
- The tree view scrolls to the selected node and highlights it

Action: Click a file deep in the treemap that requires expanding multiple tree levels
Expected:
- All ancestor directories expand automatically
- The tree view scrolls to show the selected file

- [ ] **Step 4: Verify extension stats → treemap filtering**

Action: Click an extension name in the ext stats table (e.g., "rs")
Expected:
- The clicked extension row highlights (selectable_label visual feedback)
- All non-matching rectangles in the treemap dim to ~30% brightness
- Matching rectangles remain at full brightness

Action: Click the same extension again
Expected:
- The extension deselects (toggle off)
- All treemap rectangles return to full brightness

Action: Click a segment in the bar chart
Expected:
- Same filtering behavior as clicking the table row
- The corresponding table row also shows as selected

- [ ] **Step 5: Verify simultaneous node + extension selection**

Action: Click an extension in ext stats, then click a specific file in the treemap
Expected:
- Extension dimming remains active
- The clicked file gets a white border
- Both selections coexist visually

- [ ] **Step 6: Verify treemap drill-down via double-click**

Action: Double-click a file that is inside a subdirectory
Expected:
- The treemap zooms to show only the contents of that subdirectory
- A breadcrumb bar appears above the treemap showing the path from root to the subdirectory
- Files in the new view fill the entire treemap area

Action: Double-click again on a file inside a deeper subdirectory
Expected:
- The treemap drills deeper (progressive drill-down)
- The breadcrumb bar extends with the additional path component

- [ ] **Step 7: Verify breadcrumb navigation**

Action: Click a breadcrumb component (not the current directory)
Expected:
- The treemap zooms out to that ancestor directory
- The breadcrumb bar shortens to reflect the new position
- Files from the selected directory level fill the treemap

Action: Click the root breadcrumb
Expected:
- The treemap returns to showing the full scan (top-level view)
- The breadcrumb bar disappears (only visible when drilled in)

- [ ] **Step 8: Verify re-scan resets all state**

Action: After drilling down and selecting nodes/extensions, scan a different directory
Expected:
- Treemap resets to top level (no breadcrumb)
- Extension selection clears
- Node selection clears
- New scan results display correctly in all three panels

- [ ] **Step 9: Commit if any fixes were needed**

```bash
git add -u
git commit -m "MS10: fix issues found during manual verification"
```

---

### Task 8: Update documentation

**Files:**
- Modify: `crates/rds-gui/CLAUDE.md`
- Modify: `plans/CLAUDE.md`
- Modify: `docs/milestones.md`

- [ ] **Step 1: Update rds-gui CLAUDE.md**

Replace the content of `crates/rds-gui/CLAUDE.md` with:

```markdown
# crates/rds-gui/

egui/eframe GUI shell with directory picker, scanner integration, directory tree view, extension statistics, treemap renderer, panel synchronization, and drill-down navigation.

## Files

| File | What | When to read |
| ---- | ---- | ------------ |
| `Cargo.toml` | Crate manifest; depends on `eframe`, `egui`, `streemap`, `crossbeam-channel`, `tracing`, `rds-core`, `rds-scanner`, `rfd` | Modifying GUI dependencies |
| `src/lib.rs` | `RustDirStatApp` with ScanPhase state machine, directory picker (rfd), scanner spawning, event drain loop, 3-panel layout with tree view + treemap + ext stats, breadcrumb navigation, format_bytes utility | Modifying app state, scan lifecycle, layout, adding panel implementations |
| `src/tree_view.rs` | `SubtreeStats` (cached subtree sizes/file counts), `TreeViewState` (expanded nodes, selection sync, scroll-to), `expand_ancestors`, `sorted_children`, tree view rendering (show/render_node) | Modifying tree view display, adding tree interactions, understanding size caching strategy, understanding auto-expand/scroll behavior |
| `src/ext_stats.rs` | `hsl_to_color32` (HSL→Color32 conversion), `show` (extension stats panel with stacked bar chart, scrollable Grid table, click-to-select extension filter) | Modifying extension statistics display, adding bar chart interactions, reusing hsl_to_color32 for treemap coloring |
| `src/treemap.rs` | `TreemapRect`, `TreemapLayout` (cached layout with root tracking), `compute_recursive` (recursive squarify), `find_drill_target` (double-click navigation), `breadcrumb_chain` (ancestor path), `show` (render + tooltip + click selection + extension dimming + double-click drill-down) | Modifying treemap rendering, adding cushion shading (MS9), understanding drill-down navigation |
```

- [ ] **Step 2: Add plan entry to plans/CLAUDE.md**

Add to the file table in `plans/CLAUDE.md`:

```markdown
| `ms10-panel-synchronization.md` | MS10 plan: cross-panel selection sync, extension filter, treemap drill-down with breadcrumb, auto-expand ancestors (completed) | Reviewing panel synchronization design, understanding selection state model, understanding drill-down navigation |
```

- [ ] **Step 3: Update milestone status in docs/milestones.md**

Change `## MS10 — Panel Synchronization` to done:

```markdown
## ~~MS10 — Panel Synchronization~~ DONE
```

- [ ] **Step 4: Commit**

```bash
git add crates/rds-gui/CLAUDE.md plans/CLAUDE.md plans/ms10-panel-synchronization.md docs/milestones.md
git commit -m "MS10: update docs for panel synchronization completion"
```
