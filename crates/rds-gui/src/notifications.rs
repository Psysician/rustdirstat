use std::time::Duration;

use egui_notify::Toasts;

pub(crate) struct Notifications {
    toasts: Toasts,
}

impl Default for Notifications {
    fn default() -> Self {
        Self {
            toasts: Toasts::new().with_anchor(egui_notify::Anchor::TopRight),
        }
    }
}

impl Notifications {
    #[allow(dead_code)]
    pub(crate) fn info(&mut self, msg: impl Into<egui::WidgetText>) {
        self.toasts.info(msg).duration(Some(Duration::from_secs(3)));
    }

    #[allow(dead_code)]
    pub(crate) fn warning(&mut self, msg: impl Into<egui::WidgetText>) {
        self.toasts
            .warning(msg)
            .duration(Some(Duration::from_secs(5)));
    }

    pub(crate) fn error(&mut self, msg: impl Into<egui::WidgetText>) {
        self.toasts
            .error(msg)
            .duration(Some(Duration::from_secs(8)));
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context) {
        self.toasts.show(ctx);
    }
}
