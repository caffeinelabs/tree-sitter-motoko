//! The syntax rules, as edits to the source text located by the tree.

use crate::Error;
use crate::config::{BlockBlankLines, Imports, Rules, Semicolons, TrailingCommas};
use crate::print::is_ignore_directive;
use crate::print::parts::{Family, list_items, list_of};
use crate::tree::{Branch, Node, parse};

struct Edit {
    start: usize,
    end: usize,
    text: String,
}

type Rule = fn(&Branch<'_>) -> Vec<Edit>;

/// Each rule's passes, by its `mo-fmt.toml` key, in the order they run: imports and commas first, since `(x,)` → `(x)`
/// can free a head or pattern of its parentheses, then the syntax rules, braces first since the others need braced
/// bodies, and last the passes that only remove what the others may leave.
fn passes(rules: &Rules) -> Vec<(&'static str, Rule)> {
    let mut out: Vec<(&'static str, Rule)> = Vec::new();
    if rules.imports == Imports::Organize {
        out.push(("imports", organize_imports));
    }
    match rules.trailing_commas {
        TrailingCommas::Preserve => {}
        TrailingCommas::Multiline => out.push(("trailing-commas", multiline_commas)),
        TrailingCommas::Never => out.push(("trailing-commas", no_trailing_commas)),
    }
    if rules.brace_bodies {
        out.push(("brace-bodies", brace_bodies));
    }
    if rules.do_blocks {
        out.push(("do-blocks", do_blocks));
    }
    if rules.unparen_heads {
        out.push(("unparen-heads", glue_head_calls));
        out.push(("unparen-heads", unparen_heads));
    }
    if rules.semicolons == Semicolons::Minimal {
        out.push(("semicolons", drop_case_semis));
    }
    if rules.unparen_patterns {
        out.push(("unparen-patterns", unparen_patterns));
    }
    if rules.block_blank_lines == BlockBlankLines::Trim {
        out.push(("block-blank-lines", trim_block_blank_lines));
    }
    if rules.semicolons == Semicolons::Minimal {
        out.push(("semicolons", drop_trailing_semis));
    }
    out
}

// Every pass removes what it matches, so a real file settles in a few rounds.
const MAX_ROUNDS: usize = 100;

/// Applies the rules. Each pass runs to a fixed point before the next, since later ones need braced bodies.
/// `Error::Syntax` if the input doesn't parse, and `Error::Internal` if a rule breaks it or never settles.
pub fn rewrite(source: &str, rules: &Rules) -> Result<String, Error> {
    let mut text = source.to_string();
    let mut comments = None;
    for (name, pass) in passes(rules) {
        for round in 0.. {
            if round == MAX_ROUNDS {
                return Err(Error::Internal(format!("the `{name}` rule did not settle")));
            }
            let root = parse(&text).map_err(|e| {
                if text == source {
                    Error::Syntax(e)
                } else {
                    Error::Internal(format!(
                        "the `{name}` rule produced code that does not parse ({e})"
                    ))
                }
            })?;
            let Node::Branch(root) = &root else {
                unreachable!("the root is a branch")
            };
            // No rule touches a comment, and the printer's guard only sees the rewritten code, so check here.
            let now = comments_of(root);
            if comments.get_or_insert_with(|| now.clone()) != &now {
                return Err(Error::Internal(format!(
                    "the `{name}` rule lost or changed a comment"
                )));
            }
            let ignored = ignored(root);
            let edits: Vec<Edit> = pass(root)
                .into_iter()
                .filter(|e| {
                    !ignored
                        .iter()
                        .any(|&(start, end)| e.start <= end && e.end >= start)
                })
                .collect();
            if edits.is_empty() {
                break;
            }
            let next = apply(&text, edits);
            if next == text {
                break;
            }
            text = next;
        }
    }
    Ok(text)
}

/// The spans of the items a `// mo-fmt-ignore` keeps as written, as the printer finds them, which no edit may touch.
fn ignored(root: &Branch<'_>) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for b in root.branches() {
        if list_of(b).is_none() {
            continue;
        }
        let items = list_items(b, b.kind != "source_file");
        for pair in items.windows(2) {
            if is_ignore_directive(pair[0].node) {
                let item = &pair[1];
                let end = item.rest.last().map_or(item.node.end(), |(_, n)| n.end());
                out.push((item.node.start(), end));
            }
        }
    }
    out
}

/// The comments' text, sorted, since `imports` may reorder them.
fn comments_of(root: &Branch<'_>) -> Vec<String> {
    fn walk(b: &Branch<'_>, out: &mut Vec<String>) {
        for c in &b.children {
            match c {
                _ if crate::print::parts::is_comment(c) => out.push(c.text().to_string()),
                Node::Branch(b) => walk(b, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort();
    out
}

/// Applies the edits whose ranges don't overlap an earlier one; the rest wait for the next round.
fn apply(source: &str, mut edits: Vec<Edit>) -> String {
    edits.sort_by_key(|e| (e.start, e.end));
    let mut chosen: Vec<Edit> = Vec::new();
    for e in edits {
        if chosen.last().is_some_and(|last| e.start < last.end) {
            continue;
        }
        chosen.push(e);
    }
    let mut out = source.to_string();
    for e in chosen.iter().rev() {
        out.replace_range(e.start..e.end, &e.text);
    }
    out
}

fn index_of(parent: &Branch<'_>, child: &Branch<'_>) -> usize {
    parent
        .children
        .iter()
        .position(|c| c.as_branch().is_some_and(|b| std::ptr::eq(b, child)))
        .expect("a child of its parent")
}

/// The spaces to put around the text replacing `children[first..=last]`, where it has no gap beside it.
fn padding(parent: &Branch<'_>, first: usize, last: usize) -> (&'static str, &'static str) {
    let kids = &parent.children;
    let before = if first > 0 && kids[first - 1].is_text() {
        ""
    } else {
        " "
    };
    let after = if kids.get(last + 1).is_some_and(Node::is_text) {
        ""
    } else {
        " "
    };
    (before, after)
}

fn is_block(b: Option<&Branch<'_>>) -> bool {
    b.is_some_and(|b| b.kind == "block_exp")
}

/// The expression right after `keyword`, for the constructs whose operand has no field.
fn after<'n, 'a>(n: &'n Branch<'a>, keyword: &str) -> Option<&'n Branch<'a>> {
    n.nodes()
        .skip_while(|c| !c.is_token(keyword))
        .skip(1)
        .find_map(|c| c.as_branch().filter(|b| !b.extra))
}

/// Braces `b`, a child of `parent`. A body on a line of its own gets its `{` at the end of the line before, as in
/// `if c {⏎ x⏎ }`, unless a line comment ends that line.
fn brace(parent: &Branch<'_>, b: &Branch<'_>) -> [Edit; 2] {
    let kids = &parent.children;
    let i = index_of(parent, b);
    let open = match (
        i.checked_sub(2).map(|j| &kids[j]),
        kids.get(i.wrapping_sub(1)),
    ) {
        (Some(before), Some(Node::Text(gap)))
            if gap.text.contains('\n') && !crate::print::parts::is_line_comment(before) =>
        {
            Edit {
                start: gap.start,
                end: gap.start,
                text: " {".into(),
            }
        }
        _ => Edit {
            start: b.start,
            end: b.start,
            text: "{ ".into(),
        },
    };
    [
        open,
        Edit {
            start: b.end,
            end: b.end,
            text: " }".into(),
        },
    ]
}

/// Every control body gets braces: `if`/`else` branches, `while`/`for`/`loop` bodies, `case` and `catch` arms,
/// and `try`, `finally`, `async` and `async*` bodies.
fn brace_bodies(root: &Branch<'_>) -> Vec<Edit> {
    let mut edits = Vec::new();
    for n in root.branches() {
        let bodies = match n.kind {
            "if_exp" => vec![
                n.field("then"),
                n.field("else").filter(|e| e.kind != "if_exp"),
            ],
            "while_exp" | "for_exp" | "loop_exp" | "case" | "catch" => vec![n.field("body")],
            "try_exp" => vec![after(n, "try")],
            "finally" => vec![after(n, "finally")],
            "async_exp" => vec![after(n, "async")],
            "asyncstar_exp" => vec![after(n, "async*")],
            _ => continue,
        };
        for body in bodies.into_iter().flatten() {
            if !is_block(Some(body)) {
                edits.extend(brace(n, body));
            }
        }
    }
    edits
}

/// `else { … }` → `else do { … }`, and likewise after every keyword whose operand is an expression rather than a body,
/// where the target syntax reads `{` as a record.
fn do_blocks(root: &Branch<'_>) -> Vec<Edit> {
    let mut edits = Vec::new();
    for n in root.branches() {
        let operand = match n.kind {
            "let_else_dec" => after(n, "else"),
            "loop_exp" => n.field("condition"),
            "label_exp" => n
                .nodes()
                .filter_map(|c| c.as_branch().filter(|b| !b.extra))
                .last(),
            "debug_exp" => after(n, "debug"),
            "ignore_exp" => after(n, "ignore"),
            "assert_exp" => after(n, "assert"),
            "throw_exp" => after(n, "throw"),
            "await_exp" => after(n, "await"),
            "awaitstar_exp" => after(n, "await*"),
            "awaitquest_exp" => after(n, "await?"),
            _ => continue,
        };
        let Some(block) = operand.filter(|b| is_block(Some(b))) else {
            continue;
        };
        let (before, _) = padding(n, index_of(n, block), index_of(n, block));
        edits.push(Edit {
            start: block.start,
            end: block.start,
            text: format!("{before}do "),
        });
    }
    edits
}

/// Expression kinds moc accepts bare in a head. Statement-like forms and records keep their parens.
const HEAD_KINDS: &[&str] = &[
    "var_exp",
    "lit_exp",
    "call_exp",
    "dot_exp",
    "proj_exp",
    "array_idx_exp",
    "bin_exp",
    "not_exp",
    "unop_exp",
    "bang_exp",
    "coalesce_exp",
];

/// The head's branches outside nested parentheses, where moc reads an ordinary expression.
fn top_level<'n, 'a>(b: &'n Branch<'a>, out: &mut Vec<&'n Branch<'a>>) {
    out.push(b);
    for c in &b.children {
        if let Node::Branch(c) = c
            && c.kind != "par_exp"
        {
            top_level(c, out);
        }
    }
}

fn top_level_of<'n, 'a>(b: &'n Branch<'a>) -> Vec<&'n Branch<'a>> {
    let mut out = Vec::new();
    top_level(b, &mut out);
    out
}

/// The gap before a call's argument or an index's `[`, if there is one: `f x`, `f<T> (x)`, `a [i]`.
fn spaced_arg<'n, 'a>(b: &'n Branch<'a>) -> Option<&'n Node<'a>> {
    let kids = &b.children;
    let gap = match b.kind {
        "call_exp" => kids.len().checked_sub(2).map(|i| &kids[i]),
        "array_idx_exp" => kids.get(1),
        _ => None,
    };
    gap.filter(|g| g.is_text())
}

/// Whether a head can drop its parens; with `glue`, once `glue_head_calls` has glued its spaced arguments.
fn head_safe(inner: &Branch<'_>, glue: bool) -> bool {
    if !HEAD_KINDS.contains(&inner.kind) || inner.text.contains('\n') {
        return false;
    }
    top_level_of(inner).into_iter().all(|n| {
        let kids = &n.children;
        // `{` in a head is read as a record.
        if kids.iter().any(|c| c.is_token("{")) {
            return false;
        }
        // A spaced argument or `(`/`[` after a head starts the branch instead of continuing the head.
        if !glue && spaced_arg(n).is_some() {
            return false;
        }
        // So does a prefix-shaped `-`/`+`/`^`/`#` (spaced before, glued after), which the grammar reads as binary.
        let op = kids.iter().position(|c| {
            c.as_branch().is_some_and(|b| {
                b.kind == "bin_op" && matches!(b.text.trim(), "-" | "+" | "^" | "#")
            })
        });
        !op.is_some_and(|i| {
            i > 0 && kids[i - 1].is_text() && !kids.get(i + 1).is_some_and(Node::is_text)
        })
    })
}

/// The single expression inside a `( … )`, or `None` for a tuple, unit or anything with a comment.
fn parenthesised<'n, 'a>(par: Option<&'n Branch<'a>>) -> Option<&'n Branch<'a>> {
    let par = par.filter(|p| p.kind == "par_exp")?;
    let nodes: Vec<_> = par.nodes().collect();
    match nodes[..] {
        [_, Node::Branch(inner), _] => Some(inner),
        _ => None,
    }
}

fn head_of<'n, 'a>(n: &'n Branch<'a>) -> Option<&'n Branch<'a>> {
    match n.kind {
        "for_exp" => n.field("iterator"),
        "switch_exp" => parenthesised(n.field("scrutinee")),
        "if_exp" | "while_exp" | "loop_exp" => parenthesised(n.field("condition")),
        _ => None,
    }
}

/// `if (f x) {` → `if (f(x)) {`, only where that is all that keeps the head's parens.
fn glue_head_calls(root: &Branch<'_>) -> Vec<Edit> {
    let mut edits = Vec::new();
    for n in root.branches() {
        let Some(inner) = head_of(n) else { continue };
        if head_safe(inner, false) || !head_safe(inner, true) {
            continue;
        }
        for call in top_level_of(inner) {
            let Some(gap) = spaced_arg(call) else {
                continue;
            };
            let arg = call.children.last().unwrap();
            let bracketed = call.kind == "array_idx_exp"
                || arg.as_branch().is_some_and(|b| b.kind == "par_exp");
            edits.push(if bracketed {
                Edit {
                    start: gap.start(),
                    end: gap.end(),
                    text: String::new(),
                }
            } else {
                Edit {
                    start: gap.start(),
                    end: arg.end(),
                    text: format!("({})", arg.text()),
                }
            });
        }
    }
    edits
}

/// Whether every branch of an `if`, down its `else if` chain, is braced: only that shape may follow a bare head.
fn braced_chain(n: &Branch<'_>) -> bool {
    is_block(n.field("then"))
        && n.field("else")
            .is_none_or(|e| is_block(Some(e)) || (e.kind == "if_exp" && braced_chain(e)))
}

/// `if (c) {` → `if c {`, likewise `while`, `switch` and `loop { … } while (c)`, and `for (p in e) {` → `for p in e {`.
/// Only once every body is braced.
fn unparen_heads(root: &Branch<'_>) -> Vec<Edit> {
    let mut edits = Vec::new();
    for n in root.branches() {
        match n.kind {
            "if_exp" | "while_exp" | "switch_exp" | "loop_exp" => {
                let name = if n.kind == "switch_exp" {
                    "scrutinee"
                } else {
                    "condition"
                };
                let Some(par) = n.field(name) else { continue };
                let Some(inner) = parenthesised(Some(par)) else {
                    continue;
                };
                if !head_safe(inner, false) {
                    continue;
                }
                if n.kind == "if_exp" && !braced_chain(n) {
                    continue;
                }
                if matches!(n.kind, "while_exp" | "loop_exp") && !is_block(n.field("body")) {
                    continue;
                }
                let i = index_of(n, par);
                let (before, after) = padding(n, i, i);
                edits.push(Edit {
                    start: par.start,
                    end: par.end,
                    text: format!("{before}{}{after}", inner.text),
                });
            }
            "for_exp" => {
                let open = n.children.iter().position(|c| c.is_token("("));
                let close = n.children.iter().position(|c| c.is_token(")"));
                let (Some(open), Some(close)) = (open, close) else {
                    continue;
                };
                let Some(iterator) = n.field("iterator") else {
                    continue;
                };
                if !head_safe(iterator, false) || !is_block(n.field("body")) {
                    continue;
                }
                let inside: String = n.children[open + 1..close].iter().map(Node::text).collect();
                // A line comment before `)` would swallow the body's `{`, as a line break would split the head.
                if inside.contains('\n') {
                    continue;
                }
                let (before, after) = padding(n, open, close);
                edits.push(Edit {
                    start: n.children[open].start(),
                    end: n.children[close].end(),
                    text: format!("{before}{}{after}", inside.trim()),
                });
            }
            _ => {}
        }
    }
    edits
}

/// A braced arm needs no `;`: `case` already ends the previous one.
fn drop_case_semis(root: &Branch<'_>) -> Vec<Edit> {
    let mut edits = Vec::new();
    for n in root
        .branches()
        .into_iter()
        .filter(|n| n.kind == "switch_exp")
    {
        let kids: Vec<_> = n.nodes().collect();
        for pair in kids.windows(2) {
            let [prev, semi] = pair else { unreachable!() };
            if semi.is_token(";")
                && let Node::Branch(arm) = prev
                && arm.kind == "case"
                && is_block(arm.field("body"))
            {
                edits.push(Edit {
                    start: semi.start(),
                    end: semi.end(),
                    text: String::new(),
                });
            }
        }
    }
    edits
}

const BARE_PATTERNS: &[&str] = &[
    "lit_pat", "var_pat", "wild_pat", "unop_pat", "obj_pat", "tup_pat",
];

/// The pattern as a case pattern spells it without outer parentheses: a variant's payload goes in `#tag(…)`,
/// and `?p`, `or`, `and` and `: T` take each operand so. `None` for a pattern that needs its parentheses.
fn bare_pattern(p: &Branch<'_>) -> Option<String> {
    if BARE_PATTERNS.contains(&p.kind) {
        return Some(p.text.to_string());
    }
    match p.kind {
        "tag_pat" => {
            let parts: Vec<_> = p.nodes().collect();
            match parts[..] {
                [tag] => Some(tag.text().to_string()),
                [tag, Node::Branch(payload)] if payload.kind == "tup_pat" => {
                    Some(format!("{}{}", tag.text(), payload.text))
                }
                [tag, payload] => Some(format!("{}({})", tag.text(), payload.text())),
                _ => None,
            }
        }
        "quest_pat" | "alt_pat" | "and_pat" | "annot_pat" => {
            let mut out = String::new();
            for c in &p.children {
                match c {
                    Node::Branch(b) if b.kind.ends_with("_pat") => out.push_str(&bare_pattern(b)?),
                    _ => out.push_str(c.text()),
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// `case (p) {` → `case p {`, `case (#t x) {` → `case #t(x) {`, and likewise `catch (e) {` → `catch e {`.
fn unparen_patterns(root: &Branch<'_>) -> Vec<Edit> {
    let mut edits = Vec::new();
    for n in root.branches() {
        if !matches!(n.kind, "case" | "catch") || !is_block(n.field("body")) {
            continue;
        }
        let Some(par) = n.field("pattern").filter(|p| p.kind == "tup_pat") else {
            continue;
        };
        let inner: Vec<_> = par.nodes().collect();
        let [_, Node::Branch(pattern), _] = inner[..] else {
            continue;
        };
        let Some(text) = bare_pattern(pattern).filter(|t| !t.contains('\n')) else {
            continue;
        };
        // `case(null)` has no gap after `case` to keep the two apart.
        let i = index_of(n, par);
        let (before, after) = padding(n, i, i);
        edits.push(Edit {
            start: par.start,
            end: par.end,
            text: format!("{before}{text}{after}"),
        });
    }
    edits
}

/// A list's last item and the separator after it, if any: what a trailing-separator rule looks at.
struct Tail<'n, 'a> {
    list: &'n Branch<'a>,
    items: usize,
    last: &'n Node<'a>,
    separator: Option<&'n Node<'a>>,
}

/// Every list of `family` with at least one item.
fn tails<'n, 'a>(root: &'n Branch<'a>, family: &[Family]) -> Vec<Tail<'n, 'a>> {
    let mut out = Vec::new();
    for b in root.branches() {
        let Some(list) = list_of(b).filter(|l| family.contains(&l.family)) else {
            continue;
        };
        let sep = if list.family == Family::CommaSep {
            ","
        } else {
            ";"
        };
        let inner: Vec<_> = b
            .nodes()
            .filter(|c| !c.is_token(list.open) && !c.is_token(list.close))
            .collect();
        // `var` in `[var …]` is a keyword, not an item.
        let is_item = |c: &Node<'_>| {
            !crate::print::parts::is_comment(c)
                && match c {
                    Node::Branch(_) => true,
                    Node::Token(t) => t.named,
                    Node::Text(_) => false,
                }
        };
        let Some(i) = inner.iter().rposition(|c| is_item(c)) else {
            continue;
        };
        out.push(Tail {
            list: b,
            items: inner.iter().filter(|c| is_item(c)).count(),
            last: inner[i],
            separator: inner[i + 1..].iter().copied().find(|c| c.is_token(sep)),
        });
    }
    out
}

fn delete(n: &Node<'_>) -> Edit {
    Edit {
        start: n.start(),
        end: n.end(),
        text: String::new(),
    }
}

/// `{ a; b; }` → `{ a; b }`: moc ignores a `;` after the last item, of a block, body, record, object or variant type,
/// braced pattern, or file.
fn drop_trailing_semis(root: &Branch<'_>) -> Vec<Edit> {
    tails(root, &[Family::SemiSep, Family::SemiSep1])
        .into_iter()
        .filter_map(|t| t.separator.map(delete))
        .collect()
}

/// `(a, b,)` → `(a, b)`: moc ignores a `,` after the last item.
fn no_trailing_commas(root: &Branch<'_>) -> Vec<Edit> {
    tails(root, &[Family::CommaSep])
        .into_iter()
        .filter_map(|t| t.separator.map(delete))
        .collect()
}

/// A `,` after the last item of a list broken one item per line, as the printer lays it out, and none on a list on one line.
/// A parenthesised single item never gets one, since `(x,)` reads as a one-tuple although moc reads it as `(x)`,
/// and nor does a `<…>` list, whose `>` moc needs glued to the last item.
fn multiline_commas(root: &Branch<'_>) -> Vec<Edit> {
    let mut edits = Vec::new();
    for t in tails(root, &[Family::CommaSep]) {
        let list = list_of(t.list).expect("a list");
        let broken = t.list.children.iter().any(Node::is_break);
        let single_paren = t.items == 1 && list.open == "(";
        let want = broken && !single_paren && !list.close_glued;
        match (want, t.separator) {
            (false, Some(sep)) => edits.push(delete(sep)),
            (true, None) => edits.push(Edit {
                start: t.last.end(),
                end: t.last.end(),
                text: ",".into(),
            }),
            _ => {}
        }
    }
    edits
}

/// `{\n\n  a;\n\n}` → `{\n  a;\n}`: no blank line just inside the braces of a block, body, `switch`, record or type.
fn trim_block_blank_lines(root: &Branch<'_>) -> Vec<Edit> {
    let mut edits = Vec::new();
    for b in root.branches() {
        if !(b.kind == "switch_exp" || list_of(b).is_some_and(|l| l.open == "{")) {
            continue;
        }
        let kids = &b.children;
        let open = kids.iter().position(|c| c.is_token("{"));
        let close = kids.iter().rposition(|c| c.is_token("}"));
        let gaps = [open.map(|i| i + 1), close.and_then(|i| i.checked_sub(1))];
        for gap in gaps.into_iter().flatten().filter_map(|i| kids.get(i)) {
            if let Node::Text(g) = gap
                && g.text.matches('\n').count() >= 2
            {
                let indent = &g.text[g.text.rfind('\n').unwrap() + 1..];
                edits.push(Edit {
                    start: g.start,
                    end: g.start + g.text.len(),
                    text: format!("\n{indent}"),
                });
            }
        }
    }
    edits
}

/// One import with the comments that go with it: those on the lines above it, and those after it on its line.
struct ImportEntry<'a> {
    above: Vec<(&'a str, bool)>,
    import: &'a Branch<'a>,
    semi: bool,
    after: Vec<&'a str>,
}

impl ImportEntry<'_> {
    /// Packages, then canisters, then local files: external before local, as goimports and isort group them.
    fn group(&self) -> u8 {
        let path = self.path();
        if path.starts_with("mo:") {
            0
        } else if path.starts_with("canister:") || path.starts_with("ic:") {
            1
        } else {
            2
        }
    }

    fn path(&self) -> &str {
        self.import
            .nodes()
            .filter(|c| c.ty() == Some("text_literal"))
            .last()
            .map_or("", |t| t.text().trim_matches('"'))
    }

    fn text(&self, semi: bool) -> String {
        let mut out = String::new();
        for (comment, own_line) in &self.above {
            out.push_str(comment);
            out.push_str(if *own_line { "\n" } else { " " });
        }
        out.push_str(self.import.text);
        if semi {
            out.push(';');
        }
        for comment in &self.after {
            out.push(' ');
            out.push_str(comment);
        }
        out
    }
}

/// Groups the imports at the top of the file, with a blank line between groups, and sorts each group by path.
/// The comments above the first import are the file's header, such as its module doc comment or a pragma, and stay put.
fn organize_imports(root: &Branch<'_>) -> Vec<Edit> {
    if root.kind != "source_file" {
        return Vec::new();
    }
    let mut entries: Vec<ImportEntry<'_>> = Vec::new();
    let mut above: Vec<(&str, bool)> = Vec::new();
    let mut start = None;
    let mut end = 0;
    let mut gap = "";
    for child in &root.children {
        if let Node::Text(g) = child {
            gap = g.text;
            continue;
        }
        let on_new_line = gap.contains('\n');
        gap = "";
        if let Some(last) = above.last_mut() {
            last.1 = on_new_line;
        }
        if crate::print::parts::is_comment(child) {
            match entries.last_mut() {
                // The header, outside what gets sorted.
                None => {}
                Some(last) if !on_new_line && above.is_empty() => {
                    last.after.push(child.text());
                    end = child.end();
                }
                Some(_) => above.push((child.text(), true)),
            }
            continue;
        }
        match child {
            Node::Branch(b) if b.kind == "import" => {
                start.get_or_insert(b.start);
                entries.push(ImportEntry {
                    above: std::mem::take(&mut above),
                    import: b,
                    semi: false,
                    after: Vec::new(),
                });
                end = b.end;
            }
            _ if child.is_token(";") && !entries.is_empty() && above.is_empty() => {
                entries.last_mut().unwrap().semi = true;
                end = child.end();
            }
            _ => break,
        }
    }
    let Some(start) = start else {
        return Vec::new();
    };
    let last_semi = entries.last().is_some_and(|e| e.semi);
    let mut sorted: Vec<&ImportEntry<'_>> = entries.iter().collect();
    sorted.sort_by(|a, b| (a.group(), a.path()).cmp(&(b.group(), b.path())));
    let mut text = String::new();
    for (i, e) in sorted.iter().enumerate() {
        if i > 0 {
            text.push_str(if sorted[i - 1].group() != e.group() {
                "\n\n"
            } else {
                "\n"
            });
        }
        text.push_str(&e.text(i + 1 < sorted.len() || last_semi));
    }
    vec![Edit { start, end, text }]
}
