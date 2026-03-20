# Notetaker — Development Guidelines

## Development Process

- **TDD is mandatory.** Write a failing test first, then implement the minimum code to pass it, then refactor. Never write implementation code without a corresponding test.
- Do not write unnecessary comments. Write clear code with clear intentions. Only add a comment when the logic genuinely requires explanation.

## Code Quality

- Use `rustfmt` for formatting and `clippy` for linting.
- Run `just fmt` before committing. Run `just lint` and `just test` before pushing.

## Task Runner

Use `just` (justfile) for all common tasks:
- `just build` — compile the project
- `just test` — run all tests
- `just fmt` — format code with rustfmt
- `just lint` — run clippy
- `just check` — run fmt check + clippy + tests

## Git Hooks

- **pre-commit:** Format code with `rustfmt` (auto-fix).
- **pre-push:** Run clippy and tests. Block push on failure.
