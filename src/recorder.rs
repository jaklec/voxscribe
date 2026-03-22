use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use hound::{WavSpec, WavWriter};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

use crate::config::AppConfig;

const TARGET_SAMPLE_RATE: u32 = 16000;

pub struct Recorder {
    device: cpal::Device,
    device_config: cpal::SupportedStreamConfig,
    temp_path: PathBuf,
    _temp_file: tempfile::TempPath,
}

impl Recorder {
    pub fn new(_config: &AppConfig) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .context("No microphone found. Please connect a microphone and try again.")?;

        let device_config = device
            .default_input_config()
            .context("Failed to get default input config")?;

        let temp_file = tempfile::Builder::new()
            .suffix(".wav")
            .tempfile()
            .context("Failed to create temp file")?;
        let temp_path = temp_file.into_temp_path();

        Ok(Self {
            device,
            device_config,
            temp_path: temp_path.to_path_buf(),
            _temp_file: temp_path,
        })
    }
}

pub struct RecordingHandle {
    paused: std::sync::Arc<AtomicBool>,
    writer_thread: Option<std::thread::JoinHandle<Result<PathBuf>>>,
    stop_flag: std::sync::Arc<AtomicBool>,
    _stream: cpal::Stream,
    _temp_file: tempfile::TempPath,
}

impl RecordingHandle {
    pub fn pause(&self) {
        self.paused.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
    }

    pub fn stop(mut self) -> Result<PathBuf> {
        self.stop_flag.store(true, Ordering::Relaxed);
        self.writer_thread
            .take()
            .expect("thread already joined")
            .join()
            .map_err(|_| anyhow::anyhow!("Recording thread panicked"))?
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }
}

impl Drop for RecordingHandle {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(thread) = self.writer_thread.take() {
            let _ = thread.join();
        }
    }
}

pub fn start_recording(recorder: Recorder) -> Result<RecordingHandle> {
    let temp_file = recorder._temp_file;
    let wav_path = recorder.temp_path.clone();
    let native_rate = recorder.device_config.sample_rate().0;
    let channels = recorder.device_config.channels() as usize;
    let sample_format = recorder.device_config.sample_format();
    let stream_config: cpal::StreamConfig = recorder.device_config.clone().into();

    let paused = std::sync::Arc::new(AtomicBool::new(false));
    let stop_flag = std::sync::Arc::new(AtomicBool::new(false));
    let (samples_tx, samples_rx) = mpsc::channel::<Vec<f32>>();

    let paused_callback = paused.clone();
    let err_fn = |err| eprintln!("Audio stream error: {err}");

    let stream = match sample_format {
        SampleFormat::F32 => recorder.device.build_input_stream(
            &stream_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if !paused_callback.load(Ordering::Relaxed) {
                    let _ = samples_tx.send(data.to_vec());
                }
            },
            err_fn,
            None,
        )?,
        SampleFormat::I16 => recorder.device.build_input_stream(
            &stream_config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                if !paused_callback.load(Ordering::Relaxed) {
                    let floats: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                    let _ = samples_tx.send(floats);
                }
            },
            err_fn,
            None,
        )?,
        _ => anyhow::bail!("Unsupported sample format: {sample_format:?}"),
    };

    stream.play()?;

    let stop_clone = stop_flag.clone();
    let writer_thread = std::thread::spawn(move || -> Result<PathBuf> {
        let spec = WavSpec {
            channels: 1,
            sample_rate: TARGET_SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer =
            WavWriter::create(&wav_path, spec).context("Failed to create WAV writer")?;

        let needs_resample = native_rate != TARGET_SAMPLE_RATE;
        let mut resampler = if needs_resample {
            Some(create_resampler(native_rate, TARGET_SAMPLE_RATE)?)
        } else {
            None
        };
        let mut resample_buffer: Vec<f32> = Vec::new();

        loop {
            if stop_clone.load(Ordering::Relaxed) {
                break;
            }

            match samples_rx.recv_timeout(Duration::from_millis(10)) {
                Ok(samples) => {
                    let mono = to_mono(&samples, channels);
                    if let Some(ref mut r) = resampler {
                        resample_buffer.extend_from_slice(&mono);
                        write_resampled(r, &mut resample_buffer, &mut writer)?;
                    } else {
                        write_samples(&mono, &mut writer)?;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        // Drain remaining samples
        while let Ok(samples) = samples_rx.try_recv() {
            let mono = to_mono(&samples, channels);
            if let Some(ref mut r) = resampler {
                resample_buffer.extend_from_slice(&mono);
                write_resampled(r, &mut resample_buffer, &mut writer)?;
            } else {
                write_samples(&mono, &mut writer)?;
            }
        }

        writer.finalize()?;
        Ok(wav_path)
    });

    Ok(RecordingHandle {
        paused,
        writer_thread: Some(writer_thread),
        stop_flag,
        _stream: stream,
        _temp_file: temp_file,
    })
}

pub fn run_non_interactive(recorder: Recorder) -> Result<PathBuf> {
    let handle = start_recording(recorder)?;

    let stop_flag = handle.stop_flag.clone();
    ctrlc::set_handler(move || {
        stop_flag.store(true, Ordering::Relaxed);
    })?;

    eprintln!("Recording... Press Ctrl+C to stop.");
    while !handle.stop_flag.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(100));
    }

    handle.stop()
}

fn to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels == 1 {
        return samples.to_vec();
    }
    samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

fn create_resampler(from_rate: u32, to_rate: u32) -> Result<SincFixedIn<f32>> {
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };
    let resampler =
        SincFixedIn::<f32>::new(to_rate as f64 / from_rate as f64, 2.0, params, 1024, 1)?;
    Ok(resampler)
}

fn write_resampled(
    resampler: &mut SincFixedIn<f32>,
    buffer: &mut Vec<f32>,
    writer: &mut WavWriter<std::io::BufWriter<std::fs::File>>,
) -> Result<()> {
    let chunk_size = resampler.input_frames_next();
    while buffer.len() >= chunk_size {
        let chunk: Vec<f32> = buffer.drain(..chunk_size).collect();
        let output = resampler.process(&[chunk], None)?;
        write_samples(&output[0], writer)?;
    }
    Ok(())
}

fn write_samples(
    samples: &[f32],
    writer: &mut WavWriter<std::io::BufWriter<std::fs::File>>,
) -> Result<()> {
    for &sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let value = (clamped * i16::MAX as f32) as i16;
        writer.write_sample(value)?;
    }
    Ok(())
}

pub fn wav_duration(path: &Path) -> Result<f64> {
    let reader = hound::WavReader::open(path).context("Failed to read WAV file")?;
    let spec = reader.spec();
    let num_samples = reader.len() as f64;
    let duration = num_samples / spec.sample_rate as f64 / spec.channels as f64;
    Ok(duration)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_mono_single_channel() {
        let samples = vec![0.5, -0.5, 0.3];
        let mono = to_mono(&samples, 1);
        assert_eq!(mono, vec![0.5, -0.5, 0.3]);
    }

    #[test]
    fn to_mono_stereo() {
        let samples = vec![0.4, 0.6, -0.2, -0.8];
        let mono = to_mono(&samples, 2);
        assert_eq!(mono.len(), 2);
        assert!((mono[0] - 0.5).abs() < f32::EPSILON);
        assert!((mono[1] - (-0.5)).abs() < f32::EPSILON);
    }

    #[test]
    fn to_mono_four_channels() {
        let samples = vec![0.1, 0.2, 0.3, 0.4];
        let mono = to_mono(&samples, 4);
        assert_eq!(mono.len(), 1);
        assert!((mono[0] - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn write_samples_clamps_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");
        let spec = WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = WavWriter::create(&path, spec).unwrap();
        write_samples(&[2.0, -2.0, 0.5], &mut writer).unwrap();
        writer.finalize().unwrap();

        let reader = hound::WavReader::open(&path).unwrap();
        let samples: Vec<i16> = reader.into_samples::<i16>().map(|s| s.unwrap()).collect();
        assert_eq!(samples.len(), 3);
        assert_eq!(samples[0], i16::MAX);
        assert_eq!(samples[1], -i16::MAX);
        assert!(samples[2] > 0);
    }

    #[test]
    fn wav_duration_correct() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");
        let spec = WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = WavWriter::create(&path, spec).unwrap();
        for _ in 0..16000 {
            writer.write_sample(0i16).unwrap();
        }
        writer.finalize().unwrap();

        let duration = wav_duration(&path).unwrap();
        assert!((duration - 1.0).abs() < 0.001);
    }

    #[test]
    fn wav_duration_half_second() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");
        let spec = WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = WavWriter::create(&path, spec).unwrap();
        for _ in 0..8000 {
            writer.write_sample(0i16).unwrap();
        }
        writer.finalize().unwrap();

        let duration = wav_duration(&path).unwrap();
        assert!((duration - 0.5).abs() < 0.001);
    }

    #[test]
    fn create_resampler_succeeds() {
        let resampler = create_resampler(48000, 16000);
        assert!(resampler.is_ok());
    }

    #[test]
    fn resample_produces_output() {
        let mut resampler = create_resampler(48000, 16000).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");
        let spec = WavSpec {
            channels: 1,
            sample_rate: TARGET_SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = WavWriter::create(&path, spec).unwrap();

        let chunk_size = resampler.input_frames_next();
        let mut buffer: Vec<f32> = (0..chunk_size * 3)
            .map(|i| (i as f32 * 0.01).sin())
            .collect();

        write_resampled(&mut resampler, &mut buffer, &mut writer).unwrap();
        writer.finalize().unwrap();

        let reader = hound::WavReader::open(&path).unwrap();
        let output_samples = reader.len();
        assert!(output_samples > 0);
    }
}
