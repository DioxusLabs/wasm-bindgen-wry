#!/usr/bin/env bash
#
# Compatibility check: build the most-downloaded crates.io dependents of
# wasm-bindgen against the wry-bindgen shim, to catch macro/runtime API drift
# from upstream wasm-bindgen.
#
# The crate list is the top 100 reverse-dependencies of wasm-bindgen by download
# count, HARDCODED below so the set is reproducible and does not drift as
# crates.io rankings change. Refresh it deliberately, not automatically. A few of
# the 100 are omitted (each documented inline) because they are FUNDAMENTALLY
# unsupportable, not because of a fixable gap — see the "Every crate is built"
# note below for the distinction.
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
# Every listed crate is built. A failure is the point of the test: it is a shim
# gap to fix (a missing/incompatible API the macro or runtime needs to grow),
# surfaced loudly. Do NOT add an exclude list to make this green, and do NOT drop
# a crate that fails on a *fixable* gap -- that hides exactly what this test exists
# to find. Fix the shim until the crate builds.
#
# The ONLY crates dropped from the list are ones the wry transport can never
# support, which are therefore out of scope rather than gaps to surface:
#   - raw pointer ABIs marshalled through a JS call (stdweb's `*const u8` closures)
#   - out-parameters: JS writing back into a Rust buffer (getrandom 0.3+'s
#     `&mut [MaybeUninit<u8>]` wasm_js backend). A request/response wire has no
#     shared memory to write back through.
#   - wasm-bindgen-test: the test-runner crate reaches into upstream
#     `wasm_bindgen::__rt` internals rather than exercising normal binding
#     surface, so it is not a useful shim compatibility target.
# These are documented at their removal sites. Everything else stays and fails
# loudly until fixed.
#
# Exit status is non-zero if any crate fails to compile.

set -u

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root" || exit 2

WB="$repo_root/packages/wasm-bindgen"
JS="$repo_root/packages/js-sys-x"
FUT="$repo_root/packages/wasm-bindgen-futures-x"
PATCH=(
  --config "patch.crates-io.wasm-bindgen.path=\"$WB\""
  --config "patch.crates-io.js-sys.path=\"$JS\""
  --config "patch.crates-io.wasm-bindgen-futures.path=\"$FUT\""
)
WASM_TARGET="wasm32-unknown-unknown"

# Optional sharding for CI. Build only the crates whose position in the combined
# (native then wasm32) list is congruent to WB_SHARD_INDEX modulo WB_SHARD_TOTAL.
# The index spans both lists, so native and wasm32 builds spread evenly across
# shards. Default is a single shard that builds everything.
WB_SHARD_TOTAL="${WB_SHARD_TOTAL:-1}"
WB_SHARD_INDEX="${WB_SHARD_INDEX:-0}"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/wb-compat.XXXXXX")"
export CARGO_TARGET_DIR="$WORK/target"
export CARGO_TERM_COLOR=never
trap 'rm -rf "$WORK"' EXIT

# ---------------------------------------------------------------------------
# Top-100 wasm-bindgen dependents by downloads.  Format:
# "crate[@version] [features]". Default features unless listed; web-sys gets a
# broad set for macro coverage.
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
  "libp2p-wasm-ext"
  # wasm-bindgen-test is intentionally not tested: it is test-runner
  # infrastructure that depends on upstream `wasm_bindgen::__rt` internals
  # rather than a normal wasm-bindgen binding consumer.
  "worker"
  "worker-sys"
  "worker-macros"
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
  "ethers-providers"
  "plotters"
  "jiff"
  "rust_decimal"
  "value-bag"
  "adler32"
  "jpeg-decoder"
  "raw-window-handle"
  # rusqlite 0.40.x currently uses unstable `std::cfg_select!` on stable Rust,
  # which fails before any wasm-bindgen shim code is exercised.
  "rusqlite@=0.39.0"
  # sqlite-wasm-rs is a wasm32-only SQLite binding. Its host build compiles the
  # wasm C shim against native C headers, which fails before the bindgen shim is
  # exercised.
  "sqlite-wasm-rs"
  "trust-dns-proto nodefault"
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
  # stdweb is intentionally not tested: it is a pre-wasm-bindgen framework whose
  # bindings pass raw `*const u8`/`*mut u8` closure pointers through the function
  # ABI, which cannot be marshalled over the wire. This is a fundamental
  # incompatibility, not a fixable shim gap, so it is out of scope rather than a
  # failure to surface.
  "cpal"
  "egui_glow"
  "coarsetime"
  "eframe"
  "softbuffer"
  "biscuit-auth wasm"
  "zxcvbn"
  "gilrs-core"
  "femme"
  "subxt-lightclient nodefault web"
  "cedar-policy-core"
  "cedar-policy"
  "cedar-policy-validator"
  "bevy_app"
  "bevy"
  "bevy_asset"
  "bevy_render"
  "bevy_winit"
  "npyz"
  # flutter_rust_bridge's wasm transfer path passes raw `*mut
  # TransferClosurePayload<_>` values through the wasm-bindgen ABI. Raw pointer
  # ABIs cannot be marshalled over the wry request/response wire.
  "webauthn-rs-proto"
  "server_fn"
  "plotly"
  "leptos_server"
  "dateparser"
  "embassy-time"
  "titlecase"
)

# ---------------------------------------------------------------------------

fail=0
passes=0
fails=()

build_one() {
  local mode="$1" crate_spec="$2"; shift 2
  local crate="$crate_spec" version="*" label="$crate_spec"
  if [[ "$crate_spec" == *@* ]]; then
    crate="${crate_spec%@*}"
    version="${crate_spec#*@}"
  fi
  # A `nodefault` token in the feature list sets default-features = false (for
  # crates whose default features pull native-only backends, e.g. tokio/mio).
  local default_features=true feats=()
  local f
  for f in "$@"; do
    if [ "$f" = nodefault ]; then default_features=false; else feats+=("$f"); fi
  done
  local dir="$WORK/probe-$crate"
  mkdir -p "$dir/src"; : > "$dir/src/lib.rs"
  {
    echo '[package]'
    echo "name = \"probe-${crate//[^a-zA-Z0-9_]/_}\""
    echo 'version = "0.0.0"'
    echo 'edition = "2021"'
    echo '[dependencies]'
    local spec="version = \"$version\""
    [ "$default_features" = false ] && spec="$spec, default-features = false"
    if [ "${#feats[@]}" -gt 0 ]; then
      printf '%s = { %s, features = [' "$crate" "$spec"
      printf '"%s",' "${feats[@]}"
      echo '] }'
    else
      echo "$crate = { $spec }"
    fi
    # wasm32 mode: force the wry backend on for the whole build graph.
    if [ "$mode" = wasm32 ]; then
      echo 'wasm-bindgen = { version = "*", features = ["unstable_force_wry_backend"] }'
      # getrandom refuses to build for wasm32 without an explicit RNG backend
      # (its own opt-in, unrelated to the shim). Select the JS backend the way
      # any real wasm32 app does, so transitive getrandom does not mask the
      # crate's own bindings. Covers getrandom 0.2 (feature) and 0.3 (cfg, set
      # via RUSTFLAGS below). Skip when the crate under test IS getrandom, to
      # avoid a duplicate dependency key.
      [ "$crate" = getrandom ] || echo 'getrandom = { version = "0.2", features = ["js"] }'
    fi
  } > "$dir/Cargo.toml"

  local target_args=() env_args=()
  if [ "$mode" = wasm32 ]; then
    target_args=(--target "$WASM_TARGET")
    env_args=(env "RUSTFLAGS=--cfg getrandom_backend=\"wasm_js\"")
  fi

  if (
    cd "$dir" || exit 2
    if [ "$mode" = wasm32 ]; then
      "${env_args[@]}" cargo build "${PATCH[@]}" "${target_args[@]}" >"$dir/log" 2>&1
    else
      cargo build "${PATCH[@]}" >"$dir/log" 2>&1
    fi
  ); then
    printf '  ok   %-7s %s\n' "$mode" "$label"
    passes=$((passes + 1))
  else
    printf '  FAIL %-7s %s\n' "$mode" "$label"
    tail -n 80 "$dir/log" | sed 's/^/         /'
    fails+=("$label ($mode)")
    fail=1
  fi
}

shard_idx=0
in_shard() {
  local r=$(( shard_idx % WB_SHARD_TOTAL ))
  shard_idx=$(( shard_idx + 1 ))
  [ "$r" -eq "$WB_SHARD_INDEX" ]
}

[ "$WB_SHARD_TOTAL" -gt 1 ] && echo "shard $WB_SHARD_INDEX of $WB_SHARD_TOTAL"

echo "== native (host) =="
for entry in "${NATIVE[@]}"; do
  in_shard || continue
  read -r -a parts <<< "$entry"
  build_one native "${parts[@]}"
done

echo "== wasm32 + unstable_force_wry_backend =="
for entry in "${WASM[@]}"; do
  in_shard || continue
  read -r -a parts <<< "$entry"
  build_one wasm32 "${parts[@]}"
done

echo
echo "built ok: $passes   failed: ${#fails[@]}"
if [ "$fail" -ne 0 ]; then
  echo "shim gaps to fix (or crate-specific blockers to investigate):"
  printf '  - %s\n' "${fails[@]}"
fi
exit "$fail"
