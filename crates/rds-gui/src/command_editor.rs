//! Command editor window for managing user-defined custom commands.
//!
//! Provides an egui Window with inline editing of existing commands,
//! add/remove controls, and a close button. (ref: DL-004, DL-007)

use rds_core::CustomCommand;

pub(crate) fn show(
    commands: &mut Vec<CustomCommand>,
    editor: &mut super::CommandEditorState,
    ctx: &egui::Context,
) {
    if !editor.show {
        return;
    }

    let mut to_remove: Vec<usize> = Vec::new();

    egui::Window::new("Custom Commands")
        .collapsible(false)
        .resizable(true)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            for (i, cmd) in commands.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut cmd.name)
                            .hint_text("Name"),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut cmd.template)
                            .hint_text("Template (use {path})"),
                    );
                    if ui.button("Remove").clicked() {
                        to_remove.push(i);
                    }
                });
            }

            ui.separator();

            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut editor.new_name)
                        .hint_text("Name"),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut editor.new_template)
                        .hint_text("Template (use {path})"),
                );
                let can_add = !editor.new_name.is_empty() && !editor.new_template.is_empty();
                if ui.add_enabled(can_add, egui::Button::new("Add")).clicked() {
                    commands.push(CustomCommand {
                        name: editor.new_name.drain(..).collect(),
                        template: editor.new_template.drain(..).collect(),
                    });
                }
            });

            ui.separator();

            if ui.button("Close").clicked() {
                editor.show = false;
            }
        });

    for i in to_remove.into_iter().rev() {
        commands.remove(i);
    }
}
