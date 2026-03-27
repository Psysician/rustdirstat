//! CSV and JSON export of scan results.
//!
//! Defines format/scope enums, flat record structs for tree and duplicate
//! exports, dialog state, and the export dialog UI window.

#[derive(Clone, Copy, PartialEq, Default)]
pub(crate) enum ExportFormat {
    #[default]
    Csv,
    Json,
}

impl ExportFormat {
    pub fn label(&self) -> &str {
        match self {
            Self::Csv => "CSV",
            Self::Json => "JSON",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Default)]
pub(crate) enum ExportScope {
    #[default]
    FullTree,
    CurrentView,
    DuplicatesOnly,
}

impl ExportScope {
    pub fn label(&self) -> &str {
        match self {
            Self::FullTree => "Full Tree",
            Self::CurrentView => "Current View",
            Self::DuplicatesOnly => "Duplicates Only",
        }
    }
}

#[derive(serde::Serialize)]
pub(crate) struct ExportRecord {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub size_human: String,
    pub is_dir: bool,
    pub extension: String,
    pub modified_timestamp: Option<u64>,
}

#[derive(serde::Serialize)]
pub(crate) struct DuplicateExportRecord {
    pub group_number: usize,
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub size_human: String,
    pub extension: String,
    pub wasted_bytes_in_group: u64,
}

pub(crate) enum ExportResult {
    Success { path: String, record_count: usize },
    Error(String),
}

pub(crate) struct ExportDialogState {
    pub show: bool,
    pub format: ExportFormat,
    pub scope: ExportScope,
    pub last_result: Option<ExportResult>,
}

impl Default for ExportDialogState {
    fn default() -> Self {
        Self {
            show: false,
            format: ExportFormat::Csv,
            scope: ExportScope::FullTree,
            last_result: None,
        }
    }
}

pub(crate) fn export_tree(
    tree: &rds_core::tree::DirTree,
    root_index: usize,
    format: ExportFormat,
    output_path: &std::path::Path,
) -> ExportResult {
    let mut records = Vec::new();
    let mut stack = vec![root_index];

    while let Some(index) = stack.pop() {
        let node = match tree.get(index) {
            Some(n) => n,
            None => continue,
        };
        if node.deleted() {
            continue;
        }
        let path = tree.path(index).to_string_lossy().to_string();
        records.push(ExportRecord {
            path,
            name: node.name.to_string(),
            size_bytes: node.size,
            size_human: crate::format_bytes(node.size),
            is_dir: node.is_dir(),
            extension: tree
                .extension_str(node.extension)
                .unwrap_or_default()
                .to_string(),
            modified_timestamp: if node.modified != 0 {
                Some(node.modified)
            } else {
                None
            },
        });
        // Collect and reverse to process children in insertion order.
        let child_indices: Vec<u32> = tree.children(index).collect();
        for &child_idx in child_indices.iter().rev() {
            stack.push(child_idx as usize);
        }
    }

    write_records(
        &records,
        format,
        output_path,
        &[
            "path",
            "name",
            "size_bytes",
            "size_human",
            "is_dir",
            "extension",
            "modified_timestamp",
        ],
    )
}

pub(crate) fn export_duplicates(
    tree: &rds_core::tree::DirTree,
    groups: &[crate::DuplicateGroup],
    format: ExportFormat,
    output_path: &std::path::Path,
) -> ExportResult {
    let mut records = Vec::new();

    for (group_idx, group) in groups.iter().enumerate() {
        for &idx in &group.node_indices {
            let node = match tree.get(idx) {
                Some(n) => n,
                None => continue,
            };
            if node.deleted() {
                continue;
            }
            let path = tree.path(idx).to_string_lossy().to_string();
            records.push(DuplicateExportRecord {
                group_number: group_idx + 1,
                path,
                name: node.name.to_string(),
                size_bytes: node.size,
                size_human: crate::format_bytes(node.size),
                extension: tree
                    .extension_str(node.extension)
                    .unwrap_or_default()
                    .to_string(),
                wasted_bytes_in_group: group.wasted_bytes,
            });
        }
    }

    write_records(
        &records,
        format,
        output_path,
        &[
            "group_number",
            "path",
            "name",
            "size_bytes",
            "size_human",
            "extension",
            "wasted_bytes_in_group",
        ],
    )
}

fn write_records<T: serde::Serialize>(
    records: &[T],
    format: ExportFormat,
    output_path: &std::path::Path,
    empty_csv_headers: &[&str],
) -> ExportResult {
    let file = match std::fs::File::create(output_path) {
        Ok(f) => f,
        Err(e) => return ExportResult::Error(e.to_string()),
    };

    let record_count = records.len();
    let path_string = output_path.to_string_lossy().to_string();

    match format {
        ExportFormat::Csv => {
            let mut writer = csv::Writer::from_writer(file);
            if records.is_empty()
                && let Err(e) = writer.write_record(empty_csv_headers)
            {
                return ExportResult::Error(e.to_string());
            }
            for record in records {
                if let Err(e) = writer.serialize(record) {
                    return ExportResult::Error(e.to_string());
                }
            }
            if let Err(e) = writer.flush() {
                return ExportResult::Error(e.to_string());
            }
        }
        ExportFormat::Json => {
            let buf = std::io::BufWriter::new(file);
            if let Err(e) = serde_json::to_writer_pretty(buf, &records) {
                return ExportResult::Error(e.to_string());
            }
        }
    }

    ExportResult::Success {
        path: path_string,
        record_count,
    }
}

pub(crate) fn show_dialog(
    state: &mut ExportDialogState,
    tree: Option<&rds_core::tree::DirTree>,
    treemap_root: usize,
    duplicate_groups: &[crate::DuplicateGroup],
    notifications: &mut crate::notifications::Notifications,
    ctx: &egui::Context,
) {
    if !state.show {
        return;
    }

    if duplicate_groups.is_empty() && state.scope == ExportScope::DuplicatesOnly {
        state.scope = ExportScope::FullTree;
    }

    egui::Window::new("Export Scan Results")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Format");
                egui::ComboBox::from_id_salt("export_format")
                    .selected_text(state.format.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut state.format,
                            ExportFormat::Csv,
                            ExportFormat::Csv.label(),
                        );
                        ui.selectable_value(
                            &mut state.format,
                            ExportFormat::Json,
                            ExportFormat::Json.label(),
                        );
                    });
            });

            ui.horizontal(|ui| {
                ui.label("Scope");
                egui::ComboBox::from_id_salt("export_scope")
                    .selected_text(state.scope.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut state.scope,
                            ExportScope::FullTree,
                            ExportScope::FullTree.label(),
                        );
                        ui.selectable_value(
                            &mut state.scope,
                            ExportScope::CurrentView,
                            ExportScope::CurrentView.label(),
                        );
                        if duplicate_groups.is_empty() {
                            ui.add_enabled(
                                false,
                                egui::Button::new(format!(
                                    "{} (no duplicates detected)",
                                    ExportScope::DuplicatesOnly.label()
                                )),
                            );
                        } else {
                            ui.selectable_value(
                                &mut state.scope,
                                ExportScope::DuplicatesOnly,
                                ExportScope::DuplicatesOnly.label(),
                            );
                        }
                    });
            });

            ui.separator();

            if ui.button("Export...").clicked() {
                let (filter_name, filter_ext) = match state.format {
                    ExportFormat::Csv => ("CSV files", "csv"),
                    ExportFormat::Json => ("JSON files", "json"),
                };
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name(default_filename(state.format))
                    .add_filter(filter_name, &[filter_ext])
                    .save_file()
                {
                    let result = match tree {
                        Some(t) => match state.scope {
                            ExportScope::DuplicatesOnly => {
                                export_duplicates(t, duplicate_groups, state.format, &path)
                            }
                            ExportScope::CurrentView => {
                                export_tree(t, treemap_root, state.format, &path)
                            }
                            ExportScope::FullTree => export_tree(t, t.root(), state.format, &path),
                        },
                        None => ExportResult::Error("No scan data available.".to_string()),
                    };
                    if let ExportResult::Error(ref msg) = result {
                        notifications.error(msg.clone());
                    }
                    state.last_result = Some(result);
                }
            }

            if let Some(ref result) = state.last_result {
                match result {
                    ExportResult::Success { record_count, path } => {
                        ui.colored_label(
                            egui::Color32::from_rgb(80, 200, 80),
                            format!("Exported {record_count} records to {path}"),
                        );
                    }
                    ExportResult::Error(msg) => {
                        ui.colored_label(egui::Color32::from_rgb(255, 80, 80), msg);
                    }
                }
            }

            if ui.button("Close").clicked() {
                state.show = false;
            }
        });
}

pub(crate) fn default_filename(format: ExportFormat) -> String {
    match format {
        ExportFormat::Csv => "rustdirstat-export.csv".to_string(),
        ExportFormat::Json => "rustdirstat-export.json".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rds_core::tree::{DirTree, FileNode, NO_PARENT};
    use std::io::Read;

    fn make_file_node_in_tree(
        tree: &mut DirTree,
        name: &str,
        size: u64,
        ext: Option<&str>,
        modified: u64,
    ) -> FileNode {
        let ext_idx = tree.intern_extension(ext);
        FileNode {
            name: name.into(),
            size,
            first_child: u32::MAX,
            next_sibling: u32::MAX,
            modified,
            parent: NO_PARENT,
            extension: ext_idx,
            flags: 0,
        }
    }

    fn make_dir_node(_name: &str) -> FileNode {
        FileNode {
            name: _name.into(),
            size: 0,
            first_child: u32::MAX,
            next_sibling: u32::MAX,
            modified: 0,
            parent: NO_PARENT,
            extension: 0,
            flags: 1,
        }
    }

    fn build_test_tree() -> DirTree {
        let mut tree = DirTree::new("/root");
        let subdir = make_dir_node("subdir");
        let idx_sub = tree.insert(0, subdir);

        let file_a =
            make_file_node_in_tree(&mut tree, "report.txt", 1024, Some("txt"), 1_700_000_000);
        tree.insert(idx_sub, file_a);

        let file_b =
            make_file_node_in_tree(&mut tree, "image.png", 2048, Some("png"), 1_700_001_000);
        tree.insert(idx_sub, file_b);

        let file_del =
            make_file_node_in_tree(&mut tree, "deleted.log", 512, Some("log"), 1_700_002_000);
        let idx_del = tree.insert(idx_sub, file_del);
        tree.tombstone(idx_del);

        tree
    }

    #[test]
    fn export_csv_excludes_deleted_and_has_correct_content() {
        let tree = build_test_tree();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();

        let result = export_tree(&tree, tree.root(), ExportFormat::Csv, &path);
        match result {
            ExportResult::Success { record_count, .. } => {
                assert_eq!(record_count, 4);
            }
            ExportResult::Error(e) => panic!("export_tree failed: {e}"),
        }

        let mut content = String::new();
        std::fs::File::open(&path)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        let lines: Vec<&str> = content.lines().collect();

        assert_eq!(
            lines[0],
            "path,name,size_bytes,size_human,is_dir,extension,modified_timestamp"
        );

        assert_eq!(lines.len(), 5);

        let has_report = lines.iter().any(|l| l.contains("report.txt"));
        let has_image = lines.iter().any(|l| l.contains("image.png"));
        let has_deleted = lines.iter().any(|l| l.contains("deleted.log"));
        assert!(has_report, "CSV should contain report.txt");
        assert!(has_image, "CSV should contain image.png");
        assert!(!has_deleted, "CSV should not contain deleted.log");

        let report_line = lines.iter().find(|l| l.contains("report.txt")).unwrap();
        assert!(report_line.contains("1024"));
        assert!(report_line.contains("txt"));
        assert!(report_line.contains("false"));
    }

    #[test]
    fn export_json_excludes_deleted_and_has_correct_content() {
        let tree = build_test_tree();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();

        let result = export_tree(&tree, tree.root(), ExportFormat::Json, &path);
        match result {
            ExportResult::Success { record_count, .. } => {
                assert_eq!(record_count, 4);
            }
            ExportResult::Error(e) => panic!("export_tree failed: {e}"),
        }

        let mut content = String::new();
        std::fs::File::open(&path)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&content).unwrap();

        assert_eq!(parsed.len(), 4);

        let names: Vec<&str> = parsed.iter().map(|v| v["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"report.txt"));
        assert!(names.contains(&"image.png"));
        assert!(!names.contains(&"deleted.log"));

        let report = parsed.iter().find(|v| v["name"] == "report.txt").unwrap();
        assert_eq!(report["size_bytes"], 1024);
        assert_eq!(report["is_dir"], false);
        assert_eq!(report["extension"], "txt");
        assert_eq!(report["modified_timestamp"], 1_700_000_000u64);
    }

    #[test]
    fn export_current_view_limits_to_subtree() {
        let mut tree = DirTree::new("/root");
        let subdir_a = make_dir_node("subdir_a");
        let idx_a = tree.insert(0, subdir_a);

        let file_a1 = make_file_node_in_tree(&mut tree, "a1.txt", 100, Some("txt"), 0);
        tree.insert(idx_a, file_a1);
        let file_a2 = make_file_node_in_tree(&mut tree, "a2.rs", 200, Some("rs"), 0);
        tree.insert(idx_a, file_a2);

        let subdir_b = make_dir_node("subdir_b");
        let idx_b = tree.insert(0, subdir_b);

        let file_b1 = make_file_node_in_tree(&mut tree, "b1.txt", 300, Some("txt"), 0);
        tree.insert(idx_b, file_b1);

        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();

        let result = export_tree(&tree, idx_a, ExportFormat::Json, &path);
        match result {
            ExportResult::Success { record_count, .. } => {
                assert_eq!(record_count, 3);
            }
            ExportResult::Error(e) => panic!("export_tree failed: {e}"),
        }

        let mut content = String::new();
        std::fs::File::open(&path)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&content).unwrap();

        assert_eq!(parsed.len(), 3);

        let names: Vec<&str> = parsed.iter().map(|v| v["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"subdir_a"));
        assert!(names.contains(&"a1.txt"));
        assert!(names.contains(&"a2.rs"));
        assert!(!names.contains(&"subdir_b"));
        assert!(!names.contains(&"b1.txt"));
        assert!(!names.contains(&"/root"));
    }

    fn build_duplicate_test_tree() -> (DirTree, usize, usize, usize, usize) {
        let mut tree = DirTree::new("/root");
        let subdir = make_dir_node("subdir");
        let idx_sub = tree.insert(0, subdir);

        let file_a = make_file_node_in_tree(&mut tree, "photo.jpg", 5000, Some("jpg"), 0);
        let idx_a = tree.insert(idx_sub, file_a);

        let file_b = make_file_node_in_tree(&mut tree, "photo_copy.jpg", 5000, Some("jpg"), 0);
        let idx_b = tree.insert(idx_sub, file_b);

        let file_c = make_file_node_in_tree(&mut tree, "data.csv", 3000, Some("csv"), 0);
        let idx_c = tree.insert(idx_sub, file_c);

        let file_d = make_file_node_in_tree(&mut tree, "data_backup.csv", 3000, Some("csv"), 0);
        let idx_d = tree.insert(idx_sub, file_d);

        (tree, idx_a, idx_b, idx_c, idx_d)
    }

    #[test]
    fn export_duplicates_csv_correct_groups_and_fields() {
        let (tree, idx_a, idx_b, idx_c, idx_d) = build_duplicate_test_tree();
        let groups = vec![
            crate::DuplicateGroup {
                node_indices: vec![idx_a, idx_b],
                wasted_bytes: 5000,
            },
            crate::DuplicateGroup {
                node_indices: vec![idx_c, idx_d],
                wasted_bytes: 3000,
            },
        ];

        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();

        let result = export_duplicates(&tree, &groups, ExportFormat::Csv, &path);
        match result {
            ExportResult::Success { record_count, .. } => {
                assert_eq!(record_count, 4);
            }
            ExportResult::Error(e) => panic!("export_duplicates failed: {e}"),
        }

        let mut content = String::new();
        std::fs::File::open(&path)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        let lines: Vec<&str> = content.lines().collect();

        assert_eq!(
            lines[0],
            "group_number,path,name,size_bytes,size_human,extension,wasted_bytes_in_group"
        );
        assert_eq!(lines.len(), 5);

        let group1_lines: Vec<&&str> = lines.iter().filter(|l| l.starts_with("1,")).collect();
        assert_eq!(group1_lines.len(), 2);
        for line in &group1_lines {
            assert!(line.contains("5000"));
            assert!(line.contains("jpg"));
        }

        let group2_lines: Vec<&&str> = lines.iter().filter(|l| l.starts_with("2,")).collect();
        assert_eq!(group2_lines.len(), 2);
        for line in &group2_lines {
            assert!(line.contains("3000"));
            assert!(line.contains("csv"));
        }
    }

    #[test]
    fn export_duplicates_json_correct_groups_and_fields() {
        let (tree, idx_a, idx_b, idx_c, idx_d) = build_duplicate_test_tree();
        let groups = vec![
            crate::DuplicateGroup {
                node_indices: vec![idx_a, idx_b],
                wasted_bytes: 5000,
            },
            crate::DuplicateGroup {
                node_indices: vec![idx_c, idx_d],
                wasted_bytes: 3000,
            },
        ];

        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();

        let result = export_duplicates(&tree, &groups, ExportFormat::Json, &path);
        match result {
            ExportResult::Success { record_count, .. } => {
                assert_eq!(record_count, 4);
            }
            ExportResult::Error(e) => panic!("export_duplicates failed: {e}"),
        }

        let mut content = String::new();
        std::fs::File::open(&path)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&content).unwrap();

        assert_eq!(parsed.len(), 4);

        let group1: Vec<&serde_json::Value> =
            parsed.iter().filter(|v| v["group_number"] == 1).collect();
        assert_eq!(group1.len(), 2);
        for record in &group1 {
            assert_eq!(record["size_bytes"], 5000);
            assert_eq!(record["wasted_bytes_in_group"], 5000);
            assert_eq!(record["extension"], "jpg");
        }

        let group2: Vec<&serde_json::Value> =
            parsed.iter().filter(|v| v["group_number"] == 2).collect();
        assert_eq!(group2.len(), 2);
        for record in &group2 {
            assert_eq!(record["size_bytes"], 3000);
            assert_eq!(record["wasted_bytes_in_group"], 3000);
            assert_eq!(record["extension"], "csv");
        }

        let names: Vec<&str> = parsed.iter().map(|v| v["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"photo.jpg"));
        assert!(names.contains(&"photo_copy.jpg"));
        assert!(names.contains(&"data.csv"));
        assert!(names.contains(&"data_backup.csv"));
    }

    #[test]
    fn export_duplicates_empty_groups() {
        let tree = build_test_tree();
        let groups: &[crate::DuplicateGroup] = &[];

        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();

        let result = export_duplicates(&tree, groups, ExportFormat::Csv, &path);
        match result {
            ExportResult::Success { record_count, .. } => {
                assert_eq!(record_count, 0);
            }
            ExportResult::Error(e) => panic!("export_duplicates failed: {e}"),
        }

        let mut content = String::new();
        std::fs::File::open(&path)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines[0],
            "group_number,path,name,size_bytes,size_human,extension,wasted_bytes_in_group"
        );

        let tmp_json = tempfile::NamedTempFile::new().unwrap();
        let path_json = tmp_json.path().to_path_buf();

        let result_json = export_duplicates(&tree, groups, ExportFormat::Json, &path_json);
        match result_json {
            ExportResult::Success { record_count, .. } => {
                assert_eq!(record_count, 0);
            }
            ExportResult::Error(e) => panic!("export_duplicates failed: {e}"),
        }

        let mut json_content = String::new();
        std::fs::File::open(&path_json)
            .unwrap()
            .read_to_string(&mut json_content)
            .unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_content).unwrap();
        assert!(parsed.is_empty());
    }
}
