use super::parts::{is_comment, newlines};
use super::{Ctx, node_doc};
use crate::doc::{Doc, EMPTY, HardLine, concat, indent, text};
use crate::tree::{Branch, Node};

pub fn control_doc<'a>(node: &Branch<'a>, ctx: &Ctx<'a>) -> Option<Doc<'a>> {
    if node.kind != "switch_exp" {
        return None;
    }
    let children = &node.children;
    if children.len() < 9 {
        return None;
    }
    let [keyword, gap1, scrutinee, gap2, open] = &children[..5] else {
        return None;
    };
    let close = children.last()?;
    if !keyword.is_token("switch") || !open.is_token("{") || !close.is_token("}") {
        return None;
    }
    if !gap1.is_text() || !gap2.is_text() || scrutinee.as_branch().is_none() {
        return None;
    }

    let mut run: Vec<Doc<'a>> = Vec::new();
    let mut gap = None;
    for child in &children[5..children.len() - 1] {
        match child {
            Node::Text(g) => {
                gap = Some(g.text);
                continue;
            }
            _ if child.is_token(";") => {
                if run.is_empty() {
                    return None;
                }
                run.push(text(";"));
                gap = None;
                continue;
            }
            _ => {}
        }
        let item = if is_comment(child) {
            node_doc(child, ctx)
        } else {
            arm_doc(child.as_branch()?, ctx)?
        };
        if !run.is_empty() {
            run.push(separator(gap));
        }
        run.push(item);
        gap = None;
    }
    if run.is_empty() {
        return None;
    }

    let broken = children.iter().any(Node::is_break);
    let head = concat([
        text("switch"),
        text(" "),
        node_doc(scrutinee, ctx),
        text(" "),
        text("{"),
    ]);
    let blank = |c: &Node<'_>| {
        if matches!(c, Node::Text(g) if newlines(g.text) >= 2) {
            HardLine
        } else {
            EMPTY
        }
    };
    Some(if broken {
        concat([
            head,
            indent(concat([HardLine, blank(&children[5]), concat(run)])),
            blank(&children[children.len() - 2]),
            HardLine,
            text("}"),
        ])
    } else {
        concat([head, text(" "), concat(run), text(" "), text("}")])
    })
}

fn separator(gap: Option<&str>) -> Doc<'static> {
    match gap {
        Some(g) if newlines(g) >= 2 => concat([HardLine, HardLine]),
        Some(g) if g.contains('\n') => HardLine,
        _ => text(" "),
    }
}

fn arm_doc<'a>(arm: &Branch<'a>, ctx: &Ctx<'a>) -> Option<Doc<'a>> {
    let [keyword, gap1, pattern, gap2, body] = &arm.children[..] else {
        return None;
    };
    if !keyword.is_token("case")
        || !gap1.is_text()
        || pattern.as_branch().is_none()
        || body.as_branch().is_none()
    {
        return None;
    }
    let body_doc = node_doc(body, ctx);
    let rest = if gap2.is_break() {
        indent(concat([HardLine, body_doc]))
    } else {
        concat([text(" "), body_doc])
    };
    gap2.is_text()
        .then(|| concat([text("case"), text(" "), node_doc(pattern, ctx), rest]))
}
