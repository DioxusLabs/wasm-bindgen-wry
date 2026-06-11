#!/usr/bin/env bash
# Checks that downstream graphs consuming the wasm-bindgen shim build, link,
# and survive wasm-bindgen-cli post-processing. Two fixtures:
#
# 1. Linked sys shims: a fully shimmed graph — [patch.crates-io] routes
#    wasm-bindgen/web-sys through the shims, including for a dependency that
#    binds web-sys normally. wasm-bindgen-cli must accept the wasm32 module.
#
# 2. Coexistence: a mixed graph — the crate depends on the shim while other
#    dependencies pull genuine upstream wasm-bindgen from crates.io, the way
#    dioxus does via gloo-timers/gloo-net/wasm-streams. Both backends must
#    coexist through:
#      a. native build + link — GNU-flavor linkers (Linux CI) reject
#         duplicate ABI symbols between wry-bindgen and upstream; ld64 on
#         macOS silently dead-strips them, so the authoritative run is on
#         Linux. Set WB_COEXIST_NATIVE_TARGET=aarch64-unknown-linux-musl (or
#         x86_64-...) to cross-check the GNU-flavor link from a mac.
#      b. wasm32 build + link, where the shim delegates to upstream
#      c. wasm-bindgen-cli over the wasm32 cdylib, which must accept glue
#         from the shim macro and upstream's macro in one module
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root" || exit 2

cargo_cmd=(${CARGO:-cargo})
wasm_target=wasm32-unknown-unknown

host_triple="$(rustc -vV | sed -n 's/^host: //p')"
native_target="${WB_COEXIST_NATIVE_TARGET:-$host_triple}"

# Convenience for cross-checking from a mac: musl targets link self-contained
# with rust-lld, no external toolchain needed.
case "$native_target" in
  *-linux-musl*)
    rustflags_var="CARGO_TARGET_$(echo "$native_target" | tr '[:lower:]-' '[:upper:]_')_RUSTFLAGS"
    if [ -z "${!rustflags_var:-}" ]; then
      export "$rustflags_var=-C linker=rust-lld -C link-self-contained=yes"
    fi
    ;;
esac

wasm_bindgen_version="$(
  sed -n 's/^version = "\(.*\)"/\1/p' vendored/wasm-bindgen/Cargo.toml | head -n 1
)"
js_sys_version="$(
  sed -n 's/^version = "\(.*\)"/\1/p' vendored/wasm-bindgen/crates/js-sys/Cargo.toml | head -n 1
)"
web_sys_version="$(
  sed -n 's/^version = "\(.*\)"/\1/p' vendored/wasm-bindgen/crates/web-sys/Cargo.toml | head -n 1
)"
if [ -z "$wasm_bindgen_version" ] || [ -z "$js_sys_version" ] || [ -z "$web_sys_version" ]; then
  echo "could not read vendored wasm-bindgen/js-sys/web-sys versions" >&2
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
    echo "installing wasm-bindgen-cli $wasm_bindgen_version"
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

tmp="$(mktemp -d "${TMPDIR:-/tmp}/wry-bindgen-downstream.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

### Fixture 1: linked sys shims ##############################################

echo "== linked sys shims: fully shimmed graph =="

mkdir -p "$tmp/dep-normal/src" "$tmp/sys-shims/src"

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

cat > "$tmp/sys-shims/Cargo.toml" <<TOML
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

cat > "$tmp/sys-shims/src/lib.rs" <<'RS'
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
    --manifest-path "$tmp/sys-shims/Cargo.toml" \
    --target "$wasm_target"
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
  --manifest-path "$tmp/sys-shims/Cargo.toml" \
  --target "$wasm_target" \
  --release

"$wasm_bindgen_bin" \
  "$tmp/sys-shims/target/$wasm_target/release/sys_shim_cli_fixture.wasm" \
  --target web \
  --out-dir "$tmp/sys-shims-out"

echo "wasm-bindgen accepted the linked sys shim fixture"

### Fixture 2: coexistence with upstream #####################################

echo "== coexistence: shim alongside upstream wasm-bindgen =="

mkdir -p "$tmp/coexist/src"

cat > "$tmp/coexist/Cargo.toml" <<TOML
[package]
name = "coexist-fixture"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
# The shim, consumed the way dioxus consumes wasm-bindgen-x.
wasm-bindgen = { path = "$repo_root/packages/wasm-bindgen" }
# A direct handle on genuine upstream, renamed the way downstream crates can.
wasm-bindgen-upstream = { package = "wasm-bindgen", version = "=$wasm_bindgen_version" }
# An ordinary ecosystem crate that binds upstream on its own.
gloo-timers = "0.3"
# Pins upstream (and the wasm-bindgen-cli schema) to the vendored version.
js-sys = "=$js_sys_version"

# The shim pulls upstream from git to dodge the workspace patch cycle;
# redirect that to the registry so it unifies with the ecosystem copies.
[patch."https://github.com/wasm-bindgen/wasm-bindgen"]
wasm-bindgen = "=$wasm_bindgen_version"

# One codegen unit per backend so any reference into the crate pulls the
# object that defines its #[no_mangle] ABI symbols. With default CGU
# partitioning the linker only loads those objects when neighboring code
# happens to be referenced, which larger apps hit organically — this makes
# the native link deterministic instead of lucky.
[profile.dev.package.wasm-bindgen]
codegen-units = 1

[profile.dev.package.wry-bindgen]
codegen-units = 1
TOML

cat > "$tmp/coexist/src/lib.rs" <<'RS'
//! Mixes glue from the shim's `#[wasm_bindgen]` macro with glue from
//! upstream's macro (inside gloo-timers and its js-sys internals) — the
//! blend a dioxus web build hands to wasm-bindgen-cli.

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn schedule_noop() -> u32 {
    let _ = gloo_timers::callback::Timeout::new(0, || {}).forget();
    wasm_bindgen::externref_heap_live_count()
}
RS

cat > "$tmp/coexist/src/main.rs" <<'RS'
//! Exercises both backends from one native binary. The closures use the
//! ABI that owns the closure drop hooks; the non-generic
//! `externref_heap_live_count` calls force each backend's object file into
//! the link.

fn main() {
    let shim: wasm_bindgen::prelude::Closure<dyn Fn()> =
        wasm_bindgen::prelude::Closure::new(|| {});
    let upstream: wasm_bindgen_upstream::prelude::Closure<dyn Fn()> =
        wasm_bindgen_upstream::prelude::Closure::new(|| {});
    drop((shim, upstream));

    println!(
        "live externrefs: shim={} upstream={}",
        wasm_bindgen::externref_heap_live_count(),
        wasm_bindgen_upstream::externref_heap_live_count(),
    );
}
RS

tree="$(
  "${cargo_cmd[@]}" tree \
    --manifest-path "$tmp/coexist/Cargo.toml" \
    --target "$native_target"
)"

if ! grep -F "$repo_root/packages/wry-bindgen" <<<"$tree" >/dev/null; then
  echo "fixture did not resolve the shim to wry-bindgen on the native target" >&2
  echo "$tree" >&2
  exit 1
fi

# Registry upstream prints without a path suffix; the shim path dep prints
# with one. Both must be present for the check to mean anything.
if ! grep -E "wasm-bindgen v$wasm_bindgen_version\$" <<<"$tree" >/dev/null; then
  echo "fixture did not pull registry upstream wasm-bindgen into the native graph" >&2
  echo "$tree" >&2
  exit 1
fi

case "$native_target" in
  *-darwin*)
    echo "note: ld64 dead-strips the duplicated ABI symbols instead of" \
      "rejecting them; the native leg only gates regressions on Linux/GNU links"
    ;;
esac

echo "building native ($native_target)"
"${cargo_cmd[@]}" build \
  --manifest-path "$tmp/coexist/Cargo.toml" \
  --target "$native_target"

echo "building wasm32"
"${cargo_cmd[@]}" build \
  --manifest-path "$tmp/coexist/Cargo.toml" \
  --target "$wasm_target"

echo "running wasm-bindgen-cli"
"$wasm_bindgen_bin" \
  "$tmp/coexist/target/$wasm_target/debug/coexist_fixture.wasm" \
  --target web \
  --out-dir "$tmp/coexist-out"

echo "shim coexists with upstream wasm-bindgen (native + wasm32 + cli)"
