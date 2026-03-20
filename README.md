# Notetaker

## Prerequisites

- [Rust](https://rustup.rs/) — install via `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- [just](https://github.com/casey/just) — install via `brew install just` (macOS) or see [installation docs](https://github.com/casey/just#installation)

## Getting Started

```sh
git clone <repo-url>
cd notetaker
just init
```

`just init` installs required Rust components (rustfmt, clippy) and sets up git hooks.

## Development

| Command      | Description                          |
|--------------|--------------------------------------|
| `just build` | Compile the project                  |
| `just test`  | Run all tests                        |
| `just fmt`   | Format code with rustfmt             |
| `just lint`  | Run clippy                           |
| `just check` | Run fmt + clippy + tests             |
