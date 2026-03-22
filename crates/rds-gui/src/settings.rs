//! Settings dialog for configuring exclude patterns, sort order, and color scheme.

/// Transient UI state for the settings dialog window.
pub(crate) struct SettingsDialogState {
    pub show: bool,
    pub exclude_patterns: Vec<String>,
    pub new_pattern: String,
    pub default_sort: String,
    pub color_scheme: String,
}

impl Default for SettingsDialogState {
    fn default() -> Self {
        Self {
            show: false,
            exclude_patterns: Vec::new(),
            new_pattern: String::new(),
            default_sort: "size_desc".to_string(),
            color_scheme: "default".to_string(),
        }
    }
}

const COLOR_SCHEMES: &[(&str, &str)] = &[("default", "Default")];

fn color_label(value: &str) -> &str {
    COLOR_SCHEMES
        .iter()
        .find(|(v, _)| *v == value)
        .map(|(_, label)| *label)
        .unwrap_or("Default")
}

const SORT_OPTIONS: &[(&str, &str)] = &[
    ("size_desc", "Size (largest first)"),
    ("size_asc", "Size (smallest first)"),
    ("name_asc", "Name (A-Z)"),
    ("name_desc", "Name (Z-A)"),
];

fn sort_label(value: &str) -> &str {
    SORT_OPTIONS
        .iter()
        .find(|(v, _)| *v == value)
        .map(|(_, label)| *label)
        .unwrap_or("Size (largest first)")
}

pub(crate) fn show(
    state: &mut SettingsDialogState,
    live_exclude_patterns: &mut Vec<String>,
    live_default_sort: &mut String,
    live_color_scheme: &mut String,
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
                    .selected_text(sort_label(&state.default_sort))
                    .show_ui(ui, |ui| {
                        for &(value, label) in SORT_OPTIONS {
                            ui.selectable_value(&mut state.default_sort, value.to_string(), label);
                        }
                    });
            });

            ui.separator();

            ui.heading("Color Scheme");
            ui.horizontal(|ui| {
                ui.label("Scheme");
                egui::ComboBox::from_id_salt("settings_color_scheme")
                    .selected_text(color_label(&state.color_scheme))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut state.color_scheme,
                            "default".to_string(),
                            "Default",
                        );
                    });
            });

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() {
                    *live_exclude_patterns = state.exclude_patterns.clone();
                    *live_default_sort = state.default_sort.clone();
                    *live_color_scheme = state.color_scheme.clone();
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
