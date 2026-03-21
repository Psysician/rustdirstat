//! Duplicates bottom panel rendering.
//!
//! Displays duplicate file groups sorted by wasted space, with collapsible
//! headers and selectable file paths for cross-panel synchronization.

use crate::{DuplicateGroup, format_bytes};
use rds_core::tree::DirTree;

pub(crate) fn show(
    groups: &[DuplicateGroup],
    tree: &DirTree,
    selected_node: &mut Option<usize>,
    ui: &mut egui::Ui,
) {
    let total_wasted: u64 = groups.iter().map(|g| g.wasted_bytes).sum();
    ui.label(format!(
        "{} duplicate groups — {} wasted",
        groups.len(),
        format_bytes(total_wasted),
    ));
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        for (i, group) in groups.iter().enumerate() {
            let file_count = group.node_indices.len();
            let file_size = group
                .node_indices
                .first()
                .and_then(|&idx| tree.get(idx))
                .map(|n| n.size)
                .unwrap_or(0);

            let header_text = format!(
                "{} files — {} each — {} wasted",
                file_count,
                format_bytes(file_size),
                format_bytes(group.wasted_bytes),
            );

            let id = ui.make_persistent_id(("dup_group", i));
            egui::CollapsingHeader::new(header_text)
                .id_salt(id)
                .show(ui, |ui| {
                    for &idx in &group.node_indices {
                        let path = tree.path(idx);
                        let path_str = path.display().to_string();
                        let is_selected = *selected_node == Some(idx);
                        if ui.selectable_label(is_selected, &path_str).clicked() {
                            *selected_node = Some(idx);
                        }
                    }
                });
        }
    });
}
