//! The Motoko code blocks of a Markdown file.

use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};

use crate::tree::{Lines, Node, SyntaxError};
use crate::{Config, Error, format_here, normalize, on_large_stack, rewrite, tree, verify};

/// A Markdown file with its Motoko code blocks formatted.
#[derive(Debug)]
pub struct Markdown {
    /// The file with each block that formatted replaced, and everything else as written. It keeps CRLF line
    /// endings if its first line has one, and the rest become LF.
    pub formatted: String,
    /// One for each block that didn't format, located in the Markdown file. The block is left as written.
    pub errors: Vec<Error>,
    /// The blocks left as written without trying to format them.
    pub skipped: Vec<Skipped>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// One-based line of the block's opening fence.
    pub line: usize,
    pub reason: &'static str,
}

/// Formats the code blocks tagged `motoko` or `mo` in a Markdown file. A block that doesn't parse is an error,
/// unless an HTML comment `<!-- mo-fmt-ignore -->` or `<!-- prettier-ignore -->` comes before it.
///
/// `Err` is a bug in mo-fmt, and then nothing was formatted.
pub fn format_markdown(source: &str, config: &Config) -> Result<Markdown, Error> {
    on_large_stack(|| {
        let (bom, text) = normalize(source);
        let lines = Lines::new(&text);
        let mut formatted = String::from(bom);
        let mut errors = Vec::new();
        let mut skipped = Vec::new();
        let mut copied = 0;
        for block in blocks(&text) {
            let fence_line = lines.row(block.fence) + 1;
            let Some(code) = block.code.filter(|_| !block.text.trim().is_empty()) else {
                continue;
            };
            if !block.closed {
                skipped.push(Skipped {
                    line: fence_line,
                    reason: "the code block isn't closed",
                });
                continue;
            }
            let start = lines.line_start(code.start);
            let Some(prefix) = prefix(&text[start..code.end], &block.text) else {
                skipped.push(Skipped {
                    line: fence_line,
                    reason: "the code block's lines aren't indented alike",
                });
                continue;
            };
            match format_code(&block.text, config) {
                Ok(code_formatted) => {
                    formatted.push_str(&text[copied..start]);
                    for line in code_formatted.split_inclusive('\n') {
                        if line == "\n" {
                            formatted.push_str(prefix.trim_end());
                        } else {
                            formatted.push_str(prefix);
                        }
                        formatted.push_str(line);
                    }
                    copied = code.end;
                }
                Err(Error::Syntax(e)) => {
                    let row = lines.row(start) + e.line - 1;
                    let line = lines.line(row);
                    let indent = if line.starts_with(prefix) {
                        prefix.chars().count()
                    } else {
                        0
                    };
                    errors.push(Error::Syntax(SyntaxError::new(
                        e.message,
                        row,
                        indent + e.column - 1,
                        line,
                    )));
                }
                Err(Error::Internal(message)) => errors.push(Error::Internal(format!(
                    "in the code block at line {fence_line}: {message}"
                ))),
            }
        }
        formatted.push_str(&text[copied..]);
        // Only a block that changed changes the file, so the rest of it keeps its line endings.
        let crlf = source
            .find('\n')
            .is_some_and(|i| source[..i].ends_with('\r'));
        if formatted[bom.len()..] == *text {
            formatted = source.to_string();
        } else if crlf {
            formatted = formatted.replace('\n', "\r\n");
        }
        Ok(Markdown {
            formatted,
            errors,
            skipped,
        })
    })
}

struct Block {
    /// Where its opening fence starts.
    fence: usize,
    /// The span from the start of its first line of code to the end of its last, if it has any.
    code: Option<Range<usize>>,
    /// The code, without the indentation of a list or the `>` of a blockquote.
    text: String,
    closed: bool,
}

/// The Motoko code blocks that aren't ignored.
fn blocks(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut current: Option<Block> = None;
    let mut html = String::new();
    let mut ignore = false;
    for (event, range) in Parser::new(text).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                let motoko = matches!(info.split_whitespace().next(), Some("motoko" | "mo"));
                if motoko && !ignore {
                    current = Some(Block {
                        fence: range.start,
                        code: None,
                        text: String::new(),
                        closed: false,
                    });
                }
                ignore = false;
            }
            Event::Text(code) => match &mut current {
                Some(block) => {
                    let start = block.code.as_ref().map_or(range.start, |c| c.start);
                    block.code = Some(start..range.end);
                    block.text.push_str(&code);
                }
                // The text of a tight list's item, which has no paragraph.
                None => ignore = false,
            },
            Event::End(TagEnd::CodeBlock) => {
                if let Some(mut block) = current.take() {
                    // An unclosed block runs to the end of the file, so its code ends where it does.
                    block.closed = block.code.as_ref().is_none_or(|c| c.end < range.end);
                    blocks.push(block);
                }
            }
            Event::Start(Tag::HtmlBlock) => html.clear(),
            Event::Html(s) => html.push_str(&s),
            Event::End(TagEnd::HtmlBlock) => ignore = is_ignore_comment(&html),
            // A comment before a list or blockquote reaches the block that opens it.
            Event::Start(Tag::List(_) | Tag::Item | Tag::BlockQuote(_)) => {}
            _ => ignore = false,
        }
    }
    blocks
}

fn is_ignore_comment(html: &str) -> bool {
    html.trim()
        .strip_prefix("<!--")
        .and_then(|s| s.strip_suffix("-->"))
        .is_some_and(|s| matches!(s.trim(), "mo-fmt-ignore" | "prettier-ignore"))
}

/// What each line of `code` follows in `written`, its lines as they are in the file: the indentation of a list
/// item, or a blockquote's `> `. `None` if that isn't the same on every line, as when a line is indented less
/// than the fence, since the formatted lines couldn't be written back as they were.
fn prefix<'a>(written: &'a str, code: &str) -> Option<&'a str> {
    let pairs = || written.split('\n').zip(code.split('\n'));
    if written.split('\n').count() != code.split('\n').count() {
        return None;
    }
    let prefix = pairs()
        .find(|(_, line)| !line.is_empty())
        .and_then(|(written, line)| written.strip_suffix(line))?;
    // A blank line can stop short of the prefix, or before a blockquote's space.
    pairs()
        .all(|(written, line)| {
            written.strip_prefix(prefix) == Some(line)
                || (line.is_empty() && prefix.starts_with(written))
        })
        .then_some(prefix)
}

/// Formats a code block, which can be a lone member of an actor, object or module, such as a `public func`.
fn format_code(code: &str, config: &Config) -> Result<String, Error> {
    let e = match format_here(code, config) {
        Err(Error::Syntax(e)) => e,
        result => return result,
    };
    match in_module(code, config) {
        Ok(Some(formatted)) => Ok(formatted),
        // An error further in, inside the code, is one in a member, as in `public func f() { let x = ; }`.
        Err(Error::Syntax(w)) if w.line > 1 && (w.line - 1, w.column) > (e.line, e.column) => {
            match code.lines().nth(w.line - 2) {
                Some(line) => Err(Error::Syntax(SyntaxError::new(
                    w.message,
                    w.line - 2,
                    w.column - 1,
                    line,
                ))),
                None => Err(Error::Syntax(e)),
            }
        }
        Ok(None) | Err(Error::Syntax(_)) => Err(Error::Syntax(e)),
        Err(internal) => Err(internal),
    }
}

const OPEN: &str = "module {\n";
const CLOSE: &str = "}\n";

/// Formats `code` as the body of a module, and then without one. `None` if it can't be taken out of the module
/// as it is.
fn in_module(code: &str, config: &Config) -> Result<Option<String>, Error> {
    let formatted = format_here(&format!("{OPEN}{code}{CLOSE}"), config)?;
    Ok(unwrap_module(&formatted, config))
}

fn unwrap_module(formatted: &str, config: &Config) -> Option<String> {
    let body = formatted.strip_prefix(OPEN)?.strip_suffix(CLOSE)?;
    let root = tree::parse(formatted).ok()?;
    let mut verbatim = Vec::new();
    multiline_tokens(&root, &mut verbatim);
    if let Node::Branch(b) = &root {
        verbatim.extend(
            rewrite::ignored(b)
                .into_iter()
                .map(|(start, end)| start..end),
        );
    }
    let indent = " ".repeat(config.indent_width.get());
    let mut start = OPEN.len();
    let mut dedented = String::new();
    for line in body.split_inclusive('\n') {
        // A line that continues a block comment, a text literal or an item kept as written is part of it, as written.
        if line == "\n"
            || verbatim
                .iter()
                .any(|t| t.contains(&start) && t.start != start)
        {
            dedented.push_str(line);
        } else {
            dedented.push_str(line.strip_prefix(&indent)?);
        }
        start += line.len();
    }
    verify::verify(&root, formatted, &format!("{OPEN}{dedented}{CLOSE}")).ok()?;
    Some(dedented)
}

fn multiline_tokens(node: &Node<'_>, out: &mut Vec<Range<usize>>) {
    match node {
        Node::Branch(b) => b.children.iter().for_each(|c| multiline_tokens(c, out)),
        Node::Token(t) if t.text.contains('\n') => out.push(node.start()..node.end()),
        _ => {}
    }
}
