use super::{Ctx, node_doc};
use crate::doc::{Doc, HardLine, concat, indent, text};
use crate::tree::{Branch, Node};

/// Operators whose two characters must never be split or spaced apart.
const INDIVISIBLE: &[&str] = &[
    ":=", "+=", "-=", "*=", "/=", "%=", "#=", "**=", "+%=", "-%=", "|=", "&=", "^=", "<<=", ">>=",
    "<<>=", "<>>=", "|>", "**", "+%", "-%", "*%", "->", "await*", "async*", "await?", "<<", ">>",
    "<<>", "<>>",
];

// In a control head moc reads `a -1` (spaced before, glued after) as a prefix starting the branch; the grammar reads a subtraction.
const PREFIX_SHAPED: &[&str] = &["-", "+", "^"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Break {
    Before,
    After,
}

struct Chain<'n, 'a> {
    operands: Vec<&'n Node<'a>>,
    ops: Vec<&'a str>,
    /// Where the source broke the line around each operator.
    breaks: Vec<Option<Break>>,
    /// The source column each broken line started at.
    columns: Vec<Option<usize>>,
}

fn op_text<'a>(n: &Node<'a>) -> Option<&'a str> {
    let b = n.as_branch()?;
    if !matches!(b.kind, "bin_op" | "rel_op") || b.children.len() != 1 {
        return None;
    }
    Some(b.children[0].token_text()?.trim())
}

// Spaced `#` is concatenation and glued `#` a variant tag, so a break beside it changes its role.
fn chainable(op: &str) -> bool {
    op == "|>" || (op != "#" && !INDIVISIBLE.contains(&op))
}

fn three_parts<'n, 'a>(level: &'n Branch<'a>) -> Option<[(usize, &'n Node<'a>); 3]> {
    if !(3..=5).contains(&level.children.len()) {
        return None;
    }
    let parts: Vec<_> = level
        .children
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.is_text())
        .collect();
    parts.try_into().ok()
}

fn source_break(level: &Branch<'_>, op: usize) -> Option<Break> {
    let before = op.checked_sub(1).and_then(|i| level.children.get(i));
    if before.is_some_and(Node::is_break) {
        return Some(Break::Before);
    }
    level
        .children
        .get(op + 1)
        .filter(|n| n.is_break())
        .map(|_| Break::After)
}

fn prefix_shaped(level: &Branch<'_>, op: usize) -> bool {
    op > 0
        && level.children[op - 1].is_text()
        && !level.children.get(op + 1).is_some_and(Node::is_text)
}

fn line_start(ctx: &Ctx<'_>, brk: Option<Break>, op: &Node<'_>, right: &Node<'_>) -> Option<usize> {
    match brk? {
        Break::Before => Some(ctx.lines.pos(op.start()).1),
        Break::After => Some(ctx.lines.pos(right.start()).1),
    }
}

fn plan_chain<'n, 'a>(node: &'n Branch<'a>, ctx: &Ctx<'a>) -> Option<Chain<'n, 'a>> {
    if node.kind != "bin_exp" {
        return None;
    }
    let mut chain = Chain {
        operands: vec![],
        ops: vec![],
        breaks: vec![],
        columns: vec![],
    };
    let mut current = node;
    loop {
        let [(_, left), (op_at, op_node), (_, right)] = three_parts(current)?;
        let op = op_text(op_node)?;
        if !chainable(op) || (PREFIX_SHAPED.contains(&op) && prefix_shaped(current, op_at)) {
            return None;
        }
        if right.as_branch().is_some_and(|b| b.kind == "bin_exp") {
            return None;
        }
        // The tree nests to the left, so the chain is collected from its end and reversed.
        let brk = source_break(current, op_at);
        chain.ops.push(op);
        chain.operands.push(right);
        chain.breaks.push(brk);
        chain.columns.push(line_start(ctx, brk, op_node, right));
        match left.as_branch() {
            Some(b) if b.kind == "bin_exp" => current = b,
            _ => {
                chain.operands.push(left);
                chain.ops.reverse();
                chain.operands.reverse();
                chain.breaks.reverse();
                chain.columns.reverse();
                return Some(chain);
            }
        }
    }
}

fn plan_coalesce<'n, 'a>(node: &'n Branch<'a>, ctx: &Ctx<'a>) -> Option<Chain<'n, 'a>> {
    if node.kind != "coalesce_exp" {
        return None;
    }
    let mut chain = Chain {
        operands: vec![],
        ops: vec![],
        breaks: vec![],
        columns: vec![],
    };
    let mut current = node;
    loop {
        let [(_, left), (op_at, op_node), (_, right)] = three_parts(current)?;
        if op_node.token_text().map(str::trim) != Some("??") {
            return None;
        }
        let brk = source_break(current, op_at);
        chain.operands.push(left);
        chain.ops.push("??");
        chain.breaks.push(brk);
        chain.columns.push(line_start(ctx, brk, op_node, right));
        match right.as_branch() {
            Some(b) if b.kind == "coalesce_exp" => current = b,
            _ => {
                chain.operands.push(right);
                return Some(chain);
            }
        }
    }
}

/// A binary or `??` chain that breaks only where the source did, or `None` to leave it to the source-gap fallback.
pub fn binary_chain_doc<'a>(node: &Branch<'a>, ctx: &Ctx<'a>) -> Option<Doc<'a>> {
    let chain = plan_coalesce(node, ctx).or_else(|| plan_chain(node, ctx))?;
    let first = match chain.operands[0] {
        Node::Text(_) => None,
        n => Some(ctx.lines.pos(n.start()).1),
    };
    let aligned = chain
        .breaks
        .iter()
        .zip(&chain.columns)
        .all(|(b, c)| b.is_none() || *c == first);
    let mut rest = Vec::new();
    for (i, op) in chain.ops.iter().enumerate() {
        let operand = node_doc(chain.operands[i + 1], ctx);
        rest.extend(match chain.breaks[i] {
            Some(Break::Before) => [HardLine, text(*op), text(" "), operand],
            Some(Break::After) => [text(" "), text(*op), HardLine, operand],
            None => [text(" "), text(*op), text(" "), operand],
        });
    }
    let rest = concat(rest);
    Some(concat([
        node_doc(chain.operands[0], ctx),
        if aligned { rest } else { indent(rest) },
    ]))
}

#[cfg(test)]
mod tests {
    use super::chainable;

    #[test]
    fn a_chain_never_breaks_inside_or_beside_an_indivisible_operator() {
        for op in [":=", "**", "+%", "<<", ">>", "->", "#"] {
            assert!(!chainable(op), "{op}");
        }
        for op in ["|>", "+", ">=", "and"] {
            assert!(chainable(op), "{op}");
        }
    }
}
