set minimum-version := "1.29.0"

# List the recipes.
default:
    @just --list

# Install the pinned Rust toolchain.
[group('check')]
setup:
    python3 tools/check.py setup-rust

# Format the workspace.
[group('check')]
fmt:
    cargo fmt --all

# Run the fast lane during edits (not acceptance).
[group('check')]
quick:
    python3 tools/check.py quick

# Run the full gate (needs cargo-deny on PATH).
[group('check')]
check: fmt
    python3 tools/check.py

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
