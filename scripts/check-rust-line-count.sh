#!/usr/bin/env bash
set -euo pipefail

limit="${1:-300}"
failed=0

while IFS= read -r -d '' file; do
  lines=$(wc -l < "$file" | tr -d ' ')
  if (( lines > limit )); then
    printf '%s %s\n' "$lines" "$file"
    failed=1
  fi
done < <(find src -name '*.rs' -print0)

if (( failed )); then
  printf 'Rust source files must be <= %s lines.\n' "$limit" >&2
  exit 1
fi

printf 'All Rust source files are <= %s lines.\n' "$limit"
