//! Treemap layout computation using squarified algorithm.
//!
//! Converts a `DirTree` + `SubtreeStats` into a flat list of `TreemapRect`
//! values ready for painting. Directories are recursed into; only leaf files
//! produce output rects. Colors come from `ext_stats::hsl_to_color32` via
//! `rds_core::stats::color_for_extension`.

use crate::PendingDelete;
use crate::ext_stats;
use crate::tree_view::SubtreeStats;
use rds_core::CustomCommand;
use rds_core::tree::DirTree;

/// Minimum dimension (width or height) for a rect to be worth subdividing.
const MIN_RECT_DIM: f32 = 1.0;

/// Maximum number of display rectangles before aggregation kicks in.
/// Scans with more leaf files than this will merge excess items into
/// "other" buckets to keep rendering under budget.
pub const MAX_DISPLAY_RECTS: usize = 50_000;

/// Minimum rectangle dimension for cushion shading. Rects smaller
/// than this in either dimension get flat fills. (ref: DL-005)
const MIN_CUSHION_DIM: f32 = 4.0;

/// Initial ridge height at depth 0. Produces visible gradients even
/// on large rectangles. (ref: DL-009)
const INITIAL_HEIGHT: f32 = 40.0;

/// Per-depth height reduction factor. Each nesting level halves the
/// ridge height: 40 → 20 → 10 → 5 → ... (ref: DL-009)
const HEIGHT_FACTOR: f32 = 0.5;

/// Ambient intensity floor. Prevents fully black edges. (ref: DL-007)
const AMBIENT: f32 = 0.3;

/// Pre-normalized light direction toward upper-left.
/// L = normalize(-0.5, -0.5, 1.0). (ref: DL-003)
const LIGHT_X: f32 = -0.408_248_3;
const LIGHT_Y: f32 = -0.408_248_3;
const LIGHT_Z: f32 = 0.816_496_6;

/// Accumulated parabolic ridge coefficients for cushion shading.
#[derive(Clone, Copy, Default, Debug)]
pub struct CushionCoeffs {
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

    fn intensity(&self, x: f32, y: f32) -> f32 {
        let dhdx = 2.0 * self.a2x * x + self.a1x;
        let dhdy = 2.0 * self.a2y * y + self.a1y;

        let n_len = (dhdx * dhdx + dhdy * dhdy + 1.0).sqrt();
        let n_dot_l = (-dhdx * LIGHT_X - dhdy * LIGHT_Y + LIGHT_Z) / n_len;

        AMBIENT + (1.0 - AMBIENT) * n_dot_l.clamp(0.0, 1.0)
    }
}

fn shade_color(base: egui::Color32, intensity: f32) -> egui::Color32 {
    let [r, g, b, a] = base.to_array();
    egui::Color32::from_rgba_premultiplied(
        (r as f32 * intensity).round() as u8,
        (g as f32 * intensity).round() as u8,
        (b as f32 * intensity).round() as u8,
        a,
    )
}

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

/// Appends a cushion-shaded mesh grid for one rectangle into the shared `mesh`.
///
/// `rel_rect` is in relative layout coordinates (already shrunk by 0.5 for gap).
/// `offset` translates to screen-space for vertex positions.
/// Intensity is computed from `cushion` at relative coordinates. (ref: DL-002, DL-006)
fn build_cushion_mesh(
    mesh: &mut egui::Mesh,
    rel_rect: egui::Rect,
    offset: egui::Vec2,
    cushion: &CushionCoeffs,
    base_color: egui::Color32,
) {
    let w = rel_rect.width();
    let h = rel_rect.height();
    if w <= 0.0 || h <= 0.0 {
        return;
    }

    let n = grid_subdivisions(w, h) as usize;
    let base_idx = mesh.vertices.len() as u32;

    // Generate vertices with per-vertex cushion-modulated colors.
    for row in 0..=n {
        let t = row as f32 / n as f32;
        let y = rel_rect.top() + t * h;

        for col in 0..=n {
            let s = col as f32 / n as f32;
            let x = rel_rect.left() + s * w;

            let intensity = cushion.intensity(x, y);
            let color = shade_color(base_color, intensity);

            mesh.vertices.push(egui::epaint::Vertex {
                pos: egui::pos2(x + offset.x, y + offset.y),
                uv: egui::epaint::WHITE_UV,
                color,
            });
        }
    }

    // Generate triangle indices for the NxN quad grid.
    let cols = (n + 1) as u32;
    for row in 0..n as u32 {
        for col in 0..n as u32 {
            let tl = base_idx + row * cols + col;
            let tr = tl + 1;
            let bl = tl + cols;
            let br = bl + 1;
            mesh.indices.extend_from_slice(&[tl, bl, tr, tr, bl, br]);
        }
    }
}

/// A single file rectangle in the treemap, ready for painting.
pub struct TreemapRect {
    /// Index into the `DirTree` arena. `usize::MAX` is a sentinel for
    /// aggregated "other" buckets that don't correspond to a single node.
    pub node_index: usize,
    /// Screen-space rectangle (relative to treemap origin).
    pub rect: egui::Rect,
    /// Fill color derived from file extension.
    pub color: egui::Color32,
    /// Nesting depth (0 = direct child of root).
    #[allow(dead_code)] // Read in tests; will be used for drill-down (MS10)
    pub depth: u32,
    /// Accumulated cushion surface coefficients for shading.
    pub cushion: CushionCoeffs,
    /// When this rect is an aggregated "other" bucket, contains
    /// `(item_count, total_bytes)` for the merged items. `item_count`
    /// includes both files and directories.
    pub aggregated_count: Option<(u64, u64)>,
}

/// Intermediate item used during squarify layout.
struct LayoutItem {
    size: f32,
    node_index: usize,
    rect: streemap::Rect<f32>,
}

/// Computed treemap layout: a flat list of file rectangles.
pub struct TreemapLayout {
    pub rects: Vec<TreemapRect>,
    pub last_size: egui::Vec2,
    /// Root node used for this layout computation. Used for cache invalidation
    /// when treemap_root changes via drill-down. (ref: DL-009)
    pub last_root: usize,
}

/// Cached GPU mesh for the treemap, built from layout + extension filter.
/// Avoids rebuilding millions of cushion-shaded vertices every frame.
pub struct TreemapMeshCache {
    /// Pre-built cushion mesh wrapped in Arc for cheap per-frame cloning.
    pub mesh: std::sync::Arc<egui::Mesh>,
    /// Flat-fill rects for tiny rectangles.
    pub flat_rects: Vec<(egui::Rect, egui::Color32)>,
    /// The extension filter this mesh was built with.
    pub extension_filter: Option<String>,
    /// The screen offset this mesh was built with.
    pub offset: egui::Vec2,
}

impl TreemapLayout {
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
            let mut rect_count = 0usize;
            compute_recursive(
                tree,
                stats,
                root_index,
                bounds,
                CushionCoeffs::default(),
                INITIAL_HEIGHT,
                0,
                &mut rects,
                &mut rect_count,
            );
        }
        TreemapLayout {
            rects,
            last_size: size,
            last_root: root_index,
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
    rect_count: &mut usize,
) {
    if bounds.w < MIN_RECT_DIM || bounds.h < MIN_RECT_DIM {
        return;
    }

    let mut items: Vec<LayoutItem> = tree
        .children(dir_index)
        .filter_map(|idx| {
            let idx = idx as usize;
            let size = stats.size(idx) as f32;
            if size > 0.0 {
                Some(LayoutItem {
                    size,
                    node_index: idx,
                    rect: streemap::Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 0.0,
                        h: 0.0,
                    },
                })
            } else {
                None
            }
        })
        .collect();

    if items.is_empty() {
        return;
    }

    // Squarify requires size-descending order for optimal aspect ratios.
    // This intentionally ignores the user's default_sort preference,
    // which only applies to the directory tree panel.
    items.sort_by(|a, b| {
        b.size
            .partial_cmp(&a.size)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    streemap::squarify(
        bounds,
        &mut items,
        |item| item.size,
        |item, r| item.rect = r,
    );

    // If we already hit the cap, merge ALL items in this directory into
    // a single "other" bucket covering the full bounds.
    if *rect_count >= MAX_DISPLAY_RECTS {
        let file_count = items.len() as u64;
        let total_bytes: u64 = items.iter().map(|i| i.size as u64).sum();
        result.push(TreemapRect {
            node_index: usize::MAX,
            rect: egui::Rect::from_min_size(
                egui::pos2(bounds.x, bounds.y),
                egui::vec2(bounds.w, bounds.h),
            ),
            color: egui::Color32::from_rgb(80, 80, 80),
            depth,
            cushion: parent_cushion,
            aggregated_count: Some((file_count, total_bytes)),
        });
        *rect_count += 1;
        return;
    }

    for (i, item) in items.iter().enumerate() {
        let node = match tree.get(item.node_index) {
            Some(n) => n,
            None => continue,
        };

        // Add this child's rectangle as a ridge at current height. (ref: DL-001)
        let mut child_cushion = parent_cushion;
        child_cushion.add_ridge(&item.rect, height);

        if node.is_dir() {
            compute_recursive(
                tree,
                stats,
                item.node_index,
                item.rect,
                child_cushion,
                height * HEIGHT_FACTOR,
                depth + 1,
                result,
                rect_count,
            );
        } else {
            let ext = tree.extension_str(node.extension).unwrap_or("");
            let color = ext_stats::hsl_to_color32(&rds_core::stats::color_for_extension(ext));
            result.push(TreemapRect {
                node_index: item.node_index,
                rect: egui::Rect::from_min_size(
                    egui::pos2(item.rect.x, item.rect.y),
                    egui::vec2(item.rect.w, item.rect.h),
                ),
                color,
                depth,
                cushion: child_cushion,
                aggregated_count: None,
            });
            *rect_count += 1;
        }

        // If we hit the cap mid-loop, merge all REMAINING items into
        // a single "other" bucket.
        if *rect_count >= MAX_DISPLAY_RECTS && i + 1 < items.len() {
            let remaining = &items[i + 1..];
            let file_count = remaining.len() as u64;
            let total_bytes: u64 = remaining.iter().map(|r| r.size as u64).sum();

            // Compute bounding box of remaining items' squarified rects.
            let mut min_x = f32::MAX;
            let mut min_y = f32::MAX;
            let mut max_x = f32::MIN;
            let mut max_y = f32::MIN;
            for r in remaining {
                min_x = min_x.min(r.rect.x);
                min_y = min_y.min(r.rect.y);
                max_x = max_x.max(r.rect.x + r.rect.w);
                max_y = max_y.max(r.rect.y + r.rect.h);
            }

            result.push(TreemapRect {
                node_index: usize::MAX,
                rect: egui::Rect::from_min_size(
                    egui::pos2(min_x, min_y),
                    egui::vec2(max_x - min_x, max_y - min_y),
                ),
                color: egui::Color32::from_rgb(80, 80, 80),
                depth,
                cushion: parent_cushion,
                aggregated_count: Some((file_count, total_bytes)),
            });
            *rect_count += 1;
            break;
        }
    }
}

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
        if node.parent == rds_core::tree::NO_PARENT {
            return None;
        }
        let parent = node.parent as usize;
        if parent == current_root {
            if tree.get(current)?.is_dir() {
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
    while let Some(node) = tree.get(current) {
        chain.push((current, tree.name(current).to_string()));
        if node.parent == rds_core::tree::NO_PARENT {
            break;
        }
        current = node.parent as usize;
    }
    chain.reverse();
    chain
}

/// Renders the cached treemap layout with cushion shading, hover tooltips,
/// and click-to-select.
///
/// Large rectangles (>= MIN_CUSHION_DIM) get cushion-shaded mesh rendering.
/// Small rectangles get flat fills. Both use the 0.5px inset for visual
/// Builds the cached treemap mesh from a layout + tree + extension filter.
pub(crate) fn build_mesh_cache(
    layout: &TreemapLayout,
    tree: &DirTree,
    highlighted_extension: &Option<String>,
    offset: egui::Vec2,
) -> TreemapMeshCache {
    let effective_color = |rect_info: &TreemapRect| -> egui::Color32 {
        if rect_info.node_index == usize::MAX {
            return rect_info.color;
        }
        if let Some(ext) = highlighted_extension {
            let matches = tree
                .get(rect_info.node_index)
                .is_some_and(|n| tree.extension_str(n.extension).unwrap_or("") == ext.as_str());
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

    let mut mesh = egui::Mesh::default();
    let mut flat_rects = Vec::new();

    for rect_info in &layout.rects {
        let color = effective_color(rect_info);
        let w = rect_info.rect.width();
        let h = rect_info.rect.height();

        if w >= MIN_CUSHION_DIM && h >= MIN_CUSHION_DIM {
            build_cushion_mesh(
                &mut mesh,
                rect_info.rect.shrink(0.5),
                offset,
                &rect_info.cushion,
                color,
            );
        } else {
            flat_rects.push((rect_info.rect.shrink(0.5).translate(offset), color));
        }
    }

    TreemapMeshCache {
        mesh: std::sync::Arc::new(mesh),
        flat_rects,
        extension_filter: highlighted_extension.clone(),
        offset,
    }
}

/// separation. (ref: DL-002, DL-005, DL-006, DL-007, DL-008, DL-009)
#[allow(clippy::too_many_arguments)]
pub(crate) fn show(
    layout: &TreemapLayout,
    tree: &DirTree,
    selected: &mut Option<usize>,
    highlighted_extension: &Option<String>,
    treemap_root: &mut usize,
    scan_complete: bool,
    pending_delete: &mut Option<PendingDelete>,
    custom_commands: &[CustomCommand],
    notifications: &mut crate::notifications::Notifications,
    mesh_cache: &mut Option<TreemapMeshCache>,
    ui: &mut egui::Ui,
) {
    let (response, painter) = ui.allocate_painter(layout.last_size, egui::Sense::click());
    let offset = response.rect.min.to_vec2();

    // Rebuild mesh cache if stale (extension filter or offset changed).
    let needs_rebuild = mesh_cache.as_ref().is_none_or(|c| {
        c.extension_filter != *highlighted_extension
            || (c.offset.x - offset.x).abs() > 0.5
            || (c.offset.y - offset.y).abs() > 0.5
    });
    if needs_rebuild {
        *mesh_cache = Some(build_mesh_cache(
            layout,
            tree,
            highlighted_extension,
            offset,
        ));
    }
    let cache = mesh_cache.as_ref().unwrap();

    // Paint the cached mesh and flat rects. Arc clone is O(1).
    if !cache.mesh.vertices.is_empty() {
        painter.add(egui::Shape::Mesh(cache.mesh.clone()));
    }
    for &(rect, color) in &cache.flat_rects {
        painter.rect_filled(rect, 0.0, color);
    }

    // Highlight selected node with a white border. (ref: MS8 DL-008)
    // Skip sentinel index (aggregated "other" buckets).
    if let Some(sel_idx) = *selected
        && sel_idx != usize::MAX
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
    let hovered_rect = response.hover_pos().and_then(|pos| {
        let rel = pos - offset;
        layout.rects.iter().find(|r| r.rect.contains(rel))
    });
    let hovered_index = hovered_rect.map(|r| r.node_index);

    // Hover tooltip: full path + human-readable size. (ref: MS8 DL-009)
    // For aggregated rects, show file count and total size instead.
    if let Some(rect_info) = hovered_rect {
        if let Some((count, bytes)) = rect_info.aggregated_count {
            #[allow(deprecated)]
            egui::show_tooltip_at_pointer(ui.ctx(), ui.layer_id(), response.id.with("tip"), |ui| {
                ui.label(format!(
                    "{count} items ({} total)",
                    crate::format_bytes(bytes)
                ));
            });
        } else if let Some(node) = tree.get(rect_info.node_index) {
            let path = tree.path(rect_info.node_index);
            #[allow(deprecated)]
            egui::show_tooltip_at_pointer(ui.ctx(), ui.layer_id(), response.id.with("tip"), |ui| {
                ui.label(path.display().to_string());
                ui.label(crate::format_bytes(node.size));
            });
        }
    }

    // Click to select node. Don't select aggregated "other" buckets.
    if response.clicked() {
        if hovered_index == Some(usize::MAX) {
            // Don't select the sentinel.
        } else {
            *selected = hovered_index;
        }
    }

    // Re-interact to ensure secondary click detection works reliably.
    let response = response.interact(egui::Sense::click());

    // Right-click to select node for context menu. Skip sentinel.
    if response.secondary_clicked() && hovered_index != Some(usize::MAX) {
        *selected = hovered_index;
    }

    // Context menu for selected node (only when scan is complete).
    // Guard attachment so right-clicking empty space doesn't show an empty popup.
    // Skip for sentinel index (aggregated "other" buckets).
    if scan_complete && selected.is_some() && *selected != Some(usize::MAX) {
        response.context_menu(|ui| {
            if let Some(sel_idx) = *selected {
                if ui.button("Open in File Manager").clicked() {
                    if let Err(e) = crate::actions::open_in_file_manager(tree, sel_idx) {
                        notifications.error(format!("Failed to open: {e}"));
                    }
                    ui.close();
                }
                crate::actions::show_custom_commands_menu(
                    ui,
                    tree,
                    sel_idx,
                    custom_commands,
                    notifications,
                );
                if sel_idx != tree.root() {
                    ui.separator();
                    if ui.button("Delete").clicked() {
                        let node = tree.get(sel_idx).unwrap();
                        let path = tree.path(sel_idx);
                        let size = if node.is_dir() {
                            tree.subtree_size(sel_idx)
                        } else {
                            node.size
                        };
                        *pending_delete = Some(PendingDelete {
                            node_index: sel_idx,
                            path_display: path.display().to_string(),
                            size_bytes: size,
                            is_dir: node.is_dir(),
                        });
                        ui.close();
                    }
                }
            }
        });
    }

    // Double-click to drill into subdirectory. (ref: DL-005)
    // Skip aggregated "other" rects — they don't map to a single tree node.
    if response.double_clicked()
        && let Some(idx) = hovered_index
        && idx != usize::MAX
        && let Some(target) = find_drill_target(tree, idx, *treemap_root)
    {
        *treemap_root = target;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rds_core::tree::{FileNode, NO_PARENT};

    fn make_file(size: u64) -> FileNode {
        FileNode {
            name_offset: 0,
            name_len: 0,
            size,
            first_child: u32::MAX,
            next_sibling: u32::MAX,
            modified: 0,
            parent: NO_PARENT,
            extension: 0,
            flags: 0,
        }
    }

    fn make_dir() -> FileNode {
        FileNode {
            name_offset: 0,
            name_len: 0,
            size: 0,
            first_child: u32::MAX,
            next_sibling: u32::MAX,
            modified: 0,
            parent: NO_PARENT,
            extension: 0,
            flags: 1,
        }
    }

    #[test]
    fn layout_single_file() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file(1000), "a.rs");
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0), tree.root());
        assert_eq!(layout.rects.len(), 1);
        let r = &layout.rects[0];
        assert_eq!(r.node_index, 1);
        let area = r.rect.width() * r.rect.height();
        assert!((area - 800.0 * 600.0).abs() < 1.0);
    }

    #[test]
    fn layout_three_files_within_bounds() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file(600), "a.rs");
        tree.insert(0, make_file(300), "b.py");
        tree.insert(0, make_file(100), "c.js");
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0), tree.root());
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
        tree.insert(0, make_file(900), "big.rs");
        tree.insert(0, make_file(100), "small.py");
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0), tree.root());
        assert_eq!(layout.rects.len(), 2);
        let area_big = layout
            .rects
            .iter()
            .find(|r| r.node_index == 1)
            .map(|r| r.rect.width() * r.rect.height())
            .unwrap();
        let area_small = layout
            .rects
            .iter()
            .find(|r| r.node_index == 2)
            .map(|r| r.rect.width() * r.rect.height())
            .unwrap();
        assert!(area_big > area_small);
    }

    #[test]
    fn layout_nested_directory() {
        let mut tree = DirTree::new("/root");
        let sub = tree.insert(0, make_dir(), "sub");
        tree.insert(sub, make_file(500), "a.rs");
        tree.insert(sub, make_file(500), "b.rs");
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0), tree.root());
        assert_eq!(layout.rects.len(), 2);
        // Both files should be present (directory itself is not a rect).
        let indices: Vec<usize> = layout.rects.iter().map(|r| r.node_index).collect();
        assert!(indices.contains(&2));
        assert!(indices.contains(&3));
    }

    #[test]
    fn layout_zero_size_excluded() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file(1000), "a.rs");
        tree.insert(0, make_file(0), "empty.rs");
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0), tree.root());
        assert_eq!(layout.rects.len(), 1);
        assert_eq!(layout.rects[0].node_index, 1);
    }

    #[test]
    fn layout_empty_directory() {
        let tree = DirTree::new("/root");
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0), tree.root());
        assert!(layout.rects.is_empty());
    }

    #[test]
    fn layout_color_matches_extension() {
        let mut tree = DirTree::new("/root");
        let ext_idx = tree.intern_extension(Some("rs"));
        let mut node = make_file(1000);
        node.extension = ext_idx;
        tree.insert(0, node, "a.rs");
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0), tree.root());
        let expected = ext_stats::hsl_to_color32(&rds_core::stats::color_for_extension("rs"));
        assert_eq!(layout.rects[0].color, expected);
    }

    #[test]
    fn layout_no_extension_color() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file(1000), "Makefile");
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0), tree.root());
        let expected = ext_stats::hsl_to_color32(&rds_core::stats::color_for_extension(""));
        assert_eq!(layout.rects[0].color, expected);
    }

    #[test]
    fn layout_no_zero_area_rects() {
        let mut tree = DirTree::new("/root");
        for i in 0..20 {
            let name = format!("f{i}.rs");
            tree.insert(0, make_file((i as u64 + 1) * 100), &name);
        }
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0), tree.root());
        for r in &layout.rects {
            assert!(r.rect.width() > 0.0, "zero-width rect at {:?}", r.rect);
            assert!(r.rect.height() > 0.0, "zero-height rect at {:?}", r.rect);
        }
    }

    #[test]
    fn layout_zero_size_bounds() {
        let mut tree = DirTree::new("/root");
        tree.insert(0, make_file(1000), "a.rs");
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(0.0, 0.0), tree.root());
        assert!(layout.rects.is_empty());
    }

    #[test]
    fn layout_deeply_nested_files() {
        let mut tree = DirTree::new("/root");
        let d1 = tree.insert(0, make_dir(), "d1");
        let d2 = tree.insert(d1, make_dir(), "d2");
        let d3 = tree.insert(d2, make_dir(), "d3");
        tree.insert(d3, make_file(1000), "deep.rs");
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0), tree.root());
        assert_eq!(layout.rects.len(), 1);
        assert_eq!(layout.rects[0].node_index, 4); // deep.rs is index 4
    }

    #[test]
    fn layout_stores_last_size() {
        let tree = DirTree::new("/root");
        let stats = SubtreeStats::compute(&tree);
        let size = egui::vec2(1024.0, 768.0);
        let layout = TreemapLayout::compute(&tree, &stats, size, tree.root());
        assert_eq!(layout.last_size, size);
    }

    // --- CushionCoeffs tests ---

    #[test]
    fn cushion_add_ridge_coefficients() {
        let mut c = CushionCoeffs::default();
        c.add_ridge(
            &streemap::Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 50.0,
            },
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
            &streemap::Rect {
                x: 0.0,
                y: 0.0,
                w: 200.0,
                h: 100.0,
            },
            40.0,
        );
        let a2x_after_first = c.a2x;
        c.add_ridge(
            &streemap::Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 50.0,
            },
            20.0,
        );
        assert!(c.a2x < a2x_after_first);
    }

    #[test]
    fn cushion_intensity_center_is_bright() {
        let mut c = CushionCoeffs::default();
        c.add_ridge(
            &streemap::Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
            40.0,
        );
        let center = c.intensity(50.0, 50.0);
        assert!(center > 0.8, "center intensity {center} should be > 0.8");
    }

    #[test]
    fn cushion_intensity_upper_left_brighter_than_lower_right() {
        let mut c = CushionCoeffs::default();
        c.add_ridge(
            &streemap::Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
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
            &streemap::Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
            40.0,
        );
        c.add_ridge(
            &streemap::Rect {
                x: 0.0,
                y: 0.0,
                w: 50.0,
                h: 50.0,
            },
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
        tree.insert(0, make_file(500), "top.rs");
        let sub = tree.insert(0, make_dir(), "sub");
        tree.insert(sub, make_file(500), "deep.rs");
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(200.0, 100.0), tree.root());

        assert_eq!(layout.rects.len(), 2);
        let top = layout.rects.iter().find(|r| r.node_index == 1).unwrap();
        let deep = layout.rects.iter().find(|r| r.node_index == 3).unwrap();
        assert_eq!(top.depth, 0);
        assert_eq!(deep.depth, 1);
    }

    #[test]
    fn layout_deeply_nested_depth() {
        let mut tree = DirTree::new("/root");
        let d1 = tree.insert(0, make_dir(), "d1");
        let d2 = tree.insert(d1, make_dir(), "d2");
        let d3 = tree.insert(d2, make_dir(), "d3");
        tree.insert(d3, make_file(500), "deep.rs");
        tree.insert(0, make_file(500), "top.rs");
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(200.0, 100.0), tree.root());

        let deep = layout.rects.iter().find(|r| r.node_index == 4).unwrap();
        let top = layout.rects.iter().find(|r| r.node_index == 5).unwrap();
        assert_eq!(deep.depth, 3);
        assert_eq!(top.depth, 0);
    }

    #[test]
    fn layout_cushion_accumulates_across_levels() {
        let mut tree = DirTree::new("/root");
        let sub = tree.insert(0, make_dir(), "sub");
        tree.insert(sub, make_file(1000), "deep.rs");
        tree.insert(0, make_file(1000), "top.rs");
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(200.0, 100.0), tree.root());

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
        tree.insert(0, make_file(1000), "a.rs");
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(100.0, 100.0), tree.root());

        let r = &layout.rects[0];
        assert!(r.cushion.a2x != 0.0, "a2x should be non-zero");
        assert!(r.cushion.a1x != 0.0, "a1x should be non-zero");
        assert!(r.cushion.a2y != 0.0, "a2y should be non-zero");
        assert!(r.cushion.a1y != 0.0, "a1y should be non-zero");
    }

    // --- build_cushion_mesh tests ---

    #[test]
    fn build_mesh_vertex_and_index_counts() {
        let mut c = CushionCoeffs::default();
        c.add_ridge(
            &streemap::Rect {
                x: 10.0,
                y: 10.0,
                w: 100.0,
                h: 100.0,
            },
            40.0,
        );
        let rel_rect = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(100.0, 100.0));
        let offset = egui::vec2(50.0, 50.0);
        let base = egui::Color32::from_rgb(200, 100, 50);

        let mut mesh = egui::Mesh::default();
        build_cushion_mesh(&mut mesh, rel_rect.shrink(0.5), offset, &c, base);

        // 99px after shrink → grid_subdivisions = 6
        let n = 6_usize;
        let expected_verts = (n + 1) * (n + 1);
        let expected_indices = n * n * 6;
        assert_eq!(mesh.vertices.len(), expected_verts, "vertex count");
        assert_eq!(mesh.indices.len(), expected_indices, "index count");
    }

    #[test]
    fn build_mesh_vertices_within_bounds() {
        let mut c = CushionCoeffs::default();
        c.add_ridge(
            &streemap::Rect {
                x: 20.0,
                y: 30.0,
                w: 80.0,
                h: 60.0,
            },
            40.0,
        );
        let rel_rect = egui::Rect::from_min_size(egui::pos2(20.0, 30.0), egui::vec2(80.0, 60.0));
        let offset = egui::vec2(100.0, 200.0);
        let base = egui::Color32::from_rgb(200, 100, 50);

        let mut mesh = egui::Mesh::default();
        build_cushion_mesh(&mut mesh, rel_rect.shrink(0.5), offset, &c, base);

        let abs_rect = rel_rect.shrink(0.5).translate(offset);
        for v in &mesh.vertices {
            assert!(
                v.pos.x >= abs_rect.left() - 0.01
                    && v.pos.x <= abs_rect.right() + 0.01
                    && v.pos.y >= abs_rect.top() - 0.01
                    && v.pos.y <= abs_rect.bottom() + 0.01,
                "vertex {:?} outside bounds {:?}",
                v.pos,
                abs_rect,
            );
        }
    }

    #[test]
    fn build_mesh_colors_vary() {
        let mut c = CushionCoeffs::default();
        c.add_ridge(
            &streemap::Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
            40.0,
        );
        let rel_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));
        let base = egui::Color32::from_rgb(200, 100, 50);

        let mut mesh = egui::Mesh::default();
        build_cushion_mesh(&mut mesh, rel_rect, egui::Vec2::ZERO, &c, base);

        let first_color = mesh.vertices[0].color;
        let has_different = mesh.vertices.iter().any(|v| v.color != first_color);
        assert!(
            has_different,
            "all vertices have same color — no cushion effect"
        );
    }

    #[test]
    fn build_mesh_accumulates_into_existing() {
        let mut c = CushionCoeffs::default();
        c.add_ridge(
            &streemap::Rect {
                x: 0.0,
                y: 0.0,
                w: 50.0,
                h: 50.0,
            },
            40.0,
        );
        let rel = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(50.0, 50.0));
        let base = egui::Color32::from_rgb(100, 100, 100);

        let mut mesh = egui::Mesh::default();
        build_cushion_mesh(&mut mesh, rel, egui::Vec2::ZERO, &c, base);
        let first_count = mesh.vertices.len();

        build_cushion_mesh(
            &mut mesh,
            rel.translate(egui::vec2(60.0, 0.0)),
            egui::Vec2::ZERO,
            &c,
            base,
        );
        assert!(
            mesh.vertices.len() > first_count,
            "second call should add more vertices"
        );
    }

    #[test]
    fn performance_50k_layout_and_mesh() {
        // Build a tree with 100 dirs x 500 files = 50,000 leaf files.
        let mut tree = DirTree::new("/root");
        for d in 0..100 {
            let dir_name = format!("dir_{d}");
            let dir = tree.insert(0, make_dir(), &dir_name);
            for f in 0..500 {
                let name = format!("file_{f}.rs");
                tree.insert(dir, make_file((f as u64 + 1) * 100), &name);
            }
        }
        let stats = SubtreeStats::compute(&tree);

        // Time the layout computation (includes cushion coefficient accumulation).
        let start = std::time::Instant::now();
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(1920.0, 1080.0), tree.root());
        let layout_elapsed = start.elapsed();

        assert_eq!(layout.rects.len(), 50_000);
        assert!(
            layout_elapsed.as_secs() < 2,
            "layout took {layout_elapsed:?}, expected < 2s",
        );

        // Time mesh construction for all cushion-eligible rects.
        let start = std::time::Instant::now();
        let mut mesh = egui::Mesh::default();
        let offset = egui::Vec2::ZERO;
        for rect_info in &layout.rects {
            if rect_info.rect.width() >= MIN_CUSHION_DIM
                && rect_info.rect.height() >= MIN_CUSHION_DIM
            {
                build_cushion_mesh(
                    &mut mesh,
                    rect_info.rect.shrink(0.5),
                    offset,
                    &rect_info.cushion,
                    rect_info.color,
                );
            }
        }
        let mesh_elapsed = start.elapsed();

        assert!(
            mesh_elapsed.as_secs() < 2,
            "mesh build took {mesh_elapsed:?}, expected < 2s (vertices: {}, indices: {})",
            mesh.vertices.len(),
            mesh.indices.len(),
        );
    }

    #[test]
    fn layout_with_custom_root_shows_subtree_only() {
        let mut tree = DirTree::new("/root");
        let sub = tree.insert(0, make_dir(), "sub");
        tree.insert(sub, make_file(500), "a.rs");
        tree.insert(sub, make_file(300), "b.rs");
        tree.insert(0, make_file(200), "top.txt");

        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0), sub);

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
        let sub = tree.insert(0, make_dir(), "sub");
        tree.insert(sub, make_file(100), "a.rs");
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0), sub);
        assert_eq!(layout.last_root, sub);
    }

    #[test]
    fn drill_target_file_inside_subdir() {
        let mut tree = DirTree::new("/root");
        let sub = tree.insert(0, make_dir(), "sub");
        let file = tree.insert(sub, make_file(100), "a.rs");
        assert_eq!(find_drill_target(&tree, file, 0), Some(sub));
    }

    #[test]
    fn drill_target_file_at_top_level_returns_none() {
        let mut tree = DirTree::new("/root");
        let file = tree.insert(0, make_file(100), "a.rs");
        assert_eq!(find_drill_target(&tree, file, 0), None);
    }

    #[test]
    fn drill_target_deeply_nested_file() {
        let mut tree = DirTree::new("/root");
        let d1 = tree.insert(0, make_dir(), "d1");
        let d2 = tree.insert(d1, make_dir(), "d2");
        let file = tree.insert(d2, make_file(100), "deep.rs");
        assert_eq!(find_drill_target(&tree, file, 0), Some(d1));
        assert_eq!(find_drill_target(&tree, file, d1), Some(d2));
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
        let sub = tree.insert(0, make_dir(), "sub");
        let chain = breadcrumb_chain(&tree, sub);
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0], (0, "/root".to_string()));
        assert_eq!(chain[1], (sub, "sub".to_string()));
    }

    #[test]
    fn breadcrumb_chain_three_levels() {
        let mut tree = DirTree::new("/root");
        let d1 = tree.insert(0, make_dir(), "d1");
        let d2 = tree.insert(d1, make_dir(), "d2");
        let d3 = tree.insert(d2, make_dir(), "d3");
        let chain = breadcrumb_chain(&tree, d3);
        assert_eq!(chain.len(), 4);
        assert_eq!(chain[0], (0, "/root".to_string()));
        assert_eq!(chain[1], (d1, "d1".to_string()));
        assert_eq!(chain[2], (d2, "d2".to_string()));
        assert_eq!(chain[3], (d3, "d3".to_string()));
    }

    // --- Aggregation tests ---

    #[test]
    fn aggregation_caps_rect_count() {
        // 200 directories x 500 files = 100,000 leaf files.
        let mut tree = DirTree::new("/root");
        for d in 0..200 {
            let dir_name = format!("dir_{d}");
            let dir = tree.insert(0, make_dir(), &dir_name);
            for f in 0..500 {
                let name = format!("file_{f}.rs");
                tree.insert(dir, make_file((f as u64 + 1) * 10), &name);
            }
        }
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0), tree.root());

        // Rect count should be capped near MAX_DISPLAY_RECTS, with some
        // slack for directories that contribute rects before the cap.
        assert!(
            layout.rects.len() <= MAX_DISPLAY_RECTS + 200,
            "expected <= {} rects, got {}",
            MAX_DISPLAY_RECTS + 200,
            layout.rects.len(),
        );

        // At least one aggregated rect should exist.
        assert!(
            layout.rects.iter().any(|r| r.aggregated_count.is_some()),
            "expected at least one aggregated rect",
        );
    }

    #[test]
    fn aggregation_not_triggered_below_threshold() {
        // 100 files — well below the 50k cap.
        let mut tree = DirTree::new("/root");
        for i in 0..100 {
            let name = format!("file_{i}.rs");
            tree.insert(0, make_file((i as u64 + 1) * 100), &name);
        }
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0), tree.root());

        assert_eq!(layout.rects.len(), 100);
        assert!(
            layout.rects.iter().all(|r| r.aggregated_count.is_none()),
            "no rects should be aggregated below the threshold",
        );
    }

    #[test]
    fn aggregated_rect_has_sentinel_index() {
        // 200 dirs x 500 files = 100,000 — triggers aggregation.
        let mut tree = DirTree::new("/root");
        for d in 0..200 {
            let dir_name = format!("dir_{d}");
            let dir = tree.insert(0, make_dir(), &dir_name);
            for f in 0..500 {
                let name = format!("file_{f}.rs");
                tree.insert(dir, make_file((f as u64 + 1) * 10), &name);
            }
        }
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0), tree.root());

        let aggregated = layout
            .rects
            .iter()
            .find(|r| r.aggregated_count.is_some())
            .expect("should have at least one aggregated rect");
        assert_eq!(
            aggregated.node_index,
            usize::MAX,
            "aggregated rect should use sentinel index",
        );
    }

    #[test]
    fn aggregated_rect_size_sum_matches() {
        // 1 directory with 60,000 files of size 1 each.
        let mut tree = DirTree::new("/root");
        let dir = tree.insert(0, make_dir(), "big_dir");
        for f in 0..60_000 {
            let name = format!("f{f}.txt");
            tree.insert(dir, make_file(1), &name);
        }
        let stats = SubtreeStats::compute(&tree);
        let layout = TreemapLayout::compute(&tree, &stats, egui::vec2(800.0, 600.0), tree.root());

        // Sum the area of all rects (both regular and aggregated).
        let total_area: f32 = layout
            .rects
            .iter()
            .map(|r| r.rect.width() * r.rect.height())
            .sum();
        let bounds_area = 800.0 * 600.0;

        // Total area should approximately equal the treemap bounds area.
        let ratio = total_area / bounds_area;
        assert!(
            (0.95..=1.05).contains(&ratio),
            "total rect area ({total_area}) should approximately equal bounds area ({bounds_area}), ratio = {ratio}",
        );
    }
}
