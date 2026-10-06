use super::parts::{is_comment, newlines};
use super::{Ctx, node_doc, space};
use crate::doc::{Doc, EMPTY, HardLine, concat, indent, text};
use crate::tree::{Branch, Node};

pub fn control_doc<'a>(node: &Branch<'a>, ctx: &Ctx<'a>) -> Option<Doc<'a>> {
    if node.kind != "switch_exp" {
        return None;
    }
    let children = &node.children;
    // `switch`, the scrutinee and `{`, glued or not.
    let mut head = (0..children.len()).filter(|&i| !children[i].is_text());
    let (Some(keyword), Some(scrutinee), Some(open)) = (head.next(), head.next(), head.next())
    else {
        return None;
    };
    let (keyword, scrutinee) = (&children[keyword], &children[scrutinee]);
    let close = children.last()?;
    if !keyword.is_token("switch") || !children[open].is_token("{") || !close.is_token("}") {
        return None;
    }
    scrutinee.as_branch()?;

    let mut run: Vec<Doc<'a>> = Vec::new();
    let mut gap = None;
    for child in &children[open + 1..children.len() - 1] {
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
            indent(concat([HardLine, blank(&children[open + 1]), concat(run)])),
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
    // `case`, the pattern and the body, glued or not.
    let (keyword, pattern, gap, body) = match &arm.children[..] {
        [k, Node::Text(_), p, Node::Text(g), b] | [k, p, Node::Text(g), b] => {
            (k, p, Some(g.text), b)
        }
        [k, Node::Text(_), p, b] | [k, p, b] => (k, p, None, b),
        _ => return None,
    };
    if !keyword.is_token("case") || pattern.as_branch().is_none() || body.as_branch().is_none() {
        return None;
    }
    let body_doc = node_doc(body, ctx);
    let rest = match gap {
        Some(g) if g.contains('\n') => indent(concat([HardLine, body_doc])),
        _ => concat([
            space::join_doc(space::join(arm, pattern, body), gap),
            body_doc,
        ]),
    };
    Some(concat([
        text("case"),
        text(" "),
        node_doc(pattern, ctx),
        rest,
    ]))
}
