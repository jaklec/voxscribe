use std::path::PathBuf;

use anyhow::Result;
use serde::Deserialize;

use crate::cli::Cli;

const DEFAULT_OUTPUT_DIR: &str = "~/transcriptions";
const DEFAULT_MODEL_PATH: &str = "~/.local/share/notetaker/models";
const DEFAULT_MODEL: &str = "large-v3-turbo-q5_0";

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub output_dir: String,
    pub model_path: String,
    pub model: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            output_dir: DEFAULT_OUTPUT_DIR.to_string(),
            model_path: DEFAULT_MODEL_PATH.to_string(),
            model: DEFAULT_MODEL.to_string(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let config_path = config_file_path();
        if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path)?;
            let config: AppConfig = toml::from_str(&contents)?;
            Ok(config)
        } else {
            Ok(AppConfig::default())
        }
    }

    pub fn merge_cli(mut self, cli: &Cli) -> Self {
        use crate::cli::Command;

        if let Command::DownloadModel(args) = &cli.command {
            if let Some(ref model) = args.model {
                self.model = model.clone();
            }
        }
        self
    }

    pub fn resolved_output_dir(&self) -> PathBuf {
        expand_tilde(&self.output_dir)
    }

    pub fn resolved_model_dir(&self) -> PathBuf {
        expand_tilde(&self.model_path)
    }

    pub fn resolved_model_path(&self) -> PathBuf {
        self.resolved_model_dir().join(model_filename(&self.model))
    }
}

pub fn model_filename(model_name: &str) -> String {
    format!("ggml-{}.bin", model_name)
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    } else if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

fn config_file_path() -> PathBuf {
    dirs::config_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("notetaker")
        .join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let config = AppConfig::default();
        assert_eq!(config.output_dir, "~/transcriptions");
        assert_eq!(config.model_path, "~/.local/share/notetaker/models");
        assert_eq!(config.model, "large-v3-turbo-q5_0");
    }

    #[test]
    fn parse_full_toml_config() {
        let toml_str = r#"
            output_dir = "/tmp/notes"
            model_path = "/opt/models"
            model = "tiny"
        "#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.output_dir, "/tmp/notes");
        assert_eq!(config.model_path, "/opt/models");
        assert_eq!(config.model, "tiny");
    }

    #[test]
    fn parse_partial_toml_uses_defaults() {
        let toml_str = r#"
            model = "base"
        "#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.output_dir, "~/transcriptions");
        assert_eq!(config.model, "base");
    }

    #[test]
    fn expand_tilde_with_home() {
        let expanded = expand_tilde("~/transcriptions");
        assert!(!expanded.to_string_lossy().starts_with('~'));
        assert!(expanded.to_string_lossy().ends_with("transcriptions"));
    }

    #[test]
    fn expand_bare_tilde() {
        let expanded = expand_tilde("~");
        assert!(!expanded.to_string_lossy().contains('~'));
        assert!(expanded.is_absolute());
    }

    #[test]
    fn expand_tilde_absolute_path_unchanged() {
        let expanded = expand_tilde("/tmp/notes");
        assert_eq!(expanded, PathBuf::from("/tmp/notes"));
    }

    #[test]
    fn model_filename_format() {
        assert_eq!(
            model_filename("large-v3-turbo-q5_0"),
            "ggml-large-v3-turbo-q5_0.bin"
        );
        assert_eq!(model_filename("tiny"), "ggml-tiny.bin");
    }

    #[test]
    fn resolved_model_path_combines_dir_and_filename() {
        let config = AppConfig {
            output_dir: "/tmp".to_string(),
            model_path: "/opt/models".to_string(),
            model: "tiny".to_string(),
        };
        assert_eq!(
            config.resolved_model_path(),
            PathBuf::from("/opt/models/ggml-tiny.bin")
        );
    }

    #[test]
    fn load_returns_defaults_when_no_config_file() {
        let config = AppConfig::load().unwrap();
        assert_eq!(config.model, DEFAULT_MODEL);
    }
}
