#!/usr/bin/env bash
# Compares mo-fmt with the TypeScript prettier-plugin-motoko it was ported from, on a repo's tracked .mo files.
# Usage: tools/compare.sh <repo> <preserve|moc2> [plugin checkout, default ../../prettier-plugin-motoko]
# Leaves both outputs and a diff under target/compare/<repo>-<mode>/.
set -euo pipefail

here="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$1" && pwd)" mode="$2"
plugin="$(cd "${3:-$here/../../prettier-plugin-motoko}" && pwd)"
out="$here/target/compare/$(basename "$repo")-$mode"

(cd "$here" && cargo build -q --release)
(cd "$plugin" && npm run -s build >/dev/null)

rm -rf "$out" && mkdir -p "$out/ts" "$out/rs"
git -C "$repo" archive HEAD -- ':(glob)**/*.mo' | tar -x -C "$out/ts"
cp -R "$out/ts/." "$out/rs/"

(cd "$out/ts" && find . -name '*.mo' -print0 | xargs -0 node "$here/tools/ts-format.mjs" "$plugin" "$mode")
(cd "$out/rs" && printf 'syntax = "%s"\n' "$mode" > mo-fmt.toml &&
    find . -name '*.mo' -print0 | xargs -0 "$here/target/release/mo-fmt" >/dev/null 2>"$out/rs-errors.txt" || true)
rm "$out/rs/mo-fmt.toml"
echo "rs: $(grep -c . "$out/rs-errors.txt" || true) error lines (see rs-errors.txt)"

diff -ru "$out/ts" "$out/rs" >"$out/diff.txt" || true
echo "$(grep -c '^diff ' "$out/diff.txt" || true) files differ (see $out/diff.txt)"
