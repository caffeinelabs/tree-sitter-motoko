#!/usr/bin/env bash
# Formats every tracked .mo file in ~/motoko and ~/motoko-core, in place.
#
# Usage: tools/format-repos.sh [v2 | legacy] [--reset]
#   (default)  this checkout's mo-fmt, syntax = "preserve"
#   v2         this checkout's mo-fmt, syntax = "moc2"
#   legacy     the released prettier-plugin-motoko 0.13.0 (installed once into ~/.cache)
#   --reset    first restore the .mo files from HEAD, discarding uncommitted .mo changes
# Undo afterwards with: git -C <repo> checkout -- '*.mo'
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repos=("$HOME/motoko" "$HOME/motoko-core")

mode=preserve
reset=false
for arg in "$@"; do
    case "$arg" in
        v2) mode=moc2 ;;
        legacy) mode=legacy ;;
        --reset) reset=true ;;
        *) echo "unknown argument: $arg (expected v2, legacy or --reset)" >&2; exit 2 ;;
    esac
done

if [ "$mode" = legacy ]; then
    legacy_dir="$HOME/.cache/motoko-format-legacy"
    if [ ! -f "$legacy_dir/node_modules/prettier-plugin-motoko/package.json" ]; then
        echo "installing prettier-plugin-motoko 0.13.0 into $legacy_dir"
        mkdir -p "$legacy_dir"
        npm install --prefix "$legacy_dir" --ignore-scripts --no-save prettier@3.5.3 prettier-plugin-motoko@0.13.0 >/dev/null
    fi
    prettier="$legacy_dir/node_modules/.bin/prettier"
    plugin="$legacy_dir/node_modules/prettier-plugin-motoko/lib/environments/node.js"
else
    echo "building mo-fmt in $here"
    (cd "$here" && cargo build -q --release --locked)
    # mo-fmt reads mo-fmt.toml from the current directory, so it runs from a scratch one, on absolute paths.
    config_dir="$(mktemp -d)"
    trap 'rm -rf "$config_dir"' EXIT
    printf 'syntax = "%s"\n' "$mode" >"$config_dir/mo-fmt.toml"
fi

for repo in "${repos[@]}"; do
    if [ "$reset" = true ]; then
        git -C "$repo" checkout -- '*.mo'
    elif [ -n "$(git -C "$repo" status --porcelain -- '*.mo')" ]; then
        echo "$repo has uncommitted .mo changes; rerun with --reset to discard them" >&2
        exit 1
    fi

    echo "formatting $repo ($mode)"
    errors="$(mktemp)"
    if [ "$mode" = legacy ]; then
        # Prettier resolves each repo's own .prettierrc per file; it exits non-zero on files it can't parse.
        (cd "$repo" && git ls-files -z '*.mo' |
            xargs -0 "$prettier" --plugin="$plugin" --write --log-level warn >/dev/null 2>"$errors") || true
        failed="$(grep -o '^\[error\] [^ ]*\.mo: [A-Za-z]*: .*' "$errors" | sed 's/^\[error\] /    /' || true)"
    else
        (cd "$config_dir" && git -C "$repo" ls-files -z '*.mo' | perl -0pe "s|^|$repo/|" |
            xargs -0 "$here/target/release/mo-fmt" >/dev/null 2>"$errors") || true
        failed="$(grep -E '^/.*\.mo:' "$errors" | sed "s|^$repo/|    |" || true)"
    fi
    stat="$(git -C "$repo" diff --shortstat -- '*.mo' | sed 's/^ //')"
    echo "  ${stat:-no changes}"
    if [ -n "$failed" ]; then
        echo "  $(printf '%s\n' "$failed" | wc -l | tr -d ' ') files left unchanged because they don't format:"
        printf '%s\n' "$failed" | cut -c1-110
    fi
    rm -f "$errors"
done
