build:
    cargo build

test:
    cargo test

fmt:
    cargo fmt

lint:
    cargo clippy -- -D warnings

check: fmt lint test

init:
    rustup component add rustfmt clippy
    cp git-hooks/* .git/hooks/
    chmod +x .git/hooks/pre-commit .git/hooks/pre-push
    @echo "Development environment initialized."
