# Quick: fmt + clippy
check:
    cargo fmt
    cargo clippy --all-targets

# Full: fmt + clippy + tests
full:
    cargo fmt
    cargo clippy --all-targets
    cargo test --all-targets

# Build release binary
build:
    cargo build --release

# Auto-fix clippy suggestions
fix:
    cargo clippy --fix --allow-dirty --lib --tests
    cargo fmt

# Build Zed extension WASM
zed-build:
    cd zed && cargo build --target wasm32-wasip1 --release
