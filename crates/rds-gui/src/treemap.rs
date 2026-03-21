//! Treemap layout computation using squarified algorithm.
//!
//! Converts a `DirTree` + `SubtreeStats` into a flat list of `TreemapRect`
//! values ready for painting. Directories are recursed into; only leaf files
//! produce output rects. Colors come from `ext_stats::hsl_to_color32` via
//! `rds_core::stats::color_for_extension`.

use crate::ext_stats;
use crate::tree_view::SubtreeStats;
use rds_core::tree::DirTree;

/// Minimum dimension (width or height) for a rect to be worth subdividing.
const MIN_RECT_DIM: f32 = 1.0;

/// Minimum rectangle dimension for cushion shading. Rects smaller
/// than this in either dimension get flat fills. (ref: DL-005)
#[allow(dead_code)] // Used in show() (Task 4)
const MIN_CUSHION_DIM: f32 = 4.0;

/// Initial ridge height at depth 0. Produces visible gradients even
/// on large rectangles. (ref: DL-009)
const INITIAL_HEIGHT: f32 = 40.0;

/// Per-depth height reduction factor. Each nesting level halves the
/// ridge height: 40 → 20 → 10 → 5 → ... (ref: DL-009)
const HEIGHT_FACTOR: f32 = 0.5;

/// Ambient intensity floor. Prevents fully black edges. (ref: DL-007)
#[allow(dead_code)] // Used in intensity() via tests now; wired in Task 3/4
const AMBIENT: f32 = 0.3;

/// Pre-normalized light direction toward upper-left.
/// L = normalize(-0.5, -0.5, 1.0). (ref: DL-003)
#[allow(dead_code)] // Used in intensity() via tests now; wired in Task 3/4
const LIGHT_X: f32 = -0.408_248_3;
#[allow(dead_code)] // Used in intensity() via tests now; wired in Task 3/4
const LIGHT_Y: f32 = -0.408_248_3;
#[allow(dead_code)] // Used in intensity() via tests now; wired in Task 3/4
const LIGHT_Z: f32 = 0.816_496_6;

/// Accumulated parabolic ridge coefficients for cushion shading.
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct CushionCoeffs {
    pub a2x: f32,
    pub a1x: f32,
    pub a2y: f32,
    pub a1y: f32,
}

impl CushionCoeffs {
    fn add_ridge(&mut self, rect: &streemap::Rect<f32>, height: f32) {
        let dx = rect.w;
        let dy = rect.h;
        if dx > 0.0 {
            let x1 = rect.x;
            let x2 = rect.x + rect.w;
            self.a2x += height * (-4.0) / (dx * dx);
            self.a1x += height * 4.0 * (x1 + x2) / (dx * dx);
        }
        if dy > 0.0 {
            let y1 = rect.y;
            let y2 = rect.y + rect.h;
            self.a2y += height * (-4.0) / (dy * dy);
            self.a1y += height * 4.0 * (y1 + y2) / (dy * dy);
        }
    }

    #[allow(dead_code)] // Used in intensity() via tests now; wired in Task 3/4
    fn intensity(&self, x: f32, y: f32) -> f32 {
        let dhdx = 2.0 * self.a2x * x + self.a1x;
        let dhdy = 2.0 * self.a2y * y + self.a1y;

        let n_len = (dhdx * dhdx + dhdy * dhdy + 1.0).sqrt();
        let n_dot_l = (-dhdx * LIGHT_X - dhdy * LIGHT_Y + LIGHT_Z) / n_len;

        AMBIENT + (1.0 - AMBIENT) * n_dot_l.clamp(0.0, 1.0)
    }
}

#[allow(dead_code)] // Used in Task 3
fn shade_color(base: egui::Color32, intensity: f32) -> egui::Color32 {
    let [r, g, b, a] = base.to_array();
    egui::Color32::from_rgba_premultiplied(
        (r as f32 * intensity).round() as u8,
        (g as f32 * intensity).round() as u8,
        (b as f32 * intensity).round() as u8,
        a,
    )
}

#[allow(dead_code)] // Used in Task 3
fn grid_subdivisions(width: f32, height: f32) -> u32 {
    let min_dim = width.min(height);
    if min_dim < 20.0 {
        2
    } else if min_dim < 60.0 {
        4
    } else {
        6
    }
}

/// A single file rectangle in the treemap, ready for painting.
pub(crate) struct TreemapRect {
    /// Index into the `DirTree` arena.
    pub node_index: usize,
    /// Screen-space rectangle (relative to treemap origin).
    pub rect: egui::Rect,
    /// Fill color derived from file extension.
    pub color: egui::Color32,
    /// Nesting depth (0 = direct child of root).
    #[allow(dead_code)] // Read in Task 3/4 for cushion shading
    pub depth: u32,
    /// Accumulated cushion surface coefficients for shading.
    #[allow(dead_code)] // Read in Task 3/4 for cushion shading
    pub cushion: CushionCoeffs,
}

/// Intermediate item used during squarify layout.
struct LayoutItem {
    size: f32,
    node_index: usize,
    rect: streemap::Rect<f32>,
}

/// Computed treemap layout: a flat list of file rectangles.
pub(crate) struct TreemapLayout {
    pub rects: Vec<TreemapRect>,
    pub last_size: egui::Vec2,
}

impl TreemapLayout {
    pub fn compute(tree: &DirTree, stats: &SubtreeStats, size: egui::Vec2) -> Self {
        let mut rects = Vec::new();
        if size.x > 0.0 && size.y > 0.0 {
            let bounds = streemap::Rect {
                x: 0.0,
                y: 0.0,
                w: size.x,
                h: size.y,
            };
            compute_recursive(
                tree, stats, tree.root(), bounds,
                CushionCoeffs::default(), INITIAL_HEIGHT, 0, &mut rects,
            );
        }
        TreemapLayout {
            rects,
            last_size: size,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn compute_recursive(
    tree: &DirTree,
    stats: &SubtreeStats,
    dir_index: usize,
    bounds: streemap::Rect<f32>,
    parent_cushion: CushionCoeffs,
    height: f32,
    depth: u32,
    result: &mut Vec<TreemapRect>,
) {
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

        // Add this child's rectangle as a ridge at current height. (ref: DL-001)
        let mut child_cushion = parent_cushion;
        child_cushion.add_ridge(&item.rect, height);

        if node.is_dir {
            compute_recursive(
                tree, stats, item.node_index, item.rect,
                child_cushion, height * HEIGHT_FACTOR, depth + 1, result,
            );
        } else {
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
                depth,
                cushion: child_cushion,
            });
        }
    }
}

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
    if let Some(sel_idx) = *selected
        && let Some(hit) = layout.rects.iter().find(|r| r.node_index == sel_idx)
    {
        let abs_rect = hit.rect.translate(offset);
        painter.rect_stroke(
            abs_rect,
            0.0,
            egui::Stroke::new(2.0, egui::Color32::WHITE),
            egui::StrokeKind::Outside,
        );
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
    #[allow(deprecated)]
    if let Some(idx) = hovered_index
        && let Some(node) = tree.get(idx)
    {
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

    // Click to select node.
    if response.clicked() {
        *selected = hovered_index;
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
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0));
        assert_eq!(layout.rects.len(), 1);
        let r = &layout.rects[0];
        assert_eq!(r.node_index, 1);
        let area = r.rect.width() * r.rect.height();
        assert!((area - 800.0 * 600.0).abs() < 1.0);
    }

    #[test]
    fn layout_three_files_within_bounds() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file("a.rs", 600, Some("rs")));
        tree.insert(0, make_file("b.py", 300, Some("py")));
        tree.insert(0, make_file("c.js", 100, Some("js")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0));
        assert_eq!(layout.rects.len(), 3);
        for r in &layout.rects {
            assert!(r.rect.min.x >= 0.0);
            assert!(r.rect.min.y >= 0.0);
            assert!(r.rect.max.x <= 800.0 + 0.01);
            assert!(r.rect.max.y <= 600.0 + 0.01);
        }
    }

    #[test]
    fn layout_largest_gets_largest_area() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file("big.rs", 900, Some("rs")));
        tree.insert(0, make_file("small.py", 100, Some("py")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0));
        assert_eq!(layout.rects.len(), 2);
        let area_big = layout.rects.iter()
            .find(|r| r.node_index == 1)
            .map(|r| r.rect.width() * r.rect.height())
            .unwrap();
        let area_small = layout.rects.iter()
            .find(|r| r.node_index == 2)
            .map(|r| r.rect.width() * r.rect.height())
            .unwrap();
        assert!(area_big > area_small);
    }

    #[test]
    fn layout_nested_directory() {
        let mut tree = DirTree::new("/root");
        let sub = tree.insert(0, make_dir("sub"));
        tree.insert(sub, make_file("a.rs", 500, Some("rs")));
        tree.insert(sub, make_file("b.rs", 500, Some("rs")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0));
        assert_eq!(layout.rects.len(), 2);
        // Both files should be present (directory itself is not a rect).
        let indices: Vec<usize> = layout.rects.iter().map(|r| r.node_index).collect();
        assert!(indices.contains(&2));
        assert!(indices.contains(&3));
    }

    #[test]
    fn layout_zero_size_excluded() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file("a.rs", 1000, Some("rs")));
        tree.insert(0, make_file("empty.rs", 0, Some("rs")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0));
        assert_eq!(layout.rects.len(), 1);
        assert_eq!(layout.rects[0].node_index, 1);
    }

    #[test]
    fn layout_empty_directory() {
        let tree = DirTree::new("/root");
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0));
        assert!(layout.rects.is_empty());
    }

    #[test]
    fn layout_color_matches_extension() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file("a.rs", 1000, Some("rs")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0));
        let expected = ext_stats::hsl_to_color32(
            &rds_core::stats::color_for_extension("rs"),
        );
        assert_eq!(layout.rects[0].color, expected);
    }

    #[test]
    fn layout_no_extension_color() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file("Makefile", 1000, None));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0));
        let expected = ext_stats::hsl_to_color32(
            &rds_core::stats::color_for_extension(""),
        );
        assert_eq!(layout.rects[0].color, expected);
    }

    #[test]
    fn layout_no_zero_area_rects() {
        let mut tree = DirTree::new("/root");
        for i in 0..20 {
            tree.insert(
                0,
                make_file(&format!("f{i}.rs"), (i as u64 + 1) * 100, Some("rs")),
            );
        }
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0));
        for r in &layout.rects {
            assert!(r.rect.width() > 0.0, "zero-width rect at {:?}", r.rect);
            assert!(r.rect.height() > 0.0, "zero-height rect at {:?}", r.rect);
        }
    }

    #[test]
    fn layout_zero_size_bounds() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file("a.rs", 1000, Some("rs")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(0.0, 0.0));
        assert!(layout.rects.is_empty());
    }

    #[test]
    fn layout_deeply_nested_files() {
        let mut tree = DirTree::new("/root");
        let d1 = tree.insert(0, make_dir("d1"));
        let d2 = tree.insert(d1, make_dir("d2"));
        let d3 = tree.insert(d2, make_dir("d3"));
        tree.insert(d3, make_file("deep.rs", 1000, Some("rs")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0));
        assert_eq!(layout.rects.len(), 1);
        assert_eq!(layout.rects[0].node_index, 4); // deep.rs is index 4
    }

    #[test]
    fn layout_stores_last_size() {
        let tree = DirTree::new("/root");
        let stats = SubtreeStats::compute(&tree);
        let size = egui::vec2(1024.0, 768.0);
        let layout = TreemapLayout::compute(&tree, &stats, size);
        assert_eq!(layout.last_size, size);
    }

    // --- CushionCoeffs tests ---

    #[test]
    fn cushion_add_ridge_coefficients() {
        let mut c = CushionCoeffs::default();
        c.add_ridge(
            &streemap::Rect { x: 0.0, y: 0.0, w: 100.0, h: 50.0 },
            40.0,
        );
        assert!((c.a2x - (-0.016)).abs() < 1e-6);
        assert!((c.a1x - 1.6).abs() < 1e-6);
        assert!((c.a2y - (-0.064)).abs() < 1e-6);
        assert!((c.a1y - 3.2).abs() < 1e-6);
    }

    #[test]
    fn cushion_ridges_accumulate() {
        let mut c = CushionCoeffs::default();
        c.add_ridge(
            &streemap::Rect { x: 0.0, y: 0.0, w: 200.0, h: 100.0 },
            40.0,
        );
        let a2x_after_first = c.a2x;
        c.add_ridge(
            &streemap::Rect { x: 0.0, y: 0.0, w: 100.0, h: 50.0 },
            20.0,
        );
        assert!(c.a2x < a2x_after_first);
    }

    #[test]
    fn cushion_intensity_center_is_bright() {
        let mut c = CushionCoeffs::default();
        c.add_ridge(
            &streemap::Rect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 },
            40.0,
        );
        let center = c.intensity(50.0, 50.0);
        assert!(center > 0.8, "center intensity {center} should be > 0.8");
    }

    #[test]
    fn cushion_intensity_upper_left_brighter_than_lower_right() {
        let mut c = CushionCoeffs::default();
        c.add_ridge(
            &streemap::Rect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 },
            40.0,
        );
        let upper_left = c.intensity(10.0, 10.0);
        let lower_right = c.intensity(90.0, 90.0);
        assert!(
            upper_left > lower_right,
            "upper-left {upper_left} should be brighter than lower-right {lower_right}",
        );
    }

    #[test]
    fn cushion_intensity_always_in_range() {
        let mut c = CushionCoeffs::default();
        c.add_ridge(
            &streemap::Rect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 },
            40.0,
        );
        c.add_ridge(
            &streemap::Rect { x: 0.0, y: 0.0, w: 50.0, h: 50.0 },
            20.0,
        );
        for row in 0..=10 {
            for col in 0..=10 {
                let x = col as f32 * 5.0;
                let y = row as f32 * 5.0;
                let i = c.intensity(x, y);
                assert!(
                    (AMBIENT..=1.0).contains(&i),
                    "intensity {i} at ({x},{y}) out of [{AMBIENT}, 1.0]",
                );
            }
        }
    }

    // --- shade_color tests ---

    #[test]
    fn shade_color_full_intensity() {
        let base = egui::Color32::from_rgb(200, 100, 50);
        assert_eq!(shade_color(base, 1.0), base);
    }

    #[test]
    fn shade_color_half_intensity() {
        let base = egui::Color32::from_rgb(200, 100, 50);
        let shaded = shade_color(base, 0.5);
        assert_eq!(shaded, egui::Color32::from_rgb(100, 50, 25));
    }

    #[test]
    fn shade_color_zero_intensity() {
        let base = egui::Color32::from_rgb(200, 100, 50);
        assert_eq!(shade_color(base, 0.0), egui::Color32::from_rgb(0, 0, 0));
    }

    // --- grid_subdivisions tests ---

    #[test]
    fn grid_subdivisions_small_rect() {
        assert_eq!(grid_subdivisions(5.0, 5.0), 2);
        assert_eq!(grid_subdivisions(19.0, 19.0), 2);
    }

    #[test]
    fn grid_subdivisions_medium_rect() {
        assert_eq!(grid_subdivisions(20.0, 20.0), 4);
        assert_eq!(grid_subdivisions(59.0, 59.0), 4);
    }

    #[test]
    fn grid_subdivisions_large_rect() {
        assert_eq!(grid_subdivisions(60.0, 60.0), 6);
        assert_eq!(grid_subdivisions(500.0, 500.0), 6);
    }

    #[test]
    fn grid_subdivisions_uses_min_dimension() {
        assert_eq!(grid_subdivisions(100.0, 10.0), 2);
    }

    #[test]
    fn layout_tracks_depth() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file("top.rs", 500, Some("rs")));
        let sub = tree.insert(0, make_dir("sub"));
        tree.insert(sub, make_file("deep.rs", 500, Some("rs")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(200.0, 100.0));

        assert_eq!(layout.rects.len(), 2);
        let top = layout.rects.iter().find(|r| r.node_index == 1).unwrap();
        let deep = layout.rects.iter().find(|r| r.node_index == 3).unwrap();
        assert_eq!(top.depth, 0);
        assert_eq!(deep.depth, 1);
    }

    #[test]
    fn layout_deeply_nested_depth() {
        let mut tree = DirTree::new("/root");
        let d1 = tree.insert(0, make_dir("d1"));
        let d2 = tree.insert(d1, make_dir("d2"));
        let d3 = tree.insert(d2, make_dir("d3"));
        tree.insert(d3, make_file("deep.rs", 500, Some("rs")));
        tree.insert(0, make_file("top.rs", 500, Some("rs")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(200.0, 100.0));

        let deep = layout.rects.iter().find(|r| r.node_index == 4).unwrap();
        let top = layout.rects.iter().find(|r| r.node_index == 5).unwrap();
        assert_eq!(deep.depth, 3);
        assert_eq!(top.depth, 0);
    }

    #[test]
    fn layout_cushion_accumulates_across_levels() {
        let mut tree = DirTree::new("/root");
        let sub = tree.insert(0, make_dir("sub"));
        tree.insert(sub, make_file("deep.rs", 1000, Some("rs")));
        tree.insert(0, make_file("top.rs", 1000, Some("rs")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(200.0, 100.0));

        let top = layout.rects.iter().find(|r| r.node_index == 3).unwrap();
        let deep = layout.rects.iter().find(|r| r.node_index == 2).unwrap();

        let top_mag = top.cushion.a2x.abs() + top.cushion.a2y.abs();
        let deep_mag = deep.cushion.a2x.abs() + deep.cushion.a2y.abs();
        assert!(
            deep_mag > top_mag,
            "nested file ({deep_mag}) should have more accumulated cushion than top-level ({top_mag})",
        );
    }

    #[test]
    fn layout_cushion_coefficients_nonzero() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file("a.rs", 1000, Some("rs")));
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(100.0, 100.0));

        let r = &layout.rects[0];
        assert!(r.cushion.a2x != 0.0, "a2x should be non-zero");
        assert!(r.cushion.a1x != 0.0, "a1x should be non-zero");
        assert!(r.cushion.a2y != 0.0, "a2y should be non-zero");
        assert!(r.cushion.a1y != 0.0, "a1y should be non-zero");
    }
}
