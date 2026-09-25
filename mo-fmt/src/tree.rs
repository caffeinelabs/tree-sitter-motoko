//! The normalised tree: every byte of the source belongs to exactly one leaf, so printing the leaves in order reproduces it.

use std::fmt;

mod head_symbols {
    include!(concat!(env!("OUT_DIR"), "/head_symbols.rs"));
}
use head_symbols::HEAD_SYMBOL_IDS;

#[derive(Debug)]
pub enum Node<'a> {
    /// Whitespace between two siblings.
    Text(Gap<'a>),
    Token(Token<'a>),
    Branch(Branch<'a>),
}

#[derive(Debug)]
pub struct Gap<'a> {
    pub start: usize,
    pub text: &'a str,
}

#[derive(Debug)]
pub struct Token<'a> {
    pub start: usize,
    pub kind: &'static str,
    pub named: bool,
    pub extra: bool,
    pub error: bool,
    pub missing: bool,
    pub text: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Block,
    Object,
}

#[derive(Debug)]
pub struct Branch<'a> {
    pub start: usize,
    pub end: usize,
    /// The node type as the grammar names it, e.g. `call_exp_block`.
    pub ty: &'static str,
    /// The type without its mode suffix, e.g. `call_exp`.
    pub kind: &'static str,
    pub mode: Option<Mode>,
    pub named: bool,
    pub extra: bool,
    pub field: Option<&'static str>,
    pub children: Vec<Node<'a>>,
    pub error: bool,
    pub missing: bool,
    pub has_error: bool,
    pub text: &'a str,
}

impl<'a> Node<'a> {
    pub fn text(&self) -> &'a str {
        match self {
            Node::Text(g) => g.text,
            Node::Token(t) => t.text,
            Node::Branch(b) => b.text,
        }
    }

    pub fn start(&self) -> usize {
        match self {
            Node::Text(g) => g.start,
            Node::Token(t) => t.start,
            Node::Branch(b) => b.start,
        }
    }

    pub fn end(&self) -> usize {
        self.start() + self.text().len()
    }

    pub fn is_text(&self) -> bool {
        matches!(self, Node::Text(_))
    }

    pub fn as_branch(&self) -> Option<&Branch<'a>> {
        match self {
            Node::Branch(b) => Some(b),
            _ => None,
        }
    }

    /// The token's text, or `None` for a branch or a gap.
    pub fn token_text(&self) -> Option<&'a str> {
        match self {
            Node::Token(t) => Some(t.text),
            _ => None,
        }
    }

    /// Whether this is the token `text`.
    pub fn is_token(&self, text: &str) -> bool {
        self.token_text() == Some(text)
    }

    /// Whether this is a gap containing a line break.
    pub fn is_break(&self) -> bool {
        matches!(self, Node::Text(g) if g.text.contains('\n'))
    }

    /// The grammar type of a token or branch.
    pub fn ty(&self) -> Option<&'static str> {
        match self {
            Node::Text(_) => None,
            Node::Token(t) => Some(t.kind),
            Node::Branch(b) => Some(b.ty),
        }
    }
}

impl<'a> Branch<'a> {
    /// The first child branch with this field name.
    pub fn field(&self, name: &str) -> Option<&Branch<'a>> {
        self.children.iter().find_map(|c| match c {
            Node::Branch(b) if b.field == Some(name) => Some(b),
            _ => None,
        })
    }

    /// The children other than gaps.
    pub fn nodes(&self) -> impl Iterator<Item = &Node<'a>> {
        self.children.iter().filter(|c| !c.is_text())
    }

    /// Every branch in this subtree, preorder, including this one.
    pub fn branches(&self) -> Vec<&Branch<'a>> {
        let mut out = vec![self];
        let mut i = 0;
        while i < out.len() {
            let b = out[i];
            out.extend(b.children.iter().filter_map(Node::as_branch));
            i += 1;
        }
        out
    }
}

/// Maps byte offsets to lines and character columns.
pub struct Lines<'a> {
    source: &'a str,
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    pub fn new(source: &'a str) -> Self {
        let starts = std::iter::once(0)
            .chain(source.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        Lines { source, starts }
    }

    /// Zero-based row.
    pub fn row(&self, offset: usize) -> usize {
        self.starts.partition_point(|&s| s <= offset) - 1
    }

    /// Zero-based row and character column.
    pub fn pos(&self, offset: usize) -> (usize, usize) {
        let row = self.row(offset);
        let column = self.source[self.starts[row]..offset].chars().count();
        (row, column)
    }

    pub fn line(&self, row: usize) -> &'a str {
        let start = self.starts[row];
        let end = self
            .starts
            .get(row + 1)
            .map_or(self.source.len(), |&s| s - 1);
        &self.source[start..end]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub message: String,
    /// One-based.
    pub line: usize,
    /// One-based, in characters.
    pub column: usize,
    pub frame: String,
}

impl std::error::Error for SyntaxError {}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: {}\n{}",
            self.line, self.column, self.message, self.frame
        )
    }
}

fn language() -> tree_sitter::Language {
    tree_sitter_motoko::LANGUAGE.into()
}

pub fn parse(source: &str) -> Result<Node<'_>, SyntaxError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language())
        .expect("the grammar's ABI matches tree-sitter");
    let tree = parser
        .parse(source, None)
        .expect("parsing is never cancelled");
    let ts_root = tree.root_node();

    let mut root = Normalizer { source }.root(ts_root);
    hoist_trailing_whitespace(&mut root, source);
    let root = Node::Branch(root);
    if ts_root.has_error() {
        let lines = Lines::new(source);
        return Err(match find_problem(&root, None) {
            Some(problem) => syntax_error(&lines, problem),
            None => SyntaxError {
                message: "the parser reported an error but no offending node was found; this is a bug in mo-fmt".into(),
                line: 1,
                column: 1,
                frame: String::new(),
            },
        });
    }
    Ok(root)
}

struct Normalizer<'a> {
    source: &'a str,
}

impl<'a> Normalizer<'a> {
    fn root(&self, node: tree_sitter::Node) -> Branch<'a> {
        let children = self.children(node, 0, self.source.len());
        self.finish(node, None, children, 0, self.source.len())
    }

    fn children(&self, node: tree_sitter::Node, from: usize, to: usize) -> Vec<Node<'a>> {
        let mut out = Vec::new();
        let mut at = from;
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                self.gap(&mut out, at, child.start_byte());
                out.push(self.child(child, cursor.field_name()));
                at = child.end_byte();
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        self.gap(&mut out, at, to);
        out
    }

    fn gap(&self, out: &mut Vec<Node<'a>>, start: usize, end: usize) {
        if end <= start {
            return;
        }
        let text = &self.source[start..end];
        assert!(
            text.trim().is_empty(),
            "normalize: non-whitespace between nodes at {start}..{end}: {:?}",
            &text[..text.len().min(40)]
        );
        out.push(Node::Text(Gap { start, text }));
    }

    fn child(&self, node: tree_sitter::Node, field: Option<&'static str>) -> Node<'a> {
        // `comment_text` is lexed one character per hidden token, so only the source slice gives its text.
        if node.child_count() == 0 || node.kind() == "comment_text" {
            return Node::Token(Token {
                start: node.start_byte(),
                kind: node.kind(),
                named: node.is_named(),
                extra: node.is_extra(),
                error: node.is_error(),
                missing: node.is_missing(),
                text: &self.source[node.start_byte()..node.end_byte()],
            });
        }
        let children = self.children(node, node.start_byte(), node.end_byte());
        Node::Branch(self.finish(node, field, children, node.start_byte(), node.end_byte()))
    }

    fn finish(
        &self,
        node: tree_sitter::Node,
        field: Option<&'static str>,
        children: Vec<Node<'a>>,
        start: usize,
        end: usize,
    ) -> Branch<'a> {
        let ty = node.kind();
        let (kind, suffix_mode) = strip_mode(ty);
        let mode = if HEAD_SYMBOL_IDS.binary_search(&node.grammar_id()).is_ok() {
            Some(Mode::Block)
        } else {
            suffix_mode
        };
        Branch {
            start,
            end,
            ty,
            kind,
            mode,
            named: node.is_named(),
            extra: node.is_extra(),
            field,
            children,
            error: node.is_error(),
            missing: node.is_missing(),
            has_error: node.has_error(),
            text: &self.source[start..end],
        }
    }
}

/// Every `_block`/`_object` type is one of a pair of mode variants of the same construct.
fn strip_mode(ty: &'static str) -> (&'static str, Option<Mode>) {
    for (suffix, mode) in [("_block", Mode::Block), ("_object", Mode::Object)] {
        if let Some(kind) = ty.strip_suffix(suffix) {
            return (kind, Some(mode));
        }
    }
    (ty, None)
}

/// `<` and `??` absorb the whitespace after them into the token, where the grammar uses it to decide what they mean.
/// Moves it out of the token, and out of every branch ending with it, into the gap that follows.
fn hoist_trailing_whitespace<'a>(node: &mut Branch<'a>, source: &'a str) {
    let mut i = 0;
    while i < node.children.len() {
        let moved = match &mut node.children[i] {
            Node::Text(_) => None,
            Node::Branch(child) => {
                hoist_trailing_whitespace(child, source);
                match child.children.pop_if(|c| c.is_text()) {
                    Some(Node::Text(last)) => {
                        child.end = last.start;
                        child.text = &child.text[..child.text.len() - last.text.len()];
                        Some(last)
                    }
                    _ => None,
                }
            }
            Node::Token(t) => {
                let trimmed = t.text.trim_end();
                if t.extra || trimmed.len() == t.text.len() {
                    None
                } else {
                    let gap = Gap {
                        start: t.start + trimmed.len(),
                        text: &t.text[trimmed.len()..],
                    };
                    t.text = trimmed;
                    Some(gap)
                }
            }
        };
        if let Some(moved) = moved {
            match node.children.get_mut(i + 1) {
                Some(Node::Text(next)) => {
                    next.text = &source[moved.start..next.start + next.text.len()];
                    next.start = moved.start;
                }
                _ => node.children.insert(i + 1, Node::Text(moved)),
            }
        }
        i += 1;
    }
}

struct Problem<'n, 'a> {
    missing: bool,
    ty: &'static str,
    node: &'n Node<'a>,
}

/// tree-sitter nests the bad token's `ERROR` inside one spanning the failed construct, so the innermost is reported.
fn find_problem<'n, 'a>(
    n: &'n Node<'a>,
    enclosing: Option<&'n Node<'a>>,
) -> Option<Problem<'n, 'a>> {
    let (error, missing, ty) = match n {
        Node::Text(_) => return None,
        Node::Token(t) => (t.error, t.missing, t.kind),
        Node::Branch(b) => (b.error, b.missing, b.ty),
    };
    if error || missing {
        if let Node::Branch(b) = n
            && let Some(inner) = b.children.iter().find_map(|c| find_problem(c, Some(n)))
        {
            return Some(inner);
        }
        return Some(Problem {
            missing,
            ty,
            node: n,
        });
    }
    let Node::Branch(b) = n else { return None };
    let flagged = if b.has_error { Some(n) } else { enclosing };
    // A zero-width recovery site with no flagged node below, e.g. `include I;` gets an empty `float_literal`.
    if b.start == b.end
        && let Some(f) = flagged
    {
        return Some(Problem {
            missing: false,
            ty: f.ty().unwrap_or(""),
            node: f,
        });
    }
    b.children.iter().find_map(|c| find_problem(c, flagged))
}

fn syntax_error(lines: &Lines<'_>, problem: Problem<'_, '_>) -> SyntaxError {
    let (row, column) = lines.pos(problem.node.start());
    let message = if problem.missing {
        match problem.ty {
            "}" => "missing closing brace".to_string(),
            ty => format!("missing `{ty}`"),
        }
    } else {
        "unexpected input".to_string()
    };
    let gutter = (row + 1).to_string().len();
    let line = lines.line(row);
    // Tabs stay tabs, so the caret lines up however wide the terminal draws them.
    let pad: String = line
        .chars()
        .take(column)
        .map(|c| if c == '\t' { '\t' } else { ' ' })
        .collect();
    let frame = format!(
        "{:gutter$} |\n{} | {line}\n{:gutter$} | {pad}^",
        "",
        row + 1,
        ""
    );
    SyntaxError {
        message,
        line: row + 1,
        column: column + 1,
        frame,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaves<'a>(n: &Node<'a>, out: &mut Vec<(usize, &'a str)>) {
        match n {
            Node::Branch(b) => b.children.iter().for_each(|c| leaves(c, out)),
            _ => out.push((n.start(), n.text())),
        }
    }

    #[test]
    fn leaves_reproduce_the_source_in_order() {
        let source = "let x = a <\n b ?? c;\n/* c */ f<T>(x)\n";
        let root = parse(source).unwrap();
        let mut out = Vec::new();
        leaves(&root, &mut out);
        let mut at = 0;
        for (start, text) in out {
            assert_eq!(start, at);
            at += text.len();
        }
        assert_eq!(at, source.len());
    }

    #[test]
    fn whitespace_absorbed_by_a_token_moves_to_the_gap() {
        let root = parse("let x = a ?? b").unwrap();
        let coalesce = root
            .as_branch()
            .unwrap()
            .branches()
            .into_iter()
            .find(|b| b.kind == "coalesce_exp")
            .unwrap();
        let op = coalesce.children.iter().find(|c| c.is_token("??"));
        assert!(op.is_some(), "the `??` token is trimmed");
    }

    #[test]
    fn head_rules_report_block_mode() {
        let root = parse("if f(x) {};").unwrap();
        let call = root
            .as_branch()
            .unwrap()
            .branches()
            .into_iter()
            .find(|b| b.kind == "call_exp")
            .unwrap();
        assert_eq!(call.mode, Some(Mode::Block));
    }

    #[test]
    fn a_syntax_error_is_located_at_its_innermost_node() {
        let err = parse("let x = ;\n").unwrap_err();
        assert_eq!(
            (err.line, err.column, err.message.as_str()),
            (1, 7, "unexpected input")
        );
        assert_eq!(err.frame, "  |\n1 | let x = ;\n  |       ^");
    }
}
