set minimum-version := "1.29.0"

# List the recipes.
default:
    @just --list

# Install the Rust toolchain that rust-toolchain.toml pins.
[group('check')]
setup:
    rustup toolchain install

# Format the workspace.
[group('check')]
fmt:
    cargo fmt --all

# Run clippy on the code and on the tests. The tests use the clippy.toml in .clippy-test.
[group('check')]
lint:
    cargo clippy --locked --workspace --no-default-features
    CLIPPY_CONF_DIR=.clippy-test cargo clippy --all-targets --locked --workspace --no-default-features

# Run the checks that do not need cargo-deny: format, clippy, tests, docs, release build.
[group('check')]
host: lint
    cargo fmt --all -- --check
    cargo test --locked --workspace --no-default-features
    cargo doc --no-deps --locked --workspace --no-default-features
    cargo build --release --locked --workspace --no-default-features

# Format, then run all checks (needs cargo-deny 0.20.2 on PATH).
[group('check')]
check: fmt host no-firmware
    cargo deny --locked --workspace --no-default-features check --deny warnings

# Check that the tree holds no firmware image and no file over 1 MB.
[group('check')]
no-firmware:
    python3 tools/check_no_firmware.py

# Run the workspace tests. Arguments go to cargo test.
[group('check')]
[positional-arguments]
test *args:
    cargo test --workspace "$@"

# Run the CLI. `list`, `probe`, `crc` and `flash` touch a pad.
[group('run')]
[positional-arguments]
gsfw *args:
    cargo run -q -p gsfw -- "$@"

# Open the window.
[group('run')]
gui:
    cargo run -p gsfw-gui
