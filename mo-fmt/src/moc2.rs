//! The `moc2` rewrite: legacy syntax to the moc 2.0 forms, as edits to the source text located by the tree.

use crate::Error;
use crate::tree::{Branch, Node, parse};

struct Edit {
    start: usize,
    end: usize,
    text: String,
}

type Rule = fn(&Branch<'_>) -> Vec<Edit>;

const RULES: &[Rule] = &[
    brace_bodies,
    glue_head_calls,
    unparen_heads,
    drop_case_semis,
    unwrap_case_patterns,
];

// Every rule removes what it matches, so a real file settles in a few rounds.
const MAX_ROUNDS: usize = 100;

/// Rewrites legacy syntax to the moc 2.0 forms. Each rule runs to a fixed point before the next, since later rules need braced bodies.
/// `Error::Syntax` if the input doesn't parse, and `Error::Internal` if a rule breaks it or never settles.
pub fn rewrite(source: &str) -> Result<String, Error> {
    let mut text = source.to_string();
    for rule in RULES {
        for round in 0.. {
            if round == MAX_ROUNDS {
                return Err(Error::Internal("a moc2 rule did not settle".into()));
            }
            let root = parse(&text).map_err(|e| {
                if text == source {
                    Error::Syntax(e)
                } else {
                    Error::Internal(format!(
                        "the moc2 rewrite produced code that does not parse ({e})"
                    ))
                }
            })?;
            let Node::Branch(root) = &root else {
                unreachable!("the root is a branch")
            };
            let edits = rule(root);
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

fn brace(b: &Branch<'_>) -> [Edit; 2] {
    [
        Edit {
            start: b.start,
            end: b.start,
            text: "{ ".into(),
        },
        Edit {
            start: b.end,
            end: b.end,
            text: " }".into(),
        },
    ]
}

/// Every control body gets braces: `if`/`else` branches, `while`/`for`/`loop` bodies, `case` and `catch` arms.
fn brace_bodies(root: &Branch<'_>) -> Vec<Edit> {
    let mut edits = Vec::new();
    for n in root.branches() {
        let bodies = match n.kind {
            "if_exp" => vec![
                n.field("then"),
                n.field("else").filter(|e| e.kind != "if_exp"),
            ],
            "while_exp" | "for_exp" | "loop_exp" | "case" | "catch" => vec![n.field("body")],
            _ => continue,
        };
        for body in bodies.into_iter().flatten() {
            if !is_block(Some(body)) {
                edits.extend(brace(body));
            }
        }
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
        "if_exp" | "while_exp" => parenthesised(n.field("condition")),
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

/// `if (c) {` → `if c {`, likewise `while` and `switch`, and `for (p in e) {` → `for p in e {`. Only once every body is braced.
fn unparen_heads(root: &Branch<'_>) -> Vec<Edit> {
    let mut edits = Vec::new();
    for n in root.branches() {
        match n.kind {
            "if_exp" | "while_exp" | "switch_exp" => {
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
                if n.kind == "if_exp" {
                    let otherwise = n.field("else");
                    if !is_block(n.field("then"))
                        || otherwise.is_some_and(|e| !is_block(Some(e)) && e.kind != "if_exp")
                    {
                        continue;
                    }
                }
                if n.kind == "while_exp" && !is_block(n.field("body")) {
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
    "lit_pat",
    "var_pat",
    "wild_pat",
    "quest_pat",
    "unop_pat",
    "obj_pat",
    "tup_pat",
];

/// `case (p) {` → `case p {`, and `case (#t x) {` → `case #t(x) {`.
fn unwrap_case_patterns(root: &Branch<'_>) -> Vec<Edit> {
    let mut edits = Vec::new();
    for n in root.branches() {
        if n.kind != "case" || !is_block(n.field("body")) {
            continue;
        }
        let Some(par) = n.field("pattern").filter(|p| p.kind == "tup_pat") else {
            continue;
        };
        let inner: Vec<_> = par.nodes().collect();
        let [_, Node::Branch(pattern), _] = inner[..] else {
            continue;
        };
        let text = if BARE_PATTERNS.contains(&pattern.kind) {
            pattern.text.to_string()
        } else if pattern.kind == "tag_pat" {
            let parts: Vec<_> = pattern.nodes().collect();
            match parts[..] {
                [tag] => tag.text().to_string(),
                [tag, Node::Branch(payload), ..] if payload.kind == "tup_pat" => {
                    format!("{}{}", tag.text(), payload.text)
                }
                [tag, payload, ..] => format!("{}({})", tag.text(), payload.text()),
                [] => continue,
            }
        } else {
            continue;
        };
        if !text.contains('\n') {
            // `case(null)` has no gap after `case` to keep the two apart.
            let i = index_of(n, par);
            let (before, after) = padding(n, i, i);
            edits.push(Edit {
                start: par.start,
                end: par.end,
                text: format!("{before}{text}{after}"),
            });
        }
    }
    edits
}
