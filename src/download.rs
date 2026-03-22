use std::path::Path;

use anyhow::{Context, Result};

const HUGGINGFACE_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

const SUPPORTED_MODELS: &[&str] = &[
    "large-v3-turbo-q5_0",
    "large-v3-turbo",
    "large-v3",
    "medium",
    "small",
    "base",
    "tiny",
];

pub fn model_url(model_name: &str) -> String {
    format!("{HUGGINGFACE_BASE_URL}/ggml-{model_name}.bin")
}

pub fn validate_model_name(name: &str) -> Result<()> {
    if SUPPORTED_MODELS.contains(&name) {
        Ok(())
    } else {
        anyhow::bail!(
            "Unsupported model: '{name}'. Supported models: {}",
            SUPPORTED_MODELS.join(", ")
        );
    }
}

pub fn download_model(model_name: &str, dest_dir: &Path) -> Result<()> {
    validate_model_name(model_name)?;

    std::fs::create_dir_all(dest_dir).context("Failed to create model directory")?;

    let url = model_url(model_name);
    let filename = format!("ggml-{model_name}.bin");
    let dest_path = dest_dir.join(&filename);

    if dest_path.exists() {
        eprintln!("Model already exists at {}", dest_path.display());
        return Ok(());
    }

    eprintln!("Downloading {filename}...");
    download_file(&url, &dest_path)?;
    eprintln!("Model saved to {}", dest_path.display());

    Ok(())
}

fn download_file(url: &str, dest: &Path) -> Result<()> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(url)
        .send()
        .context("Failed to start download")?
        .error_for_status()
        .context("Download failed")?;

    let total_size = response.content_length().unwrap_or(0);

    let pb = indicatif::ProgressBar::new(total_size);
    pb.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );

    let mut file = std::io::BufWriter::new(
        std::fs::File::create(dest).context("Failed to create destination file")?,
    );
    std::io::copy(&mut pb.wrap_read(response), &mut file)?;
    pb.finish_with_message("Done");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_url_format() {
        let url = model_url("tiny");
        assert_eq!(
            url,
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin"
        );
    }

    #[test]
    fn model_url_quantized() {
        let url = model_url("large-v3-turbo-q5_0");
        assert!(url.ends_with("ggml-large-v3-turbo-q5_0.bin"));
    }

    #[test]
    fn validate_supported_models() {
        assert!(validate_model_name("tiny").is_ok());
        assert!(validate_model_name("base").is_ok());
        assert!(validate_model_name("small").is_ok());
        assert!(validate_model_name("medium").is_ok());
        assert!(validate_model_name("large-v3").is_ok());
        assert!(validate_model_name("large-v3-turbo").is_ok());
        assert!(validate_model_name("large-v3-turbo-q5_0").is_ok());
    }

    #[test]
    fn validate_unsupported_model() {
        let result = validate_model_name("nonexistent");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unsupported model"));
        assert!(err.contains("nonexistent"));
    }

    #[test]
    fn download_model_validates_name_first() {
        let dir = tempfile::tempdir().unwrap();
        let result = download_model("invalid-model", dir.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unsupported model"));
    }

    #[test]
    fn download_model_skips_existing() {
        let dir = tempfile::tempdir().unwrap();
        let model_file = dir.path().join("ggml-tiny.bin");
        std::fs::write(&model_file, b"fake model data").unwrap();

        // Should succeed without network access since file exists
        let result = download_model("tiny", dir.path());
        assert!(result.is_ok());
    }
}
