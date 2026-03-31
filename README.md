# Voxscribe

Record and transcribe audio locally using [whisper.cpp](https://github.com/ggerganov/whisper.cpp). No cloud services required.

## Installation

### Prerequisites

- [Rust](https://rustup.rs/) — install via `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- [just](https://github.com/casey/just) — install via `brew install just` (macOS) or see [installation docs](https://github.com/casey/just#installation)

### Install

```sh
git clone <repo-url>
cd voxscribe
just install
```

Then download the whisper model:

```sh
voxscribe download-model
```

## Usage

```sh
voxscribe record                # interactive recording + transcription
voxscribe record --language sv  # pin language (auto-detects by default)
voxscribe record --single-speaker  # optimize for a single speaker
voxscribe transcribe audio.wav  # transcribe an existing file
```

Transcriptions are saved to `~/transcriptions/` by default.

## Development

### Setup

```sh
just init
```

`just init` installs required Rust components (rustfmt, clippy) and sets up git hooks.

### Commands

| Command              | Description                            |
|----------------------|----------------------------------------|
| `just install`       | Install to `~/.cargo/bin/`             |
| `just build`         | Compile the project (debug)            |
| `just build-release` | Compile optimized release build        |
| `just run <args>`    | Build and run (e.g. `just run record`) |
| `just test`          | Run all tests                          |
| `just fmt`           | Format code with rustfmt               |
| `just lint`          | Run clippy                             |
| `just check`         | Run fmt + clippy + tests               |
