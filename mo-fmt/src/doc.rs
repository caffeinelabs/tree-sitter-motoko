//! A small layout IR in the style of Prettier's, without its width fitting: the formatter follows the
//! author's line breaks, so a group breaks only when told to or when it contains a forced break.

use std::borrow::Cow;

#[derive(Debug, Clone)]
pub enum Doc<'a> {
    Text(Cow<'a, str>),
    Concat(Vec<Doc<'a>>),
    Indent(Box<Doc<'a>>),
    Align(usize, Box<Doc<'a>>),
    Group {
        broken: bool,
        contents: Box<Doc<'a>>,
    },
    /// A space, or a line break in a broken group.
    Line,
    /// Nothing, or a line break in a broken group.
    SoftLine,
    /// A line break that also breaks every enclosing group.
    HardLine,
    /// A line break that keeps the text around it exactly and restarts at column 0.
    LiteralLine,
    /// Breaks every enclosing group.
    BreakParent,
}

pub use Doc::{BreakParent, HardLine, Line, SoftLine};

pub const EMPTY: Doc<'static> = Doc::Text(Cow::Borrowed(""));

pub fn text<'a>(s: impl Into<Cow<'a, str>>) -> Doc<'a> {
    Doc::Text(s.into())
}

pub fn concat<'a>(parts: impl IntoIterator<Item = Doc<'a>>) -> Doc<'a> {
    Doc::Concat(parts.into_iter().collect())
}

pub fn indent(doc: Doc<'_>) -> Doc<'_> {
    Doc::Indent(Box::new(doc))
}

pub fn align(n: usize, doc: Doc<'_>) -> Doc<'_> {
    Doc::Align(n, Box::new(doc))
}

pub fn group(doc: Doc<'_>, broken: bool) -> Doc<'_> {
    Doc::Group {
        broken,
        contents: Box::new(doc),
    }
}

/// Text that may span lines: each line break becomes a literal one.
pub fn verbatim(s: &str) -> Doc<'_> {
    if !s.contains('\n') {
        return text(s);
    }
    let mut parts = Vec::new();
    for (i, line) in s.split('\n').enumerate() {
        if i > 0 {
            parts.push(Doc::LiteralLine);
        }
        parts.push(text(line));
    }
    Doc::Concat(parts)
}

impl Doc<'_> {
    /// Whether the doc contains a forced break or a group told to break.
    pub fn will_break(&self) -> bool {
        match self {
            Doc::HardLine | Doc::LiteralLine | Doc::BreakParent => true,
            Doc::Group { broken, contents } => *broken || contents.will_break(),
            Doc::Concat(parts) => parts.iter().any(Doc::will_break),
            Doc::Indent(d) | Doc::Align(_, d) => d.will_break(),
            Doc::Text(_) | Doc::Line | Doc::SoftLine => false,
        }
    }

    /// Marks every group that contains a forced break as broken, and reports whether this doc does.
    fn propagate_breaks(&mut self) -> bool {
        match self {
            Doc::HardLine | Doc::LiteralLine | Doc::BreakParent => true,
            Doc::Group { broken, contents } => {
                let inner = contents.propagate_breaks();
                *broken |= inner;
                *broken
            }
            Doc::Concat(parts) => parts
                .iter_mut()
                .fold(false, |any, p| p.propagate_breaks() | any),
            Doc::Indent(d) | Doc::Align(_, d) => d.propagate_breaks(),
            Doc::Text(_) | Doc::Line | Doc::SoftLine => false,
        }
    }
}

pub fn print(mut doc: Doc<'_>, indent_width: usize) -> String {
    doc.propagate_breaks();
    let mut out = String::new();
    let mut stack: Vec<(usize, bool, &Doc<'_>)> = vec![(0, true, &doc)];
    while let Some((ind, broken, d)) = stack.pop() {
        match d {
            Doc::Text(s) => out.push_str(s),
            Doc::Concat(parts) => stack.extend(parts.iter().rev().map(|p| (ind, broken, p))),
            Doc::Indent(inner) => stack.push((ind + indent_width, broken, inner)),
            Doc::Align(n, inner) => stack.push((ind + n, broken, inner)),
            Doc::Group { broken, contents } => stack.push((ind, *broken, contents)),
            Doc::Line if !broken => out.push(' '),
            Doc::SoftLine if !broken => {}
            Doc::Line | Doc::SoftLine | Doc::HardLine => {
                out.truncate(out.trim_end_matches([' ', '\t']).len());
                out.push('\n');
                out.extend(std::iter::repeat_n(' ', ind));
            }
            Doc::LiteralLine => out.push('\n'),
            Doc::BreakParent => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_group_breaks_only_when_told_or_when_it_holds_a_hard_break() {
        let list = |broken, tail: Doc<'static>| {
            group(
                concat([
                    text("["),
                    indent(concat([SoftLine, text("a,"), Line, text("b"), tail])),
                    SoftLine,
                    text("]"),
                ]),
                broken,
            )
        };
        assert_eq!(print(list(false, EMPTY), 2), "[a, b]");
        assert_eq!(print(list(true, EMPTY), 2), "[\n  a,\n  b\n]");
        assert_eq!(
            print(list(false, concat([text(" // c"), BreakParent])), 2),
            "[\n  a,\n  b // c\n]"
        );
    }

    #[test]
    fn hard_breaks_trim_trailing_spaces_and_literal_breaks_keep_them() {
        let doc = concat([
            text("a  "),
            HardLine,
            align(3, concat([text("b"), HardLine, verbatim("/* x  \n  y */")])),
        ]);
        assert_eq!(print(doc, 2), "a\nb\n   /* x  \n  y */");
    }
}
