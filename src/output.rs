use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::Local;

use crate::cli::RecordArgs;
use crate::config::AppConfig;

pub fn timestamp_filename(extension: &str) -> String {
    format!("{}.{extension}", Local::now().format("%Y-%m-%dT%H-%M-%S"))
}

pub fn resolve_output_path(config: &AppConfig, output_override: Option<&Path>) -> PathBuf {
    match output_override {
        Some(path) if path.is_dir() => path.join(timestamp_filename("txt")),
        Some(path) => path.to_path_buf(),
        None => config.resolved_output_dir().join(timestamp_filename("txt")),
    }
}

pub fn resolve_audio_path(config: &AppConfig, args: &RecordArgs) -> PathBuf {
    let dir = args
        .audio_dir
        .as_deref()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| config.resolved_output_dir());
    dir.join(timestamp_filename("wav"))
}

pub fn write_transcription(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_filename_has_correct_extension() {
        let name = timestamp_filename("txt");
        assert!(name.ends_with(".txt"));
        assert!(name.len() > 4);
    }

    #[test]
    fn timestamp_filename_wav() {
        let name = timestamp_filename("wav");
        assert!(name.ends_with(".wav"));
    }

    #[test]
    fn timestamp_filename_format() {
        let name = timestamp_filename("txt");
        // Should match pattern like 2026-03-20T14-30-00.txt
        let parts: Vec<&str> = name.split('.').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1], "txt");
        assert!(parts[0].contains('T'));
        assert!(parts[0].contains('-'));
    }

    #[test]
    fn resolve_output_path_with_file_override() {
        let config = AppConfig::default();
        let path = resolve_output_path(&config, Some(Path::new("/tmp/my-notes.txt")));
        assert_eq!(path, PathBuf::from("/tmp/my-notes.txt"));
    }

    #[test]
    fn resolve_output_path_with_dir_override() {
        let dir = tempfile::tempdir().unwrap();
        let config = AppConfig::default();
        let path = resolve_output_path(&config, Some(dir.path()));
        assert!(path.starts_with(dir.path()));
        assert!(path.to_string_lossy().ends_with(".txt"));
    }

    #[test]
    fn resolve_output_path_uses_config_default() {
        let config = AppConfig {
            output_dir: "/tmp/test-notes".to_string(),
            ..AppConfig::default()
        };
        let path = resolve_output_path(&config, None);
        assert!(path.starts_with("/tmp/test-notes"));
        assert!(path.to_string_lossy().ends_with(".txt"));
    }

    #[test]
    fn write_transcription_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        write_transcription(&path, "Hello, world!").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "Hello, world!");
    }

    #[test]
    fn write_transcription_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("dir").join("test.txt");
        write_transcription(&path, "nested content").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "nested content");
    }
}
