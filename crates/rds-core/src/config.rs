use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    pub color_scheme: String,
    pub default_sort: String,
    pub recent_paths: Vec<PathBuf>,
    pub max_recent_paths: usize,
    pub follow_symlinks: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            exclude_patterns: Vec::new(),
            custom_commands: Vec::new(),
            color_scheme: "default".to_string(),
            default_sort: "size_desc".to_string(),
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
        assert_eq!(config.color_scheme, "default");
        assert_eq!(config.default_sort, "size_desc");
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
            color_scheme: "dark".to_string(),
            default_sort: "name_asc".to_string(),
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
}
