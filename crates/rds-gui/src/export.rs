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
    _scope: ExportScope,
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
        if node.deleted {
            continue;
        }
        let path = tree.path(index).to_string_lossy().to_string();
        records.push(ExportRecord {
            path,
            name: node.name.clone(),
            size_bytes: node.size,
            size_human: crate::format_bytes(node.size),
            is_dir: node.is_dir,
            extension: node.extension.clone().unwrap_or_default(),
            modified_timestamp: node.modified,
        });
        for &child_idx in tree.children(index) {
            stack.push(child_idx);
        }
    }

    let file = match std::fs::File::create(output_path) {
        Ok(f) => f,
        Err(e) => return ExportResult::Error(e.to_string()),
    };

    let record_count = records.len();
    let path_string = output_path.to_string_lossy().to_string();

    match format {
        ExportFormat::Csv => {
            let mut writer = csv::Writer::from_writer(file);
            for record in &records {
                if let Err(e) = writer.serialize(record) {
                    return ExportResult::Error(e.to_string());
                }
            }
            if let Err(e) = writer.flush() {
                return ExportResult::Error(e.to_string());
            }
        }
        ExportFormat::Json => {
            if let Err(e) = serde_json::to_writer_pretty(file, &records) {
                return ExportResult::Error(e.to_string());
            }
        }
    }

    ExportResult::Success {
        path: path_string,
        record_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rds_core::tree::{DirTree, FileNode};
    use std::io::Read;

    fn make_file_node(name: &str, size: u64, ext: Option<&str>, modified: Option<u64>) -> FileNode {
        FileNode {
            name: name.to_string(),
            size,
            is_dir: false,
            children: Vec::new(),
            parent: None,
            extension: ext.map(|s| s.to_string()),
            modified,
            deleted: false,
        }
    }

    fn make_dir_node(name: &str) -> FileNode {
        FileNode {
            name: name.to_string(),
            size: 0,
            is_dir: true,
            children: Vec::new(),
            parent: None,
            extension: None,
            modified: None,
            deleted: false,
        }
    }

    fn build_test_tree() -> DirTree {
        let mut tree = DirTree::new("/root");
        let subdir = make_dir_node("subdir");
        let idx_sub = tree.insert(0, subdir);

        let file_a = make_file_node("report.txt", 1024, Some("txt"), Some(1_700_000_000));
        tree.insert(idx_sub, file_a);

        let file_b = make_file_node("image.png", 2048, Some("png"), Some(1_700_001_000));
        tree.insert(idx_sub, file_b);

        let file_del = make_file_node("deleted.log", 512, Some("log"), Some(1_700_002_000));
        let idx_del = tree.insert(idx_sub, file_del);
        tree.tombstone(idx_del);

        tree
    }

    #[test]
    fn export_csv_excludes_deleted_and_has_correct_content() {
        let tree = build_test_tree();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();

        let result = export_tree(&tree, tree.root(), ExportScope::FullTree, ExportFormat::Csv, &path);
        match result {
            ExportResult::Success { record_count, .. } => {
                assert_eq!(record_count, 4);
            }
            ExportResult::Error(e) => panic!("export_tree failed: {e}"),
        }

        let mut content = String::new();
        std::fs::File::open(&path).unwrap().read_to_string(&mut content).unwrap();
        let lines: Vec<&str> = content.lines().collect();

        assert_eq!(lines[0], "path,name,size_bytes,size_human,is_dir,extension,modified_timestamp");

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

        let result = export_tree(&tree, tree.root(), ExportScope::FullTree, ExportFormat::Json, &path);
        match result {
            ExportResult::Success { record_count, .. } => {
                assert_eq!(record_count, 4);
            }
            ExportResult::Error(e) => panic!("export_tree failed: {e}"),
        }

        let mut content = String::new();
        std::fs::File::open(&path).unwrap().read_to_string(&mut content).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&content).unwrap();

        assert_eq!(parsed.len(), 4);

        let names: Vec<&str> = parsed.iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect();
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

        let file_a1 = make_file_node("a1.txt", 100, Some("txt"), None);
        tree.insert(idx_a, file_a1);
        let file_a2 = make_file_node("a2.rs", 200, Some("rs"), None);
        tree.insert(idx_a, file_a2);

        let subdir_b = make_dir_node("subdir_b");
        let idx_b = tree.insert(0, subdir_b);

        let file_b1 = make_file_node("b1.txt", 300, Some("txt"), None);
        tree.insert(idx_b, file_b1);

        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();

        let result = export_tree(&tree, idx_a, ExportScope::CurrentView, ExportFormat::Json, &path);
        match result {
            ExportResult::Success { record_count, .. } => {
                assert_eq!(record_count, 3);
            }
            ExportResult::Error(e) => panic!("export_tree failed: {e}"),
        }

        let mut content = String::new();
        std::fs::File::open(&path).unwrap().read_to_string(&mut content).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&content).unwrap();

        assert_eq!(parsed.len(), 3);

        let names: Vec<&str> = parsed.iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"subdir_a"));
        assert!(names.contains(&"a1.txt"));
        assert!(names.contains(&"a2.rs"));
        assert!(!names.contains(&"subdir_b"));
        assert!(!names.contains(&"b1.txt"));
        assert!(!names.contains(&"/root"));
    }
}
