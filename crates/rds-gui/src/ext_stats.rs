//! Extension statistics panel with bar chart and sorted table.
//!
//! Renders `ExtensionStats` (computed by `rds_core::stats::compute_extension_stats`)
//! as a horizontal stacked bar chart and a scrollable detail table. Colors are
//! converted from `HslColor` to `egui::Color32` via `hsl_to_color32`. (ref: DL-001, DL-003)

use rds_core::stats::{ExtensionStats, HslColor};

/// Converts an `HslColor` (hue 0–360, saturation 0–1, lightness 0–1) to
/// an `egui::Color32`. Used for rendering swatches and bar chart segments.
/// Will also serve MS8 treemap coloring. (ref: DL-001)
pub(crate) fn hsl_to_color32(hsl: &HslColor) -> egui::Color32 {
    let h = hsl.h;
    let s = hsl.s;
    let l = hsl.l;

    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    let r = ((r1 + m) * 255.0).round() as u8;
    let g = ((g1 + m) * 255.0).round() as u8;
    let b = ((b1 + m) * 255.0).round() as u8;

    egui::Color32::from_rgb(r, g, b)
}

/// Renders the extension statistics panel: a horizontal stacked bar chart
/// at the top for visual overview, and a scrollable detail table below.
///
/// `ext_stats` must be pre-sorted by `total_bytes` descending (as returned
/// by `compute_extension_stats`). (ref: DL-004, DL-006)
pub(crate) fn show(ext_stats: &[ExtensionStats], ui: &mut egui::Ui) {
    // --- Stacked horizontal bar chart (ref: DL-008) ---
    let available_width = ui.available_width();
    let bar_height = 20.0;
    let (bar_rect, _) = ui.allocate_exact_size(
        egui::vec2(available_width, bar_height),
        egui::Sense::hover(),
    );

    let mut x = bar_rect.left();
    for stat in ext_stats {
        let remaining = bar_rect.right() - x;
        if remaining <= 0.0 {
            break;
        }
        let segment_width = (stat.percentage as f32 / 100.0) * bar_rect.width();
        if segment_width < 1.0 {
            continue;
        }
        let w = segment_width.max(2.0).min(remaining);
        let segment = egui::Rect::from_min_size(
            egui::pos2(x, bar_rect.top()),
            egui::vec2(w, bar_height),
        );
        ui.painter()
            .rect_filled(segment, 0.0, hsl_to_color32(&stat.color));
        x += w;
    }

    ui.add_space(8.0);

    // --- Scrollable detail table ---
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("ext_stats_grid")
            .striped(true)
            .num_columns(5)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                // Header row.
                ui.label("");
                ui.label(egui::RichText::new("Ext").strong());
                ui.label(egui::RichText::new("Count").strong());
                ui.label(egui::RichText::new("Size").strong());
                ui.label(egui::RichText::new("%").strong());
                ui.end_row();

                // Data rows.
                for stat in ext_stats {
                    // Color swatch: 12x12 painted rectangle.
                    let (swatch_rect, _) = ui.allocate_exact_size(
                        egui::vec2(12.0, 12.0),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(
                        swatch_rect,
                        2.0,
                        hsl_to_color32(&stat.color),
                    );

                    // Extension name. (ref: DL-007)
                    let display_name = if stat.extension.is_empty() {
                        "(no ext)"
                    } else {
                        &stat.extension
                    };
                    ui.label(display_name);

                    // File count.
                    ui.label(stat.count.to_string());

                    // Total size (human-readable).
                    ui.label(super::format_bytes(stat.total_bytes));

                    // Percentage.
                    ui.label(format!("{:.1}%", stat.percentage));

                    ui.end_row();
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsl_pure_red() {
        let color = hsl_to_color32(&HslColor { h: 0.0, s: 1.0, l: 0.5 });
        assert_eq!(color, egui::Color32::from_rgb(255, 0, 0));
    }

    #[test]
    fn hsl_pure_green() {
        let color = hsl_to_color32(&HslColor { h: 120.0, s: 1.0, l: 0.5 });
        assert_eq!(color, egui::Color32::from_rgb(0, 255, 0));
    }

    #[test]
    fn hsl_pure_blue() {
        let color = hsl_to_color32(&HslColor { h: 240.0, s: 1.0, l: 0.5 });
        assert_eq!(color, egui::Color32::from_rgb(0, 0, 255));
    }

    #[test]
    fn hsl_desaturated_gray() {
        let color = hsl_to_color32(&HslColor { h: 0.0, s: 0.0, l: 0.5 });
        assert_eq!(color, egui::Color32::from_rgb(128, 128, 128));
    }

    #[test]
    fn hsl_black() {
        let color = hsl_to_color32(&HslColor { h: 0.0, s: 0.0, l: 0.0 });
        assert_eq!(color, egui::Color32::from_rgb(0, 0, 0));
    }

    #[test]
    fn hsl_white() {
        let color = hsl_to_color32(&HslColor { h: 0.0, s: 0.0, l: 1.0 });
        assert_eq!(color, egui::Color32::from_rgb(255, 255, 255));
    }

    #[test]
    fn hsl_yellow_sector() {
        let color = hsl_to_color32(&HslColor { h: 60.0, s: 1.0, l: 0.5 });
        assert_eq!(color, egui::Color32::from_rgb(255, 255, 0));
    }

    #[test]
    fn hsl_cyan_sector() {
        let color = hsl_to_color32(&HslColor { h: 180.0, s: 1.0, l: 0.5 });
        assert_eq!(color, egui::Color32::from_rgb(0, 255, 255));
    }

    #[test]
    fn hsl_magenta_sector() {
        let color = hsl_to_color32(&HslColor { h: 300.0, s: 1.0, l: 0.5 });
        assert_eq!(color, egui::Color32::from_rgb(255, 0, 255));
    }

    #[test]
    fn hsl_deterministic_for_same_input() {
        let hsl = rds_core::stats::color_for_extension("rs");
        let c1 = hsl_to_color32(&hsl);
        let c2 = hsl_to_color32(&hsl);
        assert_eq!(c1, c2);
    }

    #[test]
    fn hsl_extension_colors_differ() {
        let c_rs = hsl_to_color32(&rds_core::stats::color_for_extension("rs"));
        let c_py = hsl_to_color32(&rds_core::stats::color_for_extension("py"));
        let c_js = hsl_to_color32(&rds_core::stats::color_for_extension("js"));
        assert_ne!(c_rs, c_py);
        assert_ne!(c_rs, c_js);
        assert_ne!(c_py, c_js);
    }
}
