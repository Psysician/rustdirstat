use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    #[default]
    SizeDesc,
    SizeAsc,
    NameAsc,
    NameDesc,
}

impl SortOrder {
    pub const ALL: &[SortOrder] = &[
        SortOrder::SizeDesc,
        SortOrder::SizeAsc,
        SortOrder::NameAsc,
        SortOrder::NameDesc,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::SizeDesc => "Size (largest first)",
            Self::SizeAsc => "Size (smallest first)",
            Self::NameAsc => "Name (A-Z)",
            Self::NameDesc => "Name (Z-A)",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorScheme {
    #[default]
    Default,
    Dark,
    Light,
}

impl ColorScheme {
    pub const ALL: &[ColorScheme] = &[ColorScheme::Default, ColorScheme::Dark, ColorScheme::Light];

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "System (auto)",
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CustomCommand {
    pub name: String,
    pub template: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub exclude_patterns: Vec<String>,
    pub custom_commands: Vec<CustomCommand>,
    pub color_scheme: ColorScheme,
    pub default_sort: SortOrder,
    pub recent_paths: Vec<PathBuf>,
    pub max_recent_paths: usize,
    pub follow_symlinks: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            exclude_patterns: Vec::new(),
            custom_commands: Vec::new(),
            color_scheme: ColorScheme::default(),
            default_sort: SortOrder::default(),
            recent_paths: Vec::new(),
            max_recent_paths: 10,
            follow_symlinks: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_config_defaults() {
        let config = AppConfig::default();
        assert!(config.exclude_patterns.is_empty());
        assert!(config.custom_commands.is_empty());
        assert_eq!(config.color_scheme, ColorScheme::Default);
        assert_eq!(config.default_sort, SortOrder::SizeDesc);
        assert!(config.recent_paths.is_empty());
    }

    #[test]
    fn app_config_serde_roundtrip() {
        let config = AppConfig {
            exclude_patterns: vec!["*.tmp".to_string(), "node_modules".to_string()],
            custom_commands: vec![CustomCommand {
                name: "Open Terminal".to_string(),
                template: "cd {path} && bash".to_string(),
            }],
            color_scheme: ColorScheme::Default,
            default_sort: SortOrder::NameAsc,
            recent_paths: vec![PathBuf::from("/home/user/docs")],
            max_recent_paths: 10,
            follow_symlinks: false,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.exclude_patterns, config.exclude_patterns);
        assert_eq!(deserialized.color_scheme, config.color_scheme);
        assert_eq!(deserialized.default_sort, config.default_sort);
        assert_eq!(deserialized.recent_paths, config.recent_paths);
        assert_eq!(deserialized.custom_commands.len(), 1);
        assert_eq!(deserialized.custom_commands[0].name, "Open Terminal");
        assert_eq!(
            deserialized.custom_commands[0].template,
            "cd {path} && bash"
        );
    }

    #[test]
    fn new_fields_serde_roundtrip() {
        let config = AppConfig {
            max_recent_paths: 25,
            follow_symlinks: true,
            ..AppConfig::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.max_recent_paths, 25);
        assert!(deserialized.follow_symlinks);
    }

    #[test]
    fn missing_new_fields_deserialize_with_defaults() {
        let json = r#"{"exclude_patterns":[],"custom_commands":[],"color_scheme":"default","default_sort":"size_desc","recent_paths":[]}"#;
        let deserialized: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(deserialized.max_recent_paths, 10);
        assert!(!deserialized.follow_symlinks);
    }

    #[test]
    fn sort_order_serde_values() {
        assert_eq!(
            serde_json::to_string(&SortOrder::SizeDesc).unwrap(),
            r#""size_desc""#
        );
        assert_eq!(
            serde_json::to_string(&SortOrder::NameAsc).unwrap(),
            r#""name_asc""#
        );
    }

    #[test]
    fn sort_order_labels() {
        assert_eq!(SortOrder::SizeDesc.label(), "Size (largest first)");
        assert_eq!(SortOrder::NameAsc.label(), "Name (A-Z)");
    }

    #[test]
    fn color_scheme_dark_serde_roundtrip() {
        let json = serde_json::to_string(&ColorScheme::Dark).unwrap();
        assert_eq!(json, r#""dark""#);
        let deserialized: ColorScheme = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ColorScheme::Dark);
    }

    #[test]
    fn color_scheme_light_serde_roundtrip() {
        let json = serde_json::to_string(&ColorScheme::Light).unwrap();
        assert_eq!(json, r#""light""#);
        let deserialized: ColorScheme = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ColorScheme::Light);
    }

    #[test]
    fn color_scheme_default_backward_compat() {
        let deserialized: ColorScheme = serde_json::from_str(r#""default""#).unwrap();
        assert_eq!(deserialized, ColorScheme::Default);
    }
}
