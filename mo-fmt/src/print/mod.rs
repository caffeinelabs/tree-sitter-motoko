//! The `preserve` printer: normalises spacing and indentation, and keeps the author's line breaks.

mod control;
mod exp;
mod parts;

use crate::doc::{
    BreakParent, Doc, EMPTY, HardLine, align, concat, group, text, verbatim as verbatim_text,
};
use crate::tree::{Branch, Lines, Node};
use parts::{
    Item, List, between_separator, blank_in, has_blank_line, is_comment, is_line_comment,
    list_indent, list_items, list_of, newlines, separator_line, trailing_separator, verbatim,
};

pub(crate) struct Ctx<'a> {
    lines: Lines<'a>,
}

pub fn print<'a>(root: &Node<'a>, source: &'a str) -> Doc<'a> {
    let ctx = Ctx {
        lines: Lines::new(source),
    };
    let doc = match root {
        Node::Branch(b) if list_of(b).is_some() => source_file_doc(b, &ctx),
        _ => node_doc(root, &ctx),
    };
    group(doc, false)
}

fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| c.is_whitespace()).count()
}

fn is_ignore_directive(n: &Node<'_>) -> bool {
    if !is_comment(n) {
        return false;
    }
    let mut s = n.text();
    s = s.strip_prefix("//").unwrap_or(s);
    if let Some(rest) = s.strip_prefix('/').filter(|r| r.starts_with('*')) {
        s = rest.trim_start_matches('*');
    }
    if let Some(rest) = s.strip_suffix('/').filter(|r| r.ends_with('*')) {
        s = rest.trim_end_matches('*');
    }
    matches!(s.trim(), "mo-fmt-ignore" | "prettier-ignore")
}

pub(crate) fn node_doc<'a>(n: &Node<'a>, ctx: &Ctx<'a>) -> Doc<'a> {
    match n {
        Node::Text(g) => verbatim_text(g.text),
        _ if is_comment(n) => concat([verbatim(n), BreakParent]),
        Node::Token(_) => verbatim(n),
        Node::Branch(b) => {
            if let Some(list) = list_of(b) {
                return list_doc(b, list, ctx);
            }
            if let Some(doc) = exp::binary_chain_doc(b, ctx) {
                return doc;
            }
            if let Some(doc) = control::control_doc(b, ctx) {
                return doc;
            }
            branch_doc(b, ctx)
        }
    }
}

/// A node with no layout of its own: children with the source's gaps.
/// After a copied line break, the rest sits at its source column relative to the node's first line,
/// so a list nested inside indents from where the author put it.
fn branch_doc<'a>(b: &Branch<'a>, ctx: &Ctx<'a>) -> Doc<'a> {
    let base = indent_of(ctx.lines.line(ctx.lines.row(b.start)));
    let mut out = Vec::new();
    let mut segment: Option<(usize, Vec<Doc<'a>>)> = None;
    for child in &b.children {
        if let Node::Text(g) = child
            && g.text.contains('\n')
        {
            if let Some((n, docs)) = segment.take() {
                out.push(align(n, concat(docs)));
            }
            let column = g.text[g.text.rfind('\n').unwrap() + 1..].chars().count();
            let breaks = if newlines(g.text) >= 2 {
                vec![HardLine, HardLine]
            } else {
                vec![HardLine]
            };
            segment = Some((column.saturating_sub(base), breaks));
            continue;
        }
        let doc = node_doc(child, ctx);
        match &mut segment {
            Some((_, docs)) => docs.push(doc),
            None => out.push(doc),
        }
    }
    if let Some((n, docs)) = segment {
        out.push(align(n, concat(docs)));
    }
    concat(out)
}

fn list_doc<'a>(b: &Branch<'a>, list: List, ctx: &Ctx<'a>) -> Doc<'a> {
    let items = list_items(b, true);
    let Some(last) = items.last() else {
        return if has_blank_line(b) {
            concat([text(list.open), HardLine, HardLine, text(list.close)])
        } else {
            concat([text(list.open), text(list.close)])
        };
    };

    // The author's layout decides: a list with a line break breaks one item per line, any other stays on one line.
    let flat = !b.children.iter().any(Node::is_break);
    let before_close = b
        .children
        .iter()
        .rposition(|c| c.is_token(list.close))
        .and_then(|i| i.checked_sub(1))
        .map(|i| &b.children[i]);
    let blank_after_open = !flat && blank_in(items[0].gap);
    let blank_before_close =
        !flat && matches!(before_close, Some(Node::Text(g)) if blank_in(Some(g.text)));
    let last_is_line_comment = is_line_comment(last.node);
    let body = concat([
        if blank_after_open { HardLine } else { EMPTY },
        list_items_doc(&items, list, ctx, flat),
        if blank_before_close { HardLine } else { EMPTY },
    ]);
    group(
        concat([
            text(list.open),
            list_indent(body, list, last_is_line_comment, flat),
            text(list.close),
        ]),
        !flat,
    )
}

fn list_items_doc<'a>(items: &[Item<'_, 'a>], list: List, ctx: &Ctx<'a>, flat: bool) -> Doc<'a> {
    let mut out = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let printed = item_doc(items, i, ctx);
        let Some(next) = items.get(i + 1) else {
            out.push(printed);
            out.push(trailing_separator(
                list.family,
                item.separated,
                is_line_comment(item.node),
            ));
            break;
        };
        let sep = if item.separated {
            between_separator(
                list.family,
                &printed,
                next.gap,
                is_line_comment(item.node),
                is_comment(next.node),
                flat,
            )
        } else {
            // A block comment stays on the line of the item it annotates, as in `/* x = */ one`.
            let inline_comment = is_comment(item.node)
                && !is_line_comment(item.node)
                && !next.gap.is_some_and(|g| g.contains('\n'));
            if flat || inline_comment {
                text(" ")
            } else {
                separator_line(&printed, next.gap, is_comment(next.node))
            }
        };
        out.push(printed);
        out.push(sep);
    }
    concat(out)
}

fn item_doc<'a>(items: &[Item<'_, 'a>], i: usize, ctx: &Ctx<'a>) -> Doc<'a> {
    let item = &items[i];
    let ignored = i > 0 && is_ignore_directive(items[i - 1].node);
    let mut out = vec![if ignored {
        verbatim(item.node)
    } else {
        node_doc(item.node, ctx)
    }];
    for (gap, node) in &item.rest {
        if ignored {
            out.push(text(gap.unwrap_or("")));
            out.push(verbatim(node));
        } else {
            out.push(match gap {
                None => EMPTY,
                Some(g) if g.contains('\n') => HardLine,
                Some(_) => text(" "),
            });
            out.push(node_doc(node, ctx));
        }
    }
    concat(out)
}

fn source_file_doc<'a>(b: &Branch<'a>, ctx: &Ctx<'a>) -> Doc<'a> {
    let items = list_items(b, false);
    if items.is_empty() {
        return EMPTY;
    }
    let mut out = Vec::new();
    for (i, item) in items.iter().enumerate() {
        out.push(item_doc(&items, i, ctx));
        out.push(trailing_separator(
            parts::Family::SemiSep,
            item.separated,
            is_line_comment(item.node),
        ));
        if let Some(next) = items.get(i + 1) {
            let comment_on_this_line =
                is_comment(next.node) && !next.gap.is_some_and(|g| g.contains('\n'));
            out.push(if comment_on_this_line {
                text(" ")
            } else if blank_in(next.gap) {
                concat([HardLine, HardLine])
            } else {
                HardLine
            });
        }
    }
    out.push(HardLine);
    concat(out)
}
