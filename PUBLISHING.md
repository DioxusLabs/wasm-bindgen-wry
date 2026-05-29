# Publishing

Use `scripts/publish-wasm-bindgen-x.rs` to prepare and publish the crates.io packages for this fork.

The script does not modify the source tree. It copies the publish inputs to `target/publish-wasm-bindgen-x`, rewrites package metadata in that staging tree, and runs `cargo publish` from the staged manifests.

## Package Names

The wasm-bindgen-facing packages are published with `-x` package names. Their Rust crate names and versions stay unchanged.

| Source package | Published package |
| --- | --- |
| `wasm-bindgen` | `wasm-bindgen-x` |
| `wasm-bindgen-macro` | `wasm-bindgen-macro-x` |
| `js-sys` | `js-sys-x` |
| `web-sys` | `web-sys-x` |
| `wasm-bindgen-futures` | `wasm-bindgen-futures-x` |

The local `wry-bindgen`, `wry-bindgen-macro`, and `wry-bindgen-macro-support` packages keep their package names.

## Dry Run

Run the script with no arguments to prepare staging and run a publish dry run:

```sh
./scripts/publish-wasm-bindgen-x.rs
```

This is equivalent to:

```sh
./scripts/publish-wasm-bindgen-x.rs --dry-run
```

For a full dry run, the script runs `cargo publish --workspace --dry-run` from the staged workspace. This lets Cargo verify unpublished workspace dependencies together.

## Publish

After the dry run passes, publish for real with:

```sh
./scripts/publish-wasm-bindgen-x.rs --publish
```

Real publish runs crate-by-crate in dependency order:

1. `wry-bindgen-macro-support`
2. `wry-bindgen-macro`
3. `wry-bindgen`
4. `wasm-bindgen-macro-x`
5. `wasm-bindgen-x`
6. `js-sys-x`
7. `web-sys-x`
8. `wasm-bindgen-futures-x`

## Useful Options

Prepare the staging tree without running Cargo:

```sh
./scripts/publish-wasm-bindgen-x.rs --prepare-only
```

Publish or dry-run one package:

```sh
./scripts/publish-wasm-bindgen-x.rs --dry-run -p js-sys
./scripts/publish-wasm-bindgen-x.rs --publish -p js-sys-x
```

Pass `--no-verify` through to Cargo:

```sh
./scripts/publish-wasm-bindgen-x.rs --publish --no-verify
```
