# mo-fmt

A formatter for [Motoko](https://github.com/caffeinelabs/motoko), built on the grammar in this repository.

It normalises spacing and indentation and keeps your line breaks: a list you wrote on one line stays on one line, and one you broke goes one item per line. Every run re-parses its own output and refuses to write code that parses differently from the input.

## Usage

```sh
mo-fmt .                                     # format every .mo file under the current directory, in place
mo-fmt src/main.mo lib/                      # some files and directories
mo-fmt --check .                             # list what would change, and exit 1 if anything would
mo-fmt --stdin-filepath src/main.mo < src/main.mo   # format stdin, for editors
```

- Directories are searched for `.mo` files, honouring the project's `.gitignore` files (not your global one, so results are the same on every machine) and skipping `node_modules`, dot-directories such as `.mops`, and symlinks. A file named on the command line is always formatted, and a symlink named there is followed.
- A syntax error is reported with its line, column and the offending line; the other files are still formatted.
- A file is replaced by renaming a temporary file over it, so readers never see it half-written. Several mo-fmt runs on the same files at once are safe. An edit made by something else while a file is being formatted is kept and the file reported, on a best-effort basis: an edit landing in the instant between that check and the rename can still be lost. The rename gives the file a new inode, so hard links to it and its owner aren't carried over; its permissions are.
- Output always uses LF line endings, and a byte-order mark is kept.
- Exit codes: `0` done, `1` `--check` found files that need formatting, `2` a usage error or a file that failed to format.
- A `// mo-fmt-ignore` comment leaves the next item as written. `// prettier-ignore` still works too.

## Configuration

An optional `mo-fmt.toml` in the current directory:

```toml
syntax = "preserve"   # or "moc2", which rewrites legacy syntax to the moc 2.0 forms
indent-width = 2      # 1 to 16
```

`moc2` braces every control body, drops the parentheses around control heads and case patterns where moc 2.0 allows it, and drops the `;` after a braced `case` arm. It may change between minor versions while moc 2.0 is in beta.

## Development

From this directory:

```sh
cargo test                     # unit, case, fixture and CLI tests
MOTOKO_CORPUS_REQUIRED=1 cargo test --test corpus   # needs ../../motoko and ../../motoko-core
cargo insta review             # after a deliberate change to a fixture's output
cargo deny check               # advisories, licenses and sources, as CI runs it
```

- `rust-toolchain.toml` pins the toolchain, and CI also builds on the `rust-version` in `Cargo.toml`. Dependabot bumps the toolchain, the crates and the workflow actions.
- `tests/cases.toml` holds input/output cases, each also checked to be a fixed point.
- `tests/format/` holds larger fixtures with snapshots in `tests/snapshots/`.
- The corpus test formats the compiler's tests and motoko-core in both modes. It requires stable output and no internal errors, and requires motoko-core, which is already formatted, to come out unchanged in `preserve`. CI pins both revisions.
- `tools/format-repos.sh [v2 | legacy] [--reset]` formats `~/motoko` and `~/motoko-core` in place: with this checkout's mo-fmt in `preserve` or `v2` (`moc2`), or with the released prettier-plugin-motoko 0.13.0 for `legacy`. It refuses to run over uncommitted `.mo` changes unless given `--reset`.
- `tools/compare.sh <repo> <preserve|moc2>` compares the output with the TypeScript [prettier-plugin-motoko](https://github.com/caffeinelabs/prettier-plugin-motoko) this formatter was ported from, checked out as `../../prettier-plugin-motoko`.

Releases are cut by pushing a `mo-fmt-vX.Y.Z` tag matching `Cargo.toml`. `.github/workflows/mo-fmt-release.yml` then attaches to a GitHub release a `mo-fmt-<target>.tar.xz` for each of `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl` (static, for any Linux), `aarch64-apple-darwin` and `x86_64-apple-darwin`. Each has a build provenance attestation, checked with `gh attestation verify <archive> --repo caffeinelabs/tree-sitter-motoko`. The same builds run, without releasing, on every pull request that touches `mo-fmt/`.

## Not yet

- Config discovery: only `mo-fmt.toml` in the current directory is read. Next: the closest config above each file, then a `[format]` section in `mops.toml`, and `exclude` globs.
- Packed lists: a list broken anywhere goes one item per line, which explodes packed rows such as numeric tables.
- Spacing inside a line is only partly normalised: `x:T`, `<K,V>` and `->` are kept as written.
- `moc2` rewrites `f x` to `f(x)` only in control heads, where moc 2.0 requires it.
- moc 2.0.0-beta.2 accepts `or`, `and` and `: T` case patterns without parentheses; `moc2` still keeps them.
- Formatting files in parallel, and a wasm build for editors.
- Node kinds are strings (`"if_exp"`), so a typo disables a rule silently until a test notices. `build.rs` could generate constants from `node-types.json`.
