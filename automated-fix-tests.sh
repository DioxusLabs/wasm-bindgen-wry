# Fix what we can automatically
cargo +1.88 clippy --workspace --fix
cargo +1.88 fmt --all

# Then run tests to ensure everything is still working
cargo +1.88 fmt --all -- --check && cargo +1.88 check --workspace --all-features && cargo +1.88 clippy --workspace --all-features && cargo +nightly doc --no-deps --all-features -p wry-testing -p wasm-bindgen -p wry-bindgen-macro -p wry-bindgen-macro-support && cargo +1.88 test --workspace