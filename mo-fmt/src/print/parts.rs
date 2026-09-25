use crate::doc::{self, Doc, EMPTY, HardLine, Line, SoftLine, concat, indent, text};
use crate::tree::{Branch, Node};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Family {
    SemiSep,
    SemiSep1,
    CommaSep,
}

#[derive(Clone, Copy)]
pub struct List {
    pub family: Family,
    pub open: &'static str,
    pub close: &'static str,
    pub spaced: bool,
    /// moc lexes a spaced `>` as `GTOP`, so a `>` on its own line is a syntax error the guard cannot see.
    pub close_glued: bool,
}

const fn list(family: Family, open: &'static str, close: &'static str, spaced: bool) -> List {
    List {
        family,
        open,
        close,
        spaced,
        close_glued: false,
    }
}

pub fn list_of(b: &Branch<'_>) -> Option<List> {
    use Family::*;
    Some(match b.kind {
        "block_exp" | "obj_body" | "obj_typ" | "obj_pat" => list(SemiSep, "{", "}", true),
        "object_exp" if b.children.iter().any(|c| c.is_token("with")) => {
            list(SemiSep1, "{", "}", true)
        }
        "object_exp" => list(SemiSep, "{", "}", true),
        "source_file" => list(SemiSep, "", "", false),
        "variant_typ" => list(SemiSep1, "{", "}", true),
        "par_exp" | "tup_typ" | "tup_pat" => list(CommaSep, "(", ")", false),
        "array_exp" => list(CommaSep, "[", "]", false),
        "typ_params" | "inst" => List {
            close_glued: true,
            ..list(CommaSep, "<", ">", false)
        },
        _ => return None,
    })
}

fn is_item(c: &Node<'_>) -> bool {
    match c {
        Node::Text(_) => false,
        Node::Branch(_) => true,
        Node::Token(t) => !matches!(
            t.text,
            ";" | "," | "{" | "}" | "(" | ")" | "[" | "]" | "<" | ">"
        ),
    }
}

pub struct Item<'n, 'a> {
    pub node: &'n Node<'a>,
    pub gap: Option<&'a str>,
    pub separated: bool,
    /// Unseparated nodes that continue the item, like `and b with c = 1` after `a`, or the element after `var`.
    pub rest: Vec<(Option<&'a str>, &'n Node<'a>)>,
}

pub fn list_items<'n, 'a>(b: &'n Branch<'a>, join: bool) -> Vec<Item<'n, 'a>> {
    let mut out: Vec<Item<'n, 'a>> = Vec::new();
    let mut gap = None;
    for child in &b.children {
        if let Node::Text(g) = child {
            gap = Some(g.text);
            continue;
        }
        if is_item(child) {
            let continues = join
                && out.last().is_some_and(|last| {
                    !last.separated
                        && !is_comment(last.node)
                        && !is_comment(child)
                        && last.rest.iter().all(|(_, r)| !is_comment(r))
                });
            if continues {
                out.last_mut().unwrap().rest.push((gap, child));
            } else {
                out.push(Item {
                    node: child,
                    gap,
                    separated: false,
                    rest: Vec::new(),
                });
            }
        } else if (child.is_token(";") || child.is_token(",")) && !out.is_empty() {
            out.last_mut().unwrap().separated = true;
        }
        gap = None;
    }
    out
}

pub fn newlines(s: &str) -> usize {
    s.matches('\n').count()
}

pub fn blank_in(gap: Option<&str>) -> bool {
    gap.is_some_and(|g| newlines(g) >= 2)
}

pub fn has_blank_line(b: &Branch<'_>) -> bool {
    b.children
        .iter()
        .any(|c| matches!(c, Node::Text(g) if newlines(g.text) >= 2))
}

pub fn separator_line<'a>(left: &Doc<'_>, gap: Option<&str>, next_is_comment: bool) -> Doc<'a> {
    // A `Line` here would break under the comment's `BreakParent` and detach it from its item.
    if next_is_comment && !gap.is_some_and(|g| g.contains('\n')) {
        return text(" ");
    }
    if blank_in(gap) {
        return concat([HardLine, HardLine]);
    }
    if left.will_break() {
        return HardLine;
    }
    Line
}

fn separator_char(family: Family) -> Doc<'static> {
    text(if family == Family::CommaSep { "," } else { ";" })
}

pub fn trailing_separator(
    family: Family,
    separated: bool,
    last_is_line_comment: bool,
) -> Doc<'static> {
    if !separated {
        return EMPTY;
    }
    if last_is_line_comment {
        concat([HardLine, separator_char(family)])
    } else {
        separator_char(family)
    }
}

pub fn between_separator<'a>(
    family: Family,
    left: &Doc<'_>,
    gap: Option<&str>,
    left_is_line_comment: bool,
    right_is_comment: bool,
    flat: bool,
) -> Doc<'a> {
    let sep = separator_char(family);
    if flat {
        return concat([sep, text(" ")]);
    }
    let line = separator_line(left, gap, right_is_comment);
    if left_is_line_comment {
        concat([HardLine, sep, line])
    } else {
        concat([sep, line])
    }
}

pub fn is_comment(n: &Node<'_>) -> bool {
    matches!(
        n.ty(),
        Some("line_comment" | "block_comment" | "doc_comment")
    )
}

pub fn is_line_comment(n: &Node<'_>) -> bool {
    n.ty() == Some("line_comment")
}

fn inner_break(list: List, flat: bool) -> Doc<'static> {
    match (flat, list.spaced) {
        (true, true) => text(" "),
        (true, false) => EMPTY,
        (false, true) => Line,
        (false, false) => SoftLine,
    }
}

pub fn list_indent(joined: Doc<'_>, list: List, last_is_line_comment: bool, flat: bool) -> Doc<'_> {
    let b = inner_break(list, flat);
    // A flat list adds no indent, so a broken item it hugs (`f([ … ])`) keeps the author's indentation.
    if flat {
        let close = if list.close_glued { EMPTY } else { b.clone() };
        return concat([b, joined, close]);
    }
    let body = indent(concat([b.clone(), joined]));
    match (list.close_glued, last_is_line_comment) {
        (true, true) => concat([body, HardLine]),
        (true, false) => body,
        (false, _) => concat([body, b]),
    }
}

pub fn verbatim<'a>(n: &Node<'a>) -> Doc<'a> {
    doc::verbatim(n.text())
}
