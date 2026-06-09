# Fix what we can automatically
cargo +1.88 clippy --workspace --fix
CARGO_TOOLCHAIN=1.88 ./scripts/cargo-fmt-workspace.sh

# Then run tests to ensure everything is still working
CARGO_TOOLCHAIN=1.88 ./scripts/cargo-fmt-workspace.sh -- --check && cargo +1.88 check --workspace --all-features && cargo +1.88 check --manifest-path packages/wasm-bindgen/Cargo.toml --target wasm32-unknown-unknown --all-features && cargo +1.88 clippy --workspace --all-features && cargo +nightly doc --no-deps --all-features -p wry-launch -p wry-bindgen-macro -p wry-bindgen-macro-support && cargo +nightly doc --no-deps --all-features --manifest-path packages/wasm-bindgen/Cargo.toml && cargo +1.88 test --workspace
