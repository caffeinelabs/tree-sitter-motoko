# mo-fmt

A formatter for [Motoko](https://github.com/caffeinelabs/motoko), built on the grammar in this repository.

It normalises spacing and indentation and keeps your line breaks: a list you wrote on one line stays on one line, and one you broke goes one item per line. Every run re-parses its own output and refuses to write code that parses differently from the input.

## Usage

```sh
mo-fmt .                                     # format every .mo file under the current directory, in place
mo-fmt src/main.mo lib/                      # some files and directories
mo-fmt --check .                             # list what would change, and exit 1 if anything would
mo-fmt --stdin-filepath src/main.mo < src/main.mo   # format stdin, for editors
mo-fmt --syntax moc2 .                       # override mo-fmt.toml; see Configuration
mo-fmt --rule brace-bodies=false .           # override one rule; repeatable
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
brace-bodies = true   # any of the rules below, overriding `syntax`
```

Flags override the file key by key, so a tool can pass the options without writing one:

```sh
mo-fmt --syntax moc2 --indent-width 4 --rule unparen-patterns=false .
```

`syntax` is a preset over the rules below: `preserve` sets every rule to `false` or `preserve`, and `moc2` rewrites legacy syntax towards the [target syntax](https://github.com/caffeinelabs/motoko/issues/6352). `moc2` may change between minor versions while moc 2.0 is in beta.

### Rules

| Rule | Values | In `moc2` | Status |
|---|---|---|---|
| [`brace-bodies`](#brace-bodies) | `true`, `false` | `true` | done |
| [`unparen-heads`](#unparen-heads) | `true`, `false` | `true` | done |
| [`unparen-patterns`](#unparen-patterns) | `true`, `false` | `true` | done |
| [`do-blocks`](#do-blocks) | `true`, `false` | `true` | done |
| [`semicolons`](#semicolons) | `preserve`, `minimal` | `minimal` | done |
| [`trailing-commas`](#trailing-commas) | `preserve`, `multiline`, `never` | `preserve` | done |
| [`block-blank-lines`](#block-blank-lines) | `preserve`, `trim` | `preserve` | planned |
| [`imports`](#imports) | `preserve`, `organize` | `preserve` | planned |
| [`func-bodies`](#func-bodies) | `preserve`, `block` | `preserve` | deferred |

`moc2` leaves at `preserve` the rules that pick a style the target syntax doesn't imply. A planned rule isn't a key yet, so setting one is an error rather than silently doing nothing.

A rule set in `mo-fmt.toml` or by `--rule <name>=<value>` overrides the preset either way, so `moc2` can turn one rule off and `preserve` can turn one on. `--rule` spells values as the file does, without quotes.

Each rule is safe on its own. The rules run in a fixed order, `brace-bodies` first, and the ones that need a braced body skip any construct that isn't braced yet.

#### `brace-bodies`

Every control body becomes a braced block: `if`/`else` branches, `while`/`for`/`loop` bodies, `case` and `catch` arms, and `try`, `finally`, `async` and `async*` bodies. `else if` chains are kept.

```motoko
if (n == 0) 1 else n * fact(n - 1)
if (n == 0) { 1 } else { n * fact(n - 1) }
```

#### `unparen-heads`

Drops the parentheses around an `if`, `while` or `switch` head, around `for (p in e)`, and around the condition of `loop { … } while (c)`. A spaced call in a head, `f x`, becomes `f(x)` when that is all that keeps the parentheses. Records, tuples, statement-like and multi-line heads keep them.

```motoko
switch (map.get(key)) { … }
switch map.get(key) { … }
```

#### `unparen-patterns`

Drops the parentheses around a `case` or `catch` pattern, moving a variant's payload into `#tag(…)`, and likewise for each side of an `or`, `and` or `: T` pattern: `case (#a x or #b x)` → `case #a(x) or #b(x)`. Tuple patterns keep them.

```motoko
case (#ok v) { … }
case #ok(v) { … }
```

#### `do-blocks`

`let … else`, `label`, `debug`, `await`, `ignore`, `assert`, `throw` and the condition of `loop … while` take an expression, not a body. moc reads a bare `{ … }` after them as a block today, but the target syntax reads it as a record, so the block is spelled `do { … }`, which means the same in both.

```motoko
let ?user = users.get(id) else { return #err("unknown") };
let ?user = users.get(id) else do { return #err("unknown") };
```

#### `semicolons`

`minimal` drops every `;` moc doesn't need: the one after a braced `case` arm, since the next `case` already ends it, and the one after the last item of a block, body, record, object or variant type, or file. A `;` between two items stays.

```motoko
switch x { case #a { a() }; case #b { b(); }; }
switch x { case #a { a() } case #b { b() } }
```

#### `trailing-commas`

A `,` after the last item of a tuple, argument list, array, pattern or type list. `multiline` puts one on a list broken one item per line and none on a list on one line, and `never` drops them all. `(x,)` means the same as `(x)` in moc, so dropping one never changes the code. `multiline` leaves two kinds of list without one: a single item in parentheses, since `(x,)` reads as a one-tuple, and a `<…>` list, whose `>` moc needs glued to the last item.

```motoko
Map.add(
  map,
  key,
  value
)
```

```motoko
Map.add(
  map,
  key,
  value,
)
```

#### `block-blank-lines`

`trim` drops blank lines just inside the braces of a block or body.

```motoko
func f() {

  a();

}
```

```motoko
func f() {
  a();
}
```

#### `imports`

`organize` groups imports by prefix (`ic:`, `canister:`, `mo:`, then relative paths) with a blank line between groups, and sorts each group by path. Comments among the imports stay with the import they precede.

```motoko
import Text "mo:core/Text";
import Utils "./utils";
import Array "mo:core/Array";
```

```motoko
import Array "mo:core/Array";
import Text "mo:core/Text";

import Utils "./utils";
```

#### `func-bodies`

`preserve` (the default, also in `moc2`) or `block`, which rewrites `func f(x) : T = e` → `func f(x) : T { e }` and `func x = x + 1` → `func(x) { x + 1 }`. Deferred until moc can forward a computed `async` without `= e`.

### From prettier-plugin-motoko

| prettier option | mo-fmt |
|---|---|
| `tabWidth` | `indent-width` |
| `useTabs` | none: spaces only |
| `printWidth` | none: line breaks are kept as written |
| `bracketSpacing` | none: always `{ x = 1 }` |
| `semi` | `semicolons`, which can drop them but not add them |
| `trailingComma` | `trailing-commas` |
| `motokoRemoveLinesAroundCodeBlocks` | `block-blank-lines = "trim"` |
| `motokoOrganizeImports` | `imports = "organize"` |

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
- Formatting files in parallel, and a wasm build for editors.
- Node kinds are strings (`"if_exp"`), so a typo disables a rule silently until a test notices. `build.rs` could generate constants from `node-types.json`.
