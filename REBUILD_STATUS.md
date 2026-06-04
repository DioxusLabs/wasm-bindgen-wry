# Rebuild status: pull-out-runtime onto origin/main (#27/#28)

Worktree: `/Users/evanalmloff/Desktop/wasm-bindgen-wry-rebuild`
Branch: `pull-out-runtime-rebuild` (parent = origin/main `f483b7b`, so #27/#28 are in ancestry)
Original branch preserved at `pull-out-runtime`; pre-merge backup at `pre-merge-backup`.

## The core finding
`pull-out-runtime` is a crate-split of the **pre-#27/#28** runtime:
- proxy-bridge event loop: `WryBindgenEvent`, `handle_user_event`, `app_builder`
- NOT the driver model: no `split()`, `WryBindgenWebviewDriver`, `with_evaluate_script`

origin/main (#27/#28) replaced that with the **driver / take-over-event-loop** model.
The identical TypeScript is a red herring — the IPC *wire* protocol didn't change, only
the Rust event-loop integration did.

## DONE (Phase 1 — architecture-independent refactor reproduced on origin/main)
- `wry-bindgen-abi` (8 files) and `wry-bindgen-core` (8 files) crates brought from branch
- Runtime-crate support modules: `function_registry.rs`, `js_helpers.rs`, `type_cache.rs`,
  `id_allocator.rs` (byte-identical on both sides), `ipc.rs` shim
- macro-support changes, `wry-bindgen` shim crate, scripts/shims — ~68 files via
  `git checkout pull-out-runtime -- …`
- Old single-crate paths removed (`wry-bindgen/src/{wry,runtime,batch,id_allocator}.rs`,
  `ts/*` old paths, etc.)
- Verified per-file strategies:
  - `batch.rs`: branch version subsumes origin's #28 batch changes (both removed `top_level`)
    — BUT its test helper couples to `WryIPC::new()` whose signature is arch-specific.
  - `ipc.rs` (runtime): branch 156-line shim; impl lives in `wry-bindgen-abi`.

## REMAINING (Phase 2 — the driver event-loop port; needs design intent)
Co-designed triad `batch.rs ↔ runtime.rs (WryIPC) ↔ wry.rs (WryBindgen)`:
- `wry-bindgen-runtime/src/runtime.rs` — 3-way merge left with conflict markers (17 hunks).
  origin's `WryIPC::new() -> (ipc, senders, driver_commands)` (driver) vs branch (proxy).
- `wry-bindgen-runtime/src/wry.rs` — 3-way merge, conflict markers (39 hunks).
  origin's `WryBindgen::split()` + `WryBindgenWebviewDriver` vs branch `app_builder`/`handle_user_event`.
- `wry-bindgen-runtime/src/batch.rs` — currently 3-way merged (markers); likely resolves
  to branch version once `WryIPC` coupling is settled.
- `wry-bindgen-runtime/src/lib.rs` — currently branch (old-arch exports); must export the
  new driver API instead of `WryBindgenEvent`/`AppBuilder`/`WryBindgenResponder`.
- `wry-launch/`: `webview.rs`, `benches/*`, `tests/main_thread_tests/{main,timer_callbacks}.rs`,
  `Cargo.toml`, `src/lib.rs` — currently origin's new-arch versions; need import path
  `wasm_bindgen::wry` → `wry_bindgen_runtime` and to match the runtime crate's final API.
- Workspace `Cargo.toml`/`Cargo.lock`, `wry-bindgen/Cargo.toml`, `examples/leptos/Cargo.toml`
  — reconcile members/deps; regenerate lock.
- JS: `wry-bindgen-runtime/src/js/{hash.txt,main.js}` regenerate via build.rs (TS already in place).
- wasm-bindgen `js-sys/src/futures/mod.rs` (+9 from #27) vs branch's js-sys edits — reconcile.

## Two execution strategies for the core port
- **A — preserve your refactor:** translate origin's driver event-loop into your
  `EncodedParts`/crate-split idioms inside `runtime.rs`/`wry.rs`. Keeps your design;
  higher translation effort; needs your intent on WryIPC↔Runtime mapping.
- **B — re-derive core from origin:** take origin's `batch/runtime/wry/ipc` (trusted driver
  code) + apply only crate-path wiring; drop the `EncodedParts`/API-min on those files and
  re-polish later. Lower concurrency risk; loses some of your refactor.
