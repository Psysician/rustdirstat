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

/// A single file rectangle in the treemap, ready for painting.
pub(crate) struct TreemapRect {
    /// Index into the `DirTree` arena.
    pub node_index: usize,
    /// Screen-space rectangle (relative to treemap origin).
    pub rect: egui::Rect,
    /// Fill color derived from file extension.
    pub color: egui::Color32,
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
            compute_recursive(tree, stats, tree.root(), bounds, &mut rects);
        }
        TreemapLayout {
            rects,
            last_size: size,
        }
    }
}

fn compute_recursive(
    tree: &DirTree,
    stats: &SubtreeStats,
    dir_index: usize,
    bounds: streemap::Rect<f32>,
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

        if node.is_dir {
            compute_recursive(tree, stats, item.node_index, item.rect, result);
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
}
