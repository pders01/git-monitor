# List available recipes
default:
    @just --list

# Build the project
build:
    cargo build

# Run the application
run:
    cargo run

# Run tests
test:
    cargo test

# Run clippy lints
clippy:
    cargo clippy -- -D warnings

# Format code
fmt:
    cargo fmt

# Check formatting
fmt-check:
    cargo fmt -- --check

# Run cargo check
check:
    cargo check

# Clean build artifacts
clean:
    cargo clean

# Run full CI combo: format check, clippy, then tests
ci: fmt-check clippy test
