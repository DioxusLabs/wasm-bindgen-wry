#!/usr/bin/env bash
#
# Compatibility check: build the most-downloaded crates.io dependents of
# wasm-bindgen against the wry-bindgen shim, to catch macro/runtime API drift
# from upstream wasm-bindgen.
#
# The crate list is the top 100 reverse-dependencies of wasm-bindgen by download
# count, HARDCODED below so the set is reproducible and does not drift as
# crates.io rankings change. Refresh it deliberately, not automatically.
#
# Each crate is built with the shim forced in via `--config patch.crates-io`, in
# one of two modes:
#
#   native  - the crate uses `#[wasm_bindgen]` unconditionally (js-sys, web-sys,
#             gloo-*, the frameworks, ...). Built for the host target, where the
#             shim's wry backend is the `cfg(not(wasm32))` path.
#
#   wasm32  - the crate gates wasm-bindgen behind `cfg(target_arch = "wasm32")`
#             (chrono, reqwest, getrandom, ...). On a host build its bindings are
#             stripped before the macro sees them, so it is built for
#             wasm32-unknown-unknown with `wasm-bindgen/unstable_force_wry_backend`,
#             which forces the macro to emit the wry expansion and pulls the wry
#             backend in on wasm32. This compiles their bindings against the wry
#             backend instead of upstream.
#
# Crates listed under EXCLUDE are pinned for documentation but not built, each
# with a reason (environmental build failure, or a known gap tracked elsewhere).
#
# Exit status is non-zero if any built crate fails to compile.

set -u

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root" || exit 2

WB="$repo_root/packages/wasm-bindgen"
JS="$repo_root/vendored/wasm-bindgen/crates/js-sys"
FUT="$repo_root/vendored/wasm-bindgen/crates/futures"
PATCH=(
  --config "patch.crates-io.wasm-bindgen.path=\"$WB\""
  --config "patch.crates-io.js-sys.path=\"$JS\""
  --config "patch.crates-io.wasm-bindgen-futures.path=\"$FUT\""
)
WASM_TARGET="wasm32-unknown-unknown"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/wb-compat.XXXXXX")"
export CARGO_TARGET_DIR="$WORK/target"
trap 'rm -rf "$WORK"' EXIT

# ---------------------------------------------------------------------------
# Top-100 wasm-bindgen dependents by downloads.  Format: "crate [features]".
# Default features unless listed; web-sys gets a broad set for macro coverage.
# ---------------------------------------------------------------------------

# Build natively (wasm-bindgen used unconditionally -> exercises the macro).
NATIVE=(
  "js-sys"
  "web-sys Window Document Element HtmlElement Node Text Event EventTarget \
           CssStyleDeclaration Blob Url console Performance History Location \
           Headers Request Response WebSocket MessageEvent CustomEvent"
  "wasm-bindgen-futures"
  "wasm-streams"
  "serde-wasm-bindgen"
  "tsify"
  "gloo-timers"
  "gloo-utils"
  "gloo-net"
  "gloo-events"
  "gloo-file"
  "gloo-storage"
  "gloo-console"
  "gloo-worker"
  "gloo-render"
  "gloo-dialogs"
  "gloo-history"
  "console_error_panic_hook"
  "console_log"
  "tracing-wasm"
  "wasm-logger"
  "ws_stream_wasm"
  "wasmtimer"
  "indexed_db_futures"
  "worker-kv"
  "yew"
  "leptos"
  "leptos_dom"
  "leptos_router"
)

# Build for wasm32-unknown-unknown with unstable_force_wry_backend (bindings are
# cfg(wasm32)-gated). Features here activate each crate's wasm/JS code path.
WASM=(
  "getrandom js"
  "uuid js v4"
  "chrono wasmbind clock"
  "instant wasm-bindgen"
  "web-time"
  "iana-time-zone"
  "wasm-timer"
  "sys-locale js"
  "reqwest"
  "ehttp"
  "http-client"
  "ethers-providers"
  "plotters"
  "jiff"
  "rust_decimal"
  "value-bag"
  "adler32"
  "jpeg-decoder"
  "raw-window-handle"
  "rusqlite"
  "trust-dns-proto"
  "hickory-proto"
  "winit"
  "slug"
  "v_frame"
  "rav1e"
  "glow"
  "wgpu"
  "wgpu-hal"
  "tiny-bip39"
  "opentelemetry-jaeger"
  "rfd"
  "stdweb"
  "cpal"
  "egui_glow"
  "coarsetime"
  "eframe"
  "softbuffer"
  "biscuit-auth"
  "zxcvbn"
  "c2pa"
  "gilrs-core"
  "femme"
  "subxt-lightclient"
  "cedar-policy-core"
  "cedar-policy"
  "cedar-policy-validator"
  "bevy_app"
  "bevy"
  "bevy_asset"
  "bevy_render"
  "bevy_winit"
  "npyz"
  "flutter_rust_bridge"
  "webauthn-rs-proto"
  "server_fn"
  "plotly"
  "leptos_server"
  "dateparser"
  "embassy-time"
  "titlecase"
)

# Pinned but not built. Format: "crate -> reason".
EXCLUDE=(
  "wasm-bindgen-test -> crates.io build needs LazyCell Deref (the shim's gap); the vendored copy is patched (patches 0002/0003) and validated separately"
  "libp2p-wasm-ext   -> real shim gap: JsFunction::call trait bounds unsatisfied; tracked, fix before adding"
  "worker            -> build reads worker-sys 'cloudflare:sockets' file absent off-Workers (environmental)"
  "worker-sys        -> same as worker (environmental)"
  "worker-macros     -> same as worker (environmental)"
  "sqlite-wasm-rs    -> build script fails outside a wasm/emscripten sysroot (environmental)"
  "packed_simd_2     -> requires nightly portable-simd (environmental)"
  "wasmer            -> a wasm runtime engine, not a wasm-bindgen consumer for this target (environmental)"
  "wasmer-wasi       -> same as wasmer (environmental)"
  "wasmer-wasix      -> same as wasmer (environmental)"
)

# ---------------------------------------------------------------------------

fail=0
passes=0
fails=()

build_one() {
  local mode="$1" crate="$2"; shift 2
  local feats=("$@")
  local dir="$WORK/probe-$crate"
  mkdir -p "$dir/src"; : > "$dir/src/lib.rs"
  {
    echo '[package]'
    echo "name = \"probe-${crate//[^a-zA-Z0-9_]/_}\""
    echo 'version = "0.0.0"'
    echo 'edition = "2021"'
    echo '[dependencies]'
    if [ "${#feats[@]}" -gt 0 ]; then
      printf '%s = { version = "*", features = [' "$crate"
      printf '"%s",' "${feats[@]}"
      echo '] }'
    else
      echo "$crate = \"*\""
    fi
    # wasm32 mode: force the wry backend on for the whole build graph.
    if [ "$mode" = wasm32 ]; then
      echo 'wasm-bindgen = { version = "*", features = ["unstable_force_wry_backend"] }'
    fi
  } > "$dir/Cargo.toml"

  local target_args=()
  [ "$mode" = wasm32 ] && target_args=(--target "$WASM_TARGET")

  if (cd "$dir" && cargo build "${PATCH[@]}" "${target_args[@]}" >"$dir/log" 2>&1); then
    printf '  ok   %-7s %s\n' "$mode" "$crate"
    passes=$((passes + 1))
  else
    printf '  FAIL %-7s %s\n' "$mode" "$crate"
    grep -m3 -E '^error' "$dir/log" | sed 's/^/         /'
    fails+=("$crate ($mode)")
    fail=1
  fi
}

echo "== native (host) =="
for entry in "${NATIVE[@]}"; do
  read -r -a parts <<< "$entry"
  build_one native "${parts[@]}"
done

echo "== wasm32 + unstable_force_wry_backend =="
for entry in "${WASM[@]}"; do
  read -r -a parts <<< "$entry"
  build_one wasm32 "${parts[@]}"
done

echo "== excluded (pinned, not built) =="
for e in "${EXCLUDE[@]}"; do echo "  skip $e"; done

echo
echo "built ok: $passes   failed: ${#fails[@]}   excluded: ${#EXCLUDE[@]}"
if [ "$fail" -ne 0 ]; then
  printf 'FAILED: %s\n' "${fails[@]}"
fi
exit "$fail"
