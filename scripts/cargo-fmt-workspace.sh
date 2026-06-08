#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root" || exit 2

rustfmt_cmd=(rustfmt)
if [ -n "${CARGO_TOOLCHAIN:-}" ]; then
  rustfmt_cmd=(rustfmt "+${CARGO_TOOLCHAIN}")
fi

if [ "${1:-}" = "--" ]; then
  shift
fi

files=()
while IFS= read -r file; do
  files+=("$file")
done < <(
  git ls-files -- \
    'packages/**/*.rs' \
    'examples/**/*.rs' \
    ':(exclude)packages/wry-launch/tests/upstream_tests/main.rs'
)

if [ "${#files[@]}" -eq 0 ]; then
  exit 0
fi

"${rustfmt_cmd[@]}" --edition 2024 --style-edition 2024 "$@" "${files[@]}"
