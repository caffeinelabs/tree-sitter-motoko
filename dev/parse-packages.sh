#!/usr/bin/env bash
# Parse the Motoko sources of the packages listed in test/packages.json at their default branch head.
# Fails if any file produces an ERROR or MISSING node. Clones land under target/packages/.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

manifest=test/packages.json
clones=target/packages
mkdir -p "$clones"

status=0
while IFS=$'\t' read -r name repository path; do
  # Several packages live in one monorepo, so clones are keyed by repository, not by package
  dir="$clones/$(basename "$(dirname "$repository")")-$(basename "$repository")"
  if [ ! -d "$dir/.git" ]; then
    git clone -q --depth 1 "$repository" "$dir"
  fi
  src="$dir${path:+/$path}"
  files=$(find "$src" -name '*.mo' -not -path '*/.mops/*' -not -path '*/node_modules/*' | wc -l | tr -d ' ')
  # A vanished directory would silently pass, so an empty package is a failure
  if [ "$files" = 0 ]; then
    echo "$name: no .mo files under $src"
    status=1
    continue
  fi
  out=$(find "$src" -name '*.mo' -not -path '*/.mops/*' -not -path '*/node_modules/*' -print0 \
    | xargs -0 npx tree-sitter parse -q --stat 2>&1 || true)
  failed=$(printf '%s\n' "$out" | grep -cE 'ERROR|MISSING' || true)
  printf '%-34s %5s files %3s failed\n' "$name" "$files" "$failed"
  if [ "$failed" != 0 ]; then
    printf '%s\n' "$out" | grep -E 'ERROR|MISSING' | sed "s#^$clones/##"
    status=1
  fi
done < <(python3 -c '
import json
for p in json.load(open("'"$manifest"'")):
    print(p["name"], p["repository"], p.get("path", ""), sep="\t")
')

exit $status
