# wasm-bindgen upgrade to 0.2.129

- Current vendored version: 0.2.123
- Target upstream release/ref: 0.2.129
- Workflow run: https://github.com/DioxusLabs/wasm-bindgen-wry/actions/runs/36118685186

## Result

The script applies vendored/patches/wasm-bindgen onto the target upstream release/ref, replaces the tracked vendored/wasm-bindgen directory with the patched result, regenerates patches against the new upstream base, and bumps local crate versions.

### Cloned upstream wasm-bindgen

```text
Cloning into '/home/runner/work/_temp/wasm-bindgen-upgrade.7DUNwv/upstream-wasm-bindgen'...
```

### Checked out upstream 0.2.129

```text
Note: switching to '0.2.129'.

You are in 'detached HEAD' state. You can look around, make experimental
changes and commit them, and you can discard any commits you make in this
state without impacting any branches by switching back to a branch.

If you want to create a new branch to retain commits you create, you may
do so (now or later) by using -c with the switch command. Example:

  git switch -c <new-branch-name>

Or undo this operation with:

  git switch -

Turn off this advice by setting config variable advice.detachedHead to false

HEAD is now at 165586f85 Release 0.2.129
```

### Normalized wasm-bindgen patch versions

```text
Normalized 9 patch file(s) from base 861696ae5ad7679c6b3eed7a448e34d65ced12e4 to 165586f854c852bcbc4d14d8a75a0a0e0d0abc12.
Committed patch files were not modified before patch application.
```

### Failed to apply wasm-bindgen patch stack

```text
Applying: Prepare root workspace for wry patching
error: patch failed: Cargo.toml:46
error: Cargo.toml: patch does not apply
error: patch failed: crates/macro/Cargo.toml:8
error: crates/macro/Cargo.toml: patch does not apply
error: Did you hand edit your patch?
It does not apply to blobs recorded in its index.
hint: Use 'git am --show-current-patch=diff' to see the failed patch
hint: When you have resolved this problem, run "git am --continue".
hint: If you prefer to skip this patch, run "git am --skip" instead.
hint: To restore the original branch and stop patching, run "git am --abort".
hint: Disable this message with "git config set advice.mergeConflict false"
Using index info to reconstruct a base tree...
M	Cargo.toml
M	crates/macro/Cargo.toml
Patch failed at 0001 Prepare root workspace for wry patching

Upstream worktree status:
```

## Manual work required

Failed step: Failed to apply wasm-bindgen patch stack

The tracked wasm-bindgen directory may contain a partial update if the failure occurred after replacement. See this report for logs.
