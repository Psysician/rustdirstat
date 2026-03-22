//! Settings dialog for configuring exclude patterns, sort order, color scheme,
//! follow-symlinks, and max recent paths.

use rds_core::{ColorScheme, SortOrder};

/// Transient UI state for the settings dialog window.
pub(crate) struct SettingsDialogState {
    pub show: bool,
    pub exclude_patterns: Vec<String>,
    pub new_pattern: String,
    pub default_sort: SortOrder,
    pub color_scheme: ColorScheme,
    pub follow_symlinks: bool,
    pub max_recent_paths: usize,
}

impl Default for SettingsDialogState {
    fn default() -> Self {
        let defaults = rds_core::AppConfig::default();
        Self {
            show: false,
            exclude_patterns: Vec::new(),
            new_pattern: String::new(),
            default_sort: defaults.default_sort,
            color_scheme: defaults.color_scheme,
            follow_symlinks: defaults.follow_symlinks,
            max_recent_paths: defaults.max_recent_paths,
        }
    }
}

pub(crate) fn show(
    state: &mut SettingsDialogState,
    live_exclude_patterns: &mut Vec<String>,
    live_default_sort: &mut SortOrder,
    live_color_scheme: &mut ColorScheme,
    live_follow_symlinks: &mut bool,
    live_max_recent_paths: &mut usize,
    ctx: &egui::Context,
) -> bool {
    if !state.show {
        return false;
    }

    let mut applied = false;
    let mut to_remove: Vec<usize> = Vec::new();

    egui::Window::new("Settings")
        .collapsible(false)
        .resizable(true)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.heading("Exclude Patterns");
            for (i, pattern) in state.exclude_patterns.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(pattern.as_str());
                    if ui.button("Remove").clicked() {
                        to_remove.push(i);
                    }
                });
            }

            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut state.new_pattern).hint_text("e.g. *.tmp"));
                let can_add = !state.new_pattern.is_empty();
                if ui.add_enabled(can_add, egui::Button::new("Add")).clicked() {
                    state
                        .exclude_patterns
                        .push(std::mem::take(&mut state.new_pattern));
                }
            });

            ui.separator();

            ui.heading("Sort Order");
            ui.horizontal(|ui| {
                ui.label("Default");
                egui::ComboBox::from_id_salt("settings_sort_order")
                    .selected_text(state.default_sort.label())
                    .show_ui(ui, |ui| {
                        for &variant in SortOrder::ALL {
                            ui.selectable_value(&mut state.default_sort, variant, variant.label());
                        }
                    });
            });

            ui.separator();

            ui.heading("Color Scheme");
            ui.horizontal(|ui| {
                ui.label("Scheme");
                egui::ComboBox::from_id_salt("settings_color_scheme")
                    .selected_text(state.color_scheme.label())
                    .show_ui(ui, |ui| {
                        for &variant in ColorScheme::ALL {
                            ui.selectable_value(&mut state.color_scheme, variant, variant.label());
                        }
                    });
            });

            ui.separator();

            ui.heading("Scanner");
            ui.checkbox(&mut state.follow_symlinks, "Follow symbolic links");

            ui.separator();

            ui.heading("Recent Paths");
            ui.horizontal(|ui| {
                ui.label("Max entries");
                ui.add(egui::DragValue::new(&mut state.max_recent_paths).range(1..=100));
            });

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() {
                    *live_exclude_patterns = state.exclude_patterns.clone();
                    *live_default_sort = state.default_sort;
                    *live_color_scheme = state.color_scheme;
                    *live_follow_symlinks = state.follow_symlinks;
                    *live_max_recent_paths = state.max_recent_paths;
                    state.show = false;
                    applied = true;
                }
                if ui.button("Cancel").clicked() {
                    state.show = false;
                }
            });
        });

    for i in to_remove.into_iter().rev() {
        state.exclude_patterns.remove(i);
    }

    applied
}
