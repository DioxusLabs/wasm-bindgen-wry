# wasm-bindgen upgrade to 0.2.123

- Current vendored version: 0.2.122
- Target upstream release/ref: 0.2.123
- Workflow run: https://github.com/DioxusLabs/wasm-bindgen-wry/actions/runs/27270613288

## Result

The script applies vendored/patches/wasm-bindgen onto the target upstream release/ref, replaces the tracked vendored/wasm-bindgen directory with the patched result, regenerates patches against the new upstream base, and bumps local crate versions.

### Cloned upstream wasm-bindgen

```text
Cloning into '/home/runner/work/_temp/wasm-bindgen-upgrade.TFTTI9/upstream-wasm-bindgen'...
```

### Checked out upstream 0.2.123

```text
Note: switching to '0.2.123'.

You are in 'detached HEAD' state. You can look around, make experimental
changes and commit them, and you can discard any commits you make in this
state without impacting any branches by switching back to a branch.

If you want to create a new branch to retain commits you create, you may
do so (now or later) by using -c with the switch command. Example:

  git switch -c <new-branch-name>

Or undo this operation with:

  git switch -

Turn off this advice by setting config variable advice.detachedHead to false

HEAD is now at 861696ae5 Release 0.2.123 (#5188)
```

### Normalized wasm-bindgen patch versions

```text
Normalized 7 patch file(s) from base ddd322514d87a4b21342b7ab9a9d70796fc60576 to 861696ae5ad7679c6b3eed7a448e34d65ced12e4.
Committed patch files were not modified before patch application.
```

### Failed to apply wasm-bindgen patch stack

```text
Applying: Prepare root workspace for wry patching
Applying: Point sys crates at the wry shim
error: patch failed: crates/futures/Cargo.toml:17
error: crates/futures/Cargo.toml: patch does not apply
error: patch failed: crates/js-sys/Cargo.toml:37
error: crates/js-sys/Cargo.toml: patch does not apply
error: patch failed: crates/shared/Cargo.toml:14
error: crates/shared/Cargo.toml: patch does not apply
error: patch failed: crates/test/Cargo.toml:16
error: crates/test/Cargo.toml: patch does not apply
error: patch failed: crates/web-sys/Cargo.toml:23
error: crates/web-sys/Cargo.toml: patch does not apply
error: Did you hand edit your patch?
It does not apply to blobs recorded in its index.
hint: Use 'git am --show-current-patch=diff' to see the failed patch
hint: When you have resolved this problem, run "git am --continue".
hint: If you prefer to skip this patch, run "git am --skip" instead.
hint: To restore the original branch and stop patching, run "git am --abort".
hint: Disable this message with "git config set advice.mergeConflict false"
Using index info to reconstruct a base tree...
M	crates/futures/Cargo.toml
M	crates/js-sys/Cargo.toml
M	crates/shared/Cargo.toml
M	crates/test/Cargo.toml
M	crates/web-sys/Cargo.toml
Patch failed at 0002 Point sys crates at the wry shim

Upstream worktree status:
```

## Manual work required

Failed step: Failed to apply wasm-bindgen patch stack

The tracked wasm-bindgen directory may contain a partial update if the failure occurred after replacement. See this report for logs.
