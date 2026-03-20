# Notetaker CLI — Design Spec

## Overview

A Rust CLI tool that records audio from the system microphone, then transcribes
it locally using whisper.cpp. Built as a lightweight replacement for MacWhisper,
designed to integrate into a GTD workflow where transcriptions feed into Claude
Code's `/take-notes` skill for meeting summaries and action extraction.

## CLI Interface

```
notetaker record              # Interactive recording session
notetaker record --no-interact # Non-interactive (Ctrl+C to stop, no auto-transcribe)
notetaker record --output <path>     # Override transcription output path
notetaker record --keep-audio        # Keep the WAV file after transcription
notetaker transcribe <file>          # Transcribe an existing audio file
notetaker transcribe <file> --output <path>
notetaker download-model             # Download the whisper model
notetaker download-model --model <name>  # Specify model variant
```

All CLI flags override their config file equivalents.

## Configuration

File location: `~/.config/notetaker/config.toml`

```toml
output_dir = "~/transcriptions"
model_path = "~/.local/share/notetaker/models"
model = "large-v3-turbo-q5_0"
```

If no config file exists, these defaults are used. The tool does not create a
config file automatically — defaults are hardcoded. Tilde (`~`) in config paths
is expanded to the user's home directory at runtime via `dirs::home_dir()`.
All values can be overridden via CLI flags.

## Architecture

Single-threaded with blocking I/O. Audio capture runs in a dedicated thread,
keyboard input is handled on the main thread, and they communicate via
`std::sync::mpsc` channels. Transcription runs synchronously after recording
completes.

### Modules

```
┌─────────┐    ┌──────────┐    ┌──────────────┐    ┌────────┐
│  CLI     │───>│ Recorder │───>│ Transcriber  │───>│ Output │
│ (clap)   │    │ (cpal)   │    │ (whisper-rs) │    │ (file) │
└─────────┘    └──────────┘    └──────────────┘    └────────┘
                    ^
                    │
               ┌──────────┐
               │    UI     │
               │(terminal) │
               └──────────┘
```

- **cli** — Parses arguments with `clap`, loads config from TOML, merges CLI
  overrides, and dispatches to the appropriate action.
- **recorder** — Captures audio from the default input device via `cpal`.
  Writes 16-bit PCM WAV at 16kHz mono (whisper.cpp's expected format) to a temp
  file. Audio capture runs in a background thread. Exposes `pause()`,
  `resume()`, `stop()` via channel commands sent from the UI.
- **transcriber** — Takes a WAV file path, loads the whisper model, runs
  inference via `whisper-rs`, and returns the transcribed plain text.
- **ui** — In interactive mode: switches the terminal to raw mode via
  `crossterm`, reads keyboard input on the main thread, displays elapsed time
  and status (`Recording...`, `Paused`, `Transcribing...`). Sends commands to
  the recorder via channel. Keybindings: `[space]` pause/resume, `[q]` stop.
  The elapsed timer shows only active recording time (pauses are not counted).
- **output** — Writes the transcription text to the resolved output file path.

### Interactive Session Flow

1. CLI resolves config + flags, determines output path.
2. Recorder starts capturing audio to a temp WAV file.
3. UI shows status and elapsed timer, listens for keyboard input.
4. `[space]` toggles pause/resume (paused: audio callback discards samples,
   stream stays open).
5. `[q]` or Ctrl+C stops recording, finalizes the WAV file.
6. Transcriber processes the WAV file, UI shows `Transcribing...`.
7. Output writes plain text to the resolved path, prints the file location.
8. Temp WAV file is deleted unless `--keep-audio` is set.

### Non-Interactive Flow

1. Same recording setup but no UI — Ctrl+C triggers stop via `ctrlc` crate
   signal handler (since there is no raw terminal mode to capture key events).
2. Saves the WAV file to the output location with timestamp-based naming
   (e.g. `2026-03-20T14-30-00.wav`). The `--output` flag controls the WAV
   file destination in this mode. No auto-transcribe.

### Transcribe Subcommand Flow

1. Takes an existing WAV file path as input.
2. Loads model, runs transcription, writes output text file.

## Recording & Audio

- **Library:** `cpal` (standard Rust audio I/O, macOS CoreAudio backend).
- **Format:** WAV, 16-bit PCM, 16kHz, mono — whisper.cpp's expected input.
- **Sample rate handling:** macOS CoreAudio typically exposes devices at their
  native sample rate (44.1kHz or 48kHz). `cpal` does not resample. The recorder
  must capture at the device's native sample rate and resample to 16kHz before
  writing the WAV. Use the `rubato` crate for high-quality resampling.
- **Temp storage:** `tempfile` crate. On completion, either moved to output
  location (`--keep-audio`) or deleted after transcription. Note: `hound`'s
  `WavWriter` calls `finalize()` on drop, so abrupt termination (SIGKILL) will
  still produce a valid WAV header in most cases.
- **Pause behavior:** The `cpal` audio callback discards incoming samples while
  paused. The audio stream stays open to avoid device re-initialization on
  resume.

## Transcription

- **Library:** `whisper-rs` with the `coreml` feature flag enabled for Neural
  Engine acceleration on Apple Silicon.
- **Build prerequisites:** Xcode command-line tools must be installed (required
  for linking CoreML and Accelerate frameworks).
- **Default model:** Large v3 Turbo quantized (`q5_0`).
- **CoreML model assets:** Enabling CoreML requires a compiled `.mlmodelc`
  directory alongside the standard `.bin` weights. The `download-model`
  subcommand must fetch both the GGML weights and the pre-built CoreML model
  assets from Hugging Face. If CoreML assets are missing at runtime, whisper-rs
  falls back to Metal/CPU — the tool should warn when this happens.
- **Model download:** The `download-model` subcommand fetches from the
  `ggerganov/whisper.cpp` Hugging Face repository into the configured
  `model_path` directory. Shows a progress bar via `indicatif`. Supported
  models: `large-v3-turbo-q5_0` (default), `large-v3-turbo`, `large-v3`,
  `medium`, `base`, `small`, `tiny`.
- **Process:** Load model, feed WAV file, extract text segments, concatenate
  into plain text output.

## Output

- **Default format:** Plain text (`.txt`).
- **File naming:** Timestamp-based, e.g. `2026-03-20T14-30-00.txt`.
- **Location:** Configured `output_dir`, overridable via `--output` flag. If
  `--output` points to a directory, auto-generate the filename in that
  directory. If it points to a file path, use it as-is.
- **Future:** JSON output with timestamps and segments (not in v1).

## Error Handling

- **No microphone:** Detect at startup, exit with clear error before entering
  the session.
- **Model not found:** Check for model file on `record` or `transcribe`,
  suggest running `download-model` if missing.
- **Empty recording:** If stopped within < 1 second, skip transcription and
  warn.
- **Disk/write errors:** Propagated via `anyhow` for ergonomic error chains.
- **Ctrl+C in interactive mode:** In raw mode, `crossterm` captures Ctrl+C as a
  key event. Treated as stop — finalize WAV and transcribe (same as pressing
  `[q]`).
- **Ctrl+C in non-interactive mode:** Caught via `ctrlc` crate signal handler.
  Stop recording and save WAV, no transcription.

## Dependencies

| Crate                        | Purpose                              |
|------------------------------|--------------------------------------|
| `clap`                       | CLI argument parsing                 |
| `serde` + `toml`            | Config file parsing                  |
| `cpal`                       | Audio capture                        |
| `rubato`                     | Audio resampling (native rate → 16kHz) |
| `hound`                      | WAV file writing                     |
| `whisper-rs` (coreml)       | Transcription via whisper.cpp        |
| `crossterm`                  | Raw terminal input for key handling  |
| `indicatif`                  | Progress bars                        |
| `anyhow`                     | Error handling                       |
| `ctrlc`                      | Signal handling (non-interactive mode)|
| `tempfile`                   | Temp WAV storage                     |
| `dirs`                       | Resolve standard directories         |

## Out of Scope (v1)

- JSON output format
- Custom input device selection
- Configurable sample rate / channels
- TUI via ratatui
- Real-time / streaming transcription
