#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root" || exit 2

cargo_cmd=(${CARGO:-cargo})
target=wasm32-unknown-unknown

wasm_bindgen_version="$(
  sed -n 's/^version = "\(.*\)"/\1/p' vendored/wasm-bindgen/Cargo.toml | head -n 1
)"
if [ -z "$wasm_bindgen_version" ]; then
  echo "could not read vendored wasm-bindgen version" >&2
  exit 2
fi

web_sys_version="$(
  sed -n 's/^version = "\(.*\)"/\1/p' vendored/wasm-bindgen/crates/web-sys/Cargo.toml | head -n 1
)"
if [ -z "$web_sys_version" ]; then
  echo "could not read vendored web-sys version" >&2
  exit 2
fi

wasm_bindgen_bin="${WASM_BINDGEN:-}"
if [ -n "$wasm_bindgen_bin" ] && [ ! -x "$wasm_bindgen_bin" ]; then
  echo "WASM_BINDGEN is set but is not executable: $wasm_bindgen_bin" >&2
  exit 2
fi

bin_version() {
  "$1" --version 2>/dev/null | sed -n 's/^wasm-bindgen //p'
}

if [ -z "$wasm_bindgen_bin" ] && command -v wasm-bindgen >/dev/null 2>&1; then
  candidate="$(command -v wasm-bindgen)"
  if [ "$(bin_version "$candidate")" = "$wasm_bindgen_version" ]; then
    wasm_bindgen_bin="$candidate"
  fi
fi

if [ -z "$wasm_bindgen_bin" ]; then
  install_root="$repo_root/target/wasm-bindgen-cli-$wasm_bindgen_version"
  candidate="$install_root/bin/wasm-bindgen"
  if [ -x "$candidate" ] && [ "$(bin_version "$candidate")" = "$wasm_bindgen_version" ]; then
    wasm_bindgen_bin="$candidate"
  else
    echo "installing wasm-bindgen-cli $wasm_bindgen_version for CLI metadata check"
    "${cargo_cmd[@]}" install wasm-bindgen-cli \
      --version "$wasm_bindgen_version" \
      --locked \
      --root "$install_root"
    wasm_bindgen_bin="$candidate"
  fi
fi

actual_version="$(bin_version "$wasm_bindgen_bin")"
if [ "$actual_version" != "$wasm_bindgen_version" ]; then
  echo "wasm-bindgen CLI version mismatch: expected $wasm_bindgen_version, got ${actual_version:-unknown}" >&2
  exit 2
fi

tmp="$(mktemp -d "${TMPDIR:-/tmp}/wry-bindgen-cli-check.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/dep-normal/src" "$tmp/app/src"

cat > "$tmp/dep-normal/Cargo.toml" <<TOML
[package]
name = "dep-normal"
version = "0.1.0"
edition = "2021"

[dependencies]
web-sys = { version = "=$web_sys_version", features = ["Document", "Window"] }
TOML

cat > "$tmp/dep-normal/src/lib.rs" <<'RS'
pub fn normal_document_title() -> String {
    web_sys::window()
        .and_then(|window| window.document())
        .map(|document| document.title())
        .unwrap_or_default()
}
RS

cat > "$tmp/app/Cargo.toml" <<TOML
[package]
name = "sys-shim-cli-fixture"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
dep-normal = { path = "../dep-normal" }
wasm-bindgen = "=$wasm_bindgen_version"
web-sys = { version = "=$web_sys_version", features = ["Document", "Window"] }

[patch.crates-io]
wasm-bindgen = { path = "$repo_root/packages/wasm-bindgen" }
web-sys = { path = "$repo_root/packages/web-sys-x" }
TOML

cat > "$tmp/app/src/lib.rs" <<'RS'
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn both_document_titles() -> String {
    let title = web_sys::window()
        .and_then(|window| window.document())
        .map(|document| document.title())
        .unwrap_or_default();
    format!("{title}:{}", dep_normal::normal_document_title())
}
RS

tree="$(
  "${cargo_cmd[@]}" tree \
    --manifest-path "$tmp/app/Cargo.toml" \
    --target "$target"
)"

if ! grep -F "$repo_root/packages/web-sys-x" <<<"$tree" >/dev/null; then
  echo "fixture did not resolve web-sys through packages/web-sys-x" >&2
  echo "$tree" >&2
  exit 1
fi

if grep -F "web-sys-wry" <<<"$tree" >/dev/null; then
  echo "fixture pulled web-sys-wry into the wasm32 graph" >&2
  echo "$tree" >&2
  exit 1
fi

"${cargo_cmd[@]}" build \
  --manifest-path "$tmp/app/Cargo.toml" \
  --target "$target" \
  --release

"$wasm_bindgen_bin" \
  "$tmp/app/target/$target/release/sys_shim_cli_fixture.wasm" \
  --target web \
  --out-dir "$tmp/out"

echo "wasm-bindgen accepted the linked sys shim fixture"
