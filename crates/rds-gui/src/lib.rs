//! egui application shell. Implements `eframe::App` for the main window.
//!
//! `RustDirStatApp` satisfies the eframe contract with an empty central panel.
//! Scan state, treemap rendering, and panel layout are separate concerns
//! handled by other modules.

#[derive(Default)]
pub struct RustDirStatApp;

impl eframe::App for RustDirStatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |_ui| {});
    }
}
