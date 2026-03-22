use std::path::Path;

use anyhow::{Context, Result};

pub fn transcribe(model_path: &Path, wav_path: &Path) -> Result<String> {
    let ctx = whisper_rs::WhisperContext::new_with_params(
        &model_path.to_string_lossy(),
        whisper_rs::WhisperContextParameters::default(),
    )
    .context("Failed to load whisper model")?;

    #[cfg(feature = "coreml")]
    if !has_coreml_assets(model_path) {
        eprintln!(
            "Warning: CoreML model assets not found. Falling back to Metal/CPU. \
             Run `notetaker download-model` to fetch CoreML assets for faster transcription."
        );
    }

    let audio_data = read_wav_samples(wav_path)?;

    let mut state = ctx
        .create_state()
        .context("Failed to create whisper state")?;

    let mut params =
        whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
    params.set_print_progress(false);
    params.set_print_special(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_language(Some("en"));

    state
        .full(params, &audio_data)
        .context("Transcription failed")?;

    let num_segments = state.full_n_segments()?;
    let mut text = String::new();

    for i in 0..num_segments {
        let segment = state
            .full_get_segment_text(i)
            .context("Failed to get segment text")?;
        if !text.is_empty() && !segment.starts_with(' ') {
            text.push(' ');
        }
        text.push_str(segment.trim());
    }

    Ok(text)
}

fn read_wav_samples(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path).context("Failed to open WAV file")?;
    let spec = reader.spec();

    if spec.sample_rate != 16000 {
        anyhow::bail!(
            "Expected 16kHz WAV file, got {} Hz. Record with `notetaker record` or convert to 16kHz mono first.",
            spec.sample_rate
        );
    }
    if spec.channels != 1 {
        anyhow::bail!(
            "Expected mono WAV file, got {} channels. Record with `notetaker record` or convert to mono first.",
            spec.channels
        );
    }

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .map(|s| s.map(|v| v as f32 / i16::MAX as f32))
            .collect::<Result<Vec<_>, _>>()?,
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?,
    };

    Ok(samples)
}

#[cfg(any(feature = "coreml", test))]
fn has_coreml_assets(model_path: &Path) -> bool {
    let model_dir = model_path.parent().unwrap_or(Path::new("."));
    let stem = model_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let coreml_dir = model_dir.join(format!("{stem}-encoder.mlmodelc"));
    coreml_dir.exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_coreml_assets_returns_false_when_missing() {
        let path = Path::new("/nonexistent/ggml-tiny.bin");
        assert!(!has_coreml_assets(path));
    }

    #[test]
    fn has_coreml_assets_returns_true_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let model_path = dir.path().join("ggml-tiny.bin");
        std::fs::write(&model_path, b"fake model").unwrap();
        let coreml_dir = dir.path().join("ggml-tiny-encoder.mlmodelc");
        std::fs::create_dir(&coreml_dir).unwrap();

        assert!(has_coreml_assets(&model_path));
    }

    #[test]
    fn read_wav_samples_reads_i16() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        writer.write_sample(0i16).unwrap();
        writer.write_sample(i16::MAX).unwrap();
        writer.write_sample(i16::MIN).unwrap();
        writer.finalize().unwrap();

        let samples = read_wav_samples(&path).unwrap();
        assert_eq!(samples.len(), 3);
        assert!((samples[0] - 0.0).abs() < 0.001);
        assert!((samples[1] - 1.0).abs() < 0.001);
    }

    #[test]
    fn read_wav_samples_rejects_wrong_sample_rate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_48k.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        writer.write_sample(0i16).unwrap();
        writer.finalize().unwrap();

        let result = read_wav_samples(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("16kHz"));
    }

    #[test]
    fn read_wav_samples_rejects_stereo() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_stereo.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        writer.write_sample(0i16).unwrap();
        writer.write_sample(0i16).unwrap();
        writer.finalize().unwrap();

        let result = read_wav_samples(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("mono"));
    }

    #[test]
    fn transcribe_fails_with_missing_model() {
        let dir = tempfile::tempdir().unwrap();
        let wav_path = dir.path().join("test.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&wav_path, spec).unwrap();
        writer.write_sample(0i16).unwrap();
        writer.finalize().unwrap();

        let model_path = Path::new("/nonexistent/model.bin");
        let result = transcribe(model_path, &wav_path);
        assert!(result.is_err());
    }
}
