use std::path::PathBuf;

pub(crate) struct ScanErrorLog {
    entries: Vec<(PathBuf, String)>,
    overflow_count: u64,
}

impl Default for ScanErrorLog {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            overflow_count: 0,
        }
    }
}

impl ScanErrorLog {
    const MAX_ENTRIES: usize = 1000;

    pub(crate) fn push(&mut self, path: PathBuf, error: String) {
        if self.entries.len() < Self::MAX_ENTRIES {
            self.entries.push((path, error));
        } else {
            self.overflow_count += 1;
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.overflow_count = 0;
    }

    pub(crate) fn total_count(&self) -> u64 {
        self.entries.len() as u64 + self.overflow_count
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.overflow_count == 0
    }
}

pub(crate) fn show(log: &ScanErrorLog, ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new(format!("{} scan errors", log.total_count()))
            .strong(),
    );
    ui.separator();

    egui::ScrollArea::vertical()
        .max_height(200.0)
        .show(ui, |ui| {
            for (path, error) in &log.entries {
                ui.label(
                    egui::RichText::new(path.display().to_string())
                        .monospace()
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(error)
                        .weak(),
                );
                ui.add_space(4.0);
            }
            if log.overflow_count > 0 {
                ui.label(
                    egui::RichText::new(format!(
                        "...and {} more errors not shown",
                        log.overflow_count
                    ))
                    .weak()
                    .italics(),
                );
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_cap_behavior() {
        let mut log = ScanErrorLog::default();
        for i in 0..1005 {
            log.push(
                PathBuf::from(format!("/path/{i}")),
                format!("error {i}"),
            );
        }
        assert_eq!(log.total_count(), 1005);
        assert_eq!(log.entries.len(), 1000);
        assert_eq!(log.overflow_count, 5);
    }

    #[test]
    fn clear_resets_state() {
        let mut log = ScanErrorLog::default();
        for i in 0..3 {
            log.push(
                PathBuf::from(format!("/path/{i}")),
                format!("error {i}"),
            );
        }
        log.clear();
        assert_eq!(log.total_count(), 0);
        assert!(log.is_empty());
    }
}
