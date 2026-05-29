#!/usr/bin/env bash

set -u
set -o pipefail

usage() {
  cat <<'USAGE'
Usage:
  update-wasm-bindgen.sh [VERSION_OR_REF] [options]

Options:
  --current VERSION       Current vendored version for reporting.
  --upstream-url URL      Upstream git URL. Defaults to official wasm-bindgen.
  --report PATH           Write the markdown upgrade report to PATH.
  --log-dir PATH          Write step logs under PATH.
  --failure-file PATH     Write the manual-repair report to PATH on patch/update failure.
  --status-file PATH      Write key=value status for CI wrappers.
  --install-api-tool      Install cargo-semver-checks before the API check.
  --skip-api-check        Do not run check-wasm-bindgen-api.rs.
  --skip-metadata         Do not run cargo metadata after updating.
  -h, --help              Print this help.

If VERSION_OR_REF is omitted, the script queries upstream releases and updates to
the latest stable release when one exists.
USAGE
}

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
cd "$repo_root" || exit 2

target_ref=""
current=""
upstream_url="https://github.com/wasm-bindgen/wasm-bindgen.git"
report=""
log_dir=""
failure_file="patches/wasm-bindgen/UPGRADE_FAILURE.md"
status_file=""
install_api_tool=false
run_api_check=true
run_metadata=true

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    --current)
      current="${2:?--current requires a value}"
      shift 2
      ;;
    --current=*)
      current="${1#--current=}"
      shift
      ;;
    --upstream-url)
      upstream_url="${2:?--upstream-url requires a value}"
      shift 2
      ;;
    --upstream-url=*)
      upstream_url="${1#--upstream-url=}"
      shift
      ;;
    --report)
      report="${2:?--report requires a value}"
      shift 2
      ;;
    --report=*)
      report="${1#--report=}"
      shift
      ;;
    --log-dir)
      log_dir="${2:?--log-dir requires a value}"
      shift 2
      ;;
    --log-dir=*)
      log_dir="${1#--log-dir=}"
      shift
      ;;
    --failure-file)
      failure_file="${2:?--failure-file requires a value}"
      shift 2
      ;;
    --failure-file=*)
      failure_file="${1#--failure-file=}"
      shift
      ;;
    --status-file)
      status_file="${2:?--status-file requires a value}"
      shift 2
      ;;
    --status-file=*)
      status_file="${1#--status-file=}"
      shift
      ;;
    --install-api-tool)
      install_api_tool=true
      shift
      ;;
    --skip-api-check)
      run_api_check=false
      shift
      ;;
    --skip-metadata)
      run_metadata=false
      shift
      ;;
    --*)
      echo "error: unknown option $1" >&2
      usage >&2
      exit 2
      ;;
    *)
      if [[ -n "$target_ref" ]]; then
        echo "error: unexpected argument $1" >&2
        usage >&2
        exit 2
      fi
      target_ref="$1"
      shift
      ;;
  esac
done

read_manifest_package_version() {
  local manifest="$1"
  awk '
    $0 == "[package]" { in_package = 1; next }
    /^\[/ { in_package = 0 }
    in_package && $1 == "version" {
      gsub(/"/, "", $3)
      print $3
      exit
    }
  ' "$manifest"
}

read_package_version_at() {
  local repo="$1"
  local commit="$2"
  local manifest="$3"
  git -C "$repo" show "$commit:$manifest" 2>/dev/null | awk '
    $0 == "[package]" { in_package = 1; next }
    /^\[/ { in_package = 0 }
    in_package && $1 == "version" {
      gsub(/"/, "", $3)
      print $3
      exit
    }
  '
}

replace_literal_in_patch() {
  local patch_file="$1"
  local from="$2"
  local to="$3"
  if [[ -n "$from" && -n "$to" && "$from" != "$to" ]]; then
    FROM="$from" TO="$to" perl -0pi -e 's/\Q$ENV{FROM}\E/$ENV{TO}/g' "$patch_file"
  fi
}

normalize_patch_versions() {
  local source_commit="$1"
  local target_commit="$2"
  shift 2
  local patch_files=("$@")
  local manifest from to

  local manifests=(
    Cargo.toml
    crates/futures/Cargo.toml
    crates/js-sys/Cargo.toml
    crates/shared/Cargo.toml
    crates/web-sys/Cargo.toml
  )

  for manifest in "${manifests[@]}"; do
    from="$(read_package_version_at "$upstream_dir" "$source_commit" "$manifest")"
    to="$(read_package_version_at "$upstream_dir" "$target_commit" "$manifest")"
    if [[ -z "$from" || -z "$to" ]]; then
      continue
    fi
    for patch_file in "${patch_files[@]}"; do
      replace_literal_in_patch "$patch_file" "$from" "$to"
    done
  done
}

if [[ -z "$target_ref" ]]; then
  release_output="$(./scripts/check-wasm-bindgen-releases.rs --check --list 2>&1)"
  release_status=$?
  printf '%s\n' "$release_output"

  current="$(printf '%s\n' "$release_output" | sed -n 's/^current wasm-bindgen version:[[:space:]]*//p')"
  target_ref="$(printf '%s\n' "$release_output" | sed -n 's/^latest upstream release:[[:space:]]*//p')"

  if [[ "$release_status" -eq 0 ]]; then
    echo "wasm-bindgen is already up to date."
    if [[ -n "$status_file" ]]; then
      {
        echo "failed=false"
        echo "api_check_failed=false"
        echo "failure_step="
        echo "pr_title=wasm-bindgen already up to date at $current"
        echo "report="
      } >"$status_file"
    fi
    exit 0
  fi

  if [[ "$release_status" -ne 1 || -z "$current" || -z "$target_ref" ]]; then
    echo "error: failed to determine current/latest wasm-bindgen versions" >&2
    exit 2
  fi
fi

if [[ -z "$current" ]]; then
  current="$(read_manifest_package_version wasm-bindgen/Cargo.toml)"
fi

tmp_root="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
work_dir="$(mktemp -d "$tmp_root/wasm-bindgen-upgrade.XXXXXX")"
upstream_dir="$work_dir/upstream-wasm-bindgen"
normalized_patch_dir="$work_dir/normalized-wasm-bindgen-patches"

if [[ -z "$report" ]]; then
  report="$work_dir/wasm-bindgen-upgrade-report.md"
fi
if [[ -z "$log_dir" ]]; then
  log_dir="$work_dir/logs"
fi

mkdir -p "$log_dir" patches/wasm-bindgen
mkdir -p "$(dirname "$report")" "$(dirname "$failure_file")"
if [[ -n "$status_file" ]]; then
  mkdir -p "$(dirname "$status_file")"
fi
rm -f "$failure_file"

failed=false
api_check_failed=false
failure_step=""
pr_title="Update wasm-bindgen to $target_ref"

append_log() {
  local title="$1"
  local file="$2"
  {
    echo
    echo "### $title"
    echo
    echo '```text'
    cat "$file"
    echo '```'
  } >>"$report"
}

fail_step() {
  local title="$1"
  local file="$2"
  failed=true
  failure_step="$title"
  append_log "$title" "$file"
}

write_status() {
  if [[ -z "$status_file" ]]; then
    return
  fi

  {
    echo "failed=$failed"
    echo "api_check_failed=$api_check_failed"
    echo "failure_step=$failure_step"
    echo "pr_title=$pr_title"
    echo "report=$report"
  } >"$status_file"
}

{
  echo "# wasm-bindgen upgrade to $target_ref"
  echo
  echo "- Current vendored version: $current"
  echo "- Target upstream release/ref: $target_ref"
  if [[ -n "${GITHUB_SERVER_URL:-}" && -n "${GITHUB_REPOSITORY:-}" && -n "${GITHUB_RUN_ID:-}" ]]; then
    echo "- Workflow run: $GITHUB_SERVER_URL/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID"
  fi
  echo
  echo "## Result"
  echo
  echo "The script applies patches/wasm-bindgen onto the target upstream release/ref, replaces the tracked wasm-bindgen directory with the patched result, regenerates patches against the new upstream base, and bumps local crate versions."
} >"$report"

clone_log="$log_dir/clone-upstream.log"
if git clone --no-checkout "$upstream_url" "$upstream_dir" >"$clone_log" 2>&1; then
  git -C "$upstream_dir" config user.name "github-actions[bot]"
  git -C "$upstream_dir" config user.email "41898282+github-actions[bot]@users.noreply.github.com"
  append_log "Cloned upstream wasm-bindgen" "$clone_log"
else
  fail_step "Failed to clone upstream wasm-bindgen" "$clone_log"
fi

if [[ "$failed" == "false" ]]; then
  checkout_log="$log_dir/checkout-target.log"
  if git -C "$upstream_dir" checkout "$target_ref" >"$checkout_log" 2>&1; then
    append_log "Checked out upstream $target_ref" "$checkout_log"
  else
    fail_step "Failed to check out upstream $target_ref" "$checkout_log"
  fi
fi

shopt -s nullglob
patches=(patches/wasm-bindgen/*.patch)
shopt -u nullglob
normalized_patches=()
for patch in "${patches[@]}"; do
  normalized_patch="$normalized_patch_dir/$(basename "$patch")"
  mkdir -p "$(dirname "$normalized_patch")"
  cp "$patch" "$normalized_patch"
  normalized_patches+=("$normalized_patch")
done

if [[ "$failed" == "false" && "${#patches[@]}" -gt 0 ]]; then
  normalize_log="$log_dir/normalize-patches.log"
  base_commit=""
  if [[ -f patches/wasm-bindgen/BASE ]]; then
    base_commit="$(tr -d '[:space:]' <patches/wasm-bindgen/BASE)"
  fi
  if [[ -z "$base_commit" ]]; then
    base_commit="$current"
  fi

  if latest_commit="$(git -C "$upstream_dir" rev-parse "$target_ref^{commit}" 2>"$normalize_log")"; then
    if normalize_patch_versions "$base_commit" "$latest_commit" "${normalized_patches[@]}" >>"$normalize_log" 2>&1; then
      {
        echo "Normalized ${#normalized_patches[@]} patch file(s) from base $base_commit to $latest_commit."
        echo "Committed patch files were not modified before patch application."
      } >>"$normalize_log"
      append_log "Normalized wasm-bindgen patch versions" "$normalize_log"
    else
      fail_step "Failed to normalize wasm-bindgen patch versions" "$normalize_log"
    fi
  else
    fail_step "Failed to resolve upstream $target_ref commit" "$normalize_log"
  fi
fi

if [[ "$failed" == "false" && "${#patches[@]}" -gt 0 ]]; then
  patch_log="$log_dir/apply-patches.log"
  if git -C "$upstream_dir" am -3 "${normalized_patches[@]}" >"$patch_log" 2>&1; then
    append_log "Applied wasm-bindgen patch stack" "$patch_log"
  else
    {
      cat "$patch_log"
      echo
      echo "Upstream worktree status:"
      git -C "$upstream_dir" status --short || true
    } >"$log_dir/apply-patches-with-status.log"
    fail_step "Failed to apply wasm-bindgen patch stack" "$log_dir/apply-patches-with-status.log"
  fi
elif [[ "$failed" == "false" ]]; then
  echo "No patch files found; vendoring clean upstream $target_ref." >"$log_dir/apply-patches.log"
  append_log "No wasm-bindgen patches to apply" "$log_dir/apply-patches.log"
fi

if [[ "$failed" == "false" ]]; then
  replace_log="$log_dir/replace-vendored-tree.log"
  if rsync -a --delete --exclude .git "$upstream_dir/" wasm-bindgen/ >"$replace_log" 2>&1; then
    append_log "Replaced vendored wasm-bindgen tree" "$replace_log"
  else
    fail_step "Failed to replace vendored wasm-bindgen tree" "$replace_log"
  fi
fi

if [[ "$failed" == "false" ]]; then
  refresh_log="$log_dir/refresh-patches.log"
  rm -f patches/wasm-bindgen/*.patch patches/wasm-bindgen/BASE
  if git -C "$upstream_dir" format-patch --output-directory "$repo_root/patches/wasm-bindgen" "$target_ref..HEAD" >"$refresh_log" 2>&1; then
    git -C "$upstream_dir" rev-parse "$target_ref^{commit}" >patches/wasm-bindgen/BASE
    append_log "Regenerated wasm-bindgen patches" "$refresh_log"
  else
    fail_step "Failed to regenerate wasm-bindgen patches" "$refresh_log"
  fi
fi

if [[ "$failed" == "false" ]]; then
  version_log="$log_dir/bump-local-versions.log"
  if ./scripts/bump-wasm-bindgen-version.rs >"$version_log" 2>&1; then
    append_log "Bumped local crate versions" "$version_log"
  else
    fail_step "Failed to bump local crate versions" "$version_log"
  fi
fi

if [[ "$failed" == "false" && "$run_metadata" == "true" ]]; then
  metadata_log="$log_dir/cargo-metadata.log"
  metadata_output="$log_dir/cargo-metadata.json"
  if cargo metadata --no-deps --format-version 1 >"$metadata_output" 2>"$metadata_log"; then
    metadata_bytes="$(wc -c <"$metadata_output" | tr -d ' ')"
    echo "cargo metadata --no-deps --format-version 1 completed successfully ($metadata_bytes output bytes)." >"$metadata_log"
    append_log "Verified cargo metadata" "$metadata_log"
  else
    fail_step "Cargo metadata failed after upgrade" "$metadata_log"
  fi
fi

if [[ "$failed" == "false" && "$run_api_check" == "true" ]]; then
  if [[ "$install_api_tool" == "true" ]]; then
    install_api_tool_log="$log_dir/install-api-check-tool.log"
    if cargo binstall cargo-semver-checks -y --force --version 0.44.0 >"$install_api_tool_log" 2>&1; then
      append_log "Installed cargo-semver-checks" "$install_api_tool_log"
    else
      api_check_failed=true
      append_log "Failed to install cargo-semver-checks" "$install_api_tool_log"
    fi
  fi

  if [[ "$api_check_failed" == "false" ]]; then
    api_check_log="$log_dir/api-compatibility.log"
    if ./scripts/check-wasm-bindgen-api.rs "$target_ref" >"$api_check_log" 2>&1; then
      append_log "API compatibility check passed" "$api_check_log"
    else
      api_check_status=$?
      api_check_failed=true
      {
        echo "check-wasm-bindgen-api.rs failed with status $api_check_status"
        echo
        cat "$api_check_log"
      } >"$log_dir/api-compatibility-with-status.log"
      append_log "API compatibility check failed" "$log_dir/api-compatibility-with-status.log"
    fi
  fi
fi

if [[ "$failed" == "true" ]]; then
  {
    echo
    echo "## Manual work required"
    echo
    echo "Failed step: $failure_step"
    echo
    echo "The tracked wasm-bindgen directory may contain a partial update if the failure occurred after replacement. See this report for logs."
  } >>"$report"
  cp "$report" "$failure_file"
  pr_title="Attempt wasm-bindgen update to $target_ref (manual fix needed)"
else
  {
    echo
    echo "## Upgrade completed"
    echo
    echo "The vendored wasm-bindgen tree, patch stack, and local bindgen crate versions were updated successfully."
  } >>"$report"
  pr_title="Update wasm-bindgen to $target_ref"
fi

write_status

echo "upgrade report: $report"
echo "upgrade logs: $log_dir"

if [[ "$failed" == "true" ]]; then
  exit 1
fi
if [[ "$api_check_failed" == "true" ]]; then
  exit 3
fi
exit 0
