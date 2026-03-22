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
