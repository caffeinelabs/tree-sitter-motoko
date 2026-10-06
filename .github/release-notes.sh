#!/usr/bin/env bash
# Usage: release-notes.sh <tag>
# Prints the notes for <tag>: the PRs since the previous tag of its stream that touched that stream's files.
# `mo-fmt-v*` tags cover mo-fmt/ and `v*` tags the rest, since GitHub's generated notes can't tell the two apart.
# Needs the tags fetched and GH_TOKEN set.
set -euo pipefail

tag=$1
repo=${GITHUB_REPOSITORY:-caffeinelabs/tree-sitter-motoko}

if [[ $tag == mo-fmt-v* ]]; then
  match='mo-fmt-v*' paths=(mo-fmt)
else
  match='v*' paths=(. ':!mo-fmt')
fi

prev=$(git describe --tags --abbrev=0 --match "$match" "$tag^" 2>/dev/null || true)
if [[ -n $prev ]]; then
  range=$prev..$tag
elif [[ $tag == mo-fmt-v* ]]; then
  range=$tag
else
  # The grammar's history before its first tag goes back years and predates its PRs.
  echo "First release."
  exit
fi

feat=() fix=() deps=() other=() seen=" "
while read -r sha subject; do
  # The PR's title, which a squash merge may not have copied with its number.
  pr=$(gh api "repos/$repo/commits/$sha/pulls" -q '.[0] | select(.) | "\(.number)\t\(.title)"')
  if [[ -n $pr ]]; then
    title=${pr#*$'\t'} ref="#${pr%%$'\t'*}"
  else
    title=$subject ref=$sha
  fi
  # A PR merged as several commits is listed once.
  [[ $seen == *" $ref "* ]] && continue
  seen+="$ref "
  text=$(sed -E 's/^[a-z]+(\([^)]*\))?!?: //; s/ \(#[0-9]+\)( \(#[0-9]+\))*$//' <<<"$title")
  line="- $text ($ref)"
  case $title in
    feat[:\(]*) feat+=("$line") ;;
    fix[:\(]*) fix+=("$line") ;;
    *'(deps):'*) deps+=("$line") ;;
    ci[:\(]* | docs[:\(]* | 'chore: release'* | 'chore('*'): release'*) ;;
    *) other+=("$line") ;;
  esac
done < <(git log --first-parent --format='%H %s' "$range" -- "${paths[@]}")

section() {
  local heading=$1
  shift
  (($#)) || return 0
  printf '### %s\n\n' "$heading"
  printf '%s\n' "$@"
  echo
}
section Features ${feat[@]+"${feat[@]}"}
section Fixes ${fix[@]+"${fix[@]}"}
section Dependencies ${deps[@]+"${deps[@]}"}
section Other ${other[@]+"${other[@]}"}
if [[ -n $prev ]]; then
  echo "**Full changelog**: https://github.com/$repo/compare/$prev...$tag"
fi
