//! The spacing between two siblings on one line.
//!
//! moc reads `(`, `[`, `<`, `>`, `+`, `-`, `^`, `#` and `??` differently with whitespace beside them, which the guard can't see.
//! So a space is only added where they read the same either way, as after a keyword or `=`,
//! and only removed before a `,` and after an anonymous `func`.

use crate::doc::{Doc, EMPTY, text};
use crate::tree::{Branch, Node};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Join {
    /// As written, with a run of spaces made one.
    Keep,
    Space,
    Glue,
}

/// `gap` is the whitespace between them, which has no line break.
pub fn join_doc(join: Join, gap: Option<&str>) -> Doc<'static> {
    match (join, gap) {
        (Join::Glue, _) | (Join::Keep, None) => EMPTY,
        (Join::Space, _) | (Join::Keep, Some(_)) => text(" "),
    }
}

pub fn join(parent: &Branch<'_>, left: &Node<'_>, right: &Node<'_>) -> Join {
    // moc reads a `>` with whitespace on both sides as greater-than.
    if ends_with_spaced_gt(left) {
        return Join::Keep;
    }
    // The `,` of a list that isn't a node of its own, such as a type's arguments.
    if right.is_token(",")
        || (parent.kind == "func_exp" && left.is_token("func") && starts_with(right, &['(', '<']))
    {
        return Join::Glue;
    }
    let space = ((is_keyword(left) || left.is_token(",")) && !starts_with(right, CLOSERS))
        || (is_keyword(right) && ends_word(left))
        || is_spaced_op(left)
        || is_spaced_op(right)
        || matches!(kind(right), Some("typ_annot"))
        || ((matches!(kind(right), Some("block_exp" | "obj_body")) || right.is_token("{"))
            && (ends_word(left) || parent.kind == "do_quest_exp"))
        // A control head and its body, as in `case (x) y`.
        || (left.text().ends_with(')') && starts_word(right));
    if space { Join::Space } else { Join::Keep }
}

/// What a keyword or a `,` stays glued to, as in `break;`, `(break)` and `Map<K,>`.
const CLOSERS: &[char] = &[')', ']', '}', ';', ',', '.', '<', '>', '!'];

fn kind<'a>(n: &Node<'a>) -> Option<&'static str> {
    n.as_branch().map(|b| b.kind)
}

/// `if`, `else`, `async*`: an anonymous token spelled as a word. Literals like `true` are named.
fn is_keyword(n: &Node<'_>) -> bool {
    let Node::Token(t) = n else { return false };
    let word = t.text.strip_suffix(['*', '?']).unwrap_or(t.text);
    !t.named
        && word.starts_with(|c: char| c.is_ascii_lowercase())
        && word.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Binds or annotates, as in `x = 1`, `x : T`, `T -> U`, `T <: U`, `x := 1` and `x += 1`.
fn is_spaced_op(n: &Node<'_>) -> bool {
    match n {
        Node::Token(t) => !t.named && matches!(t.text, "=" | ":" | "->" | "<:" | ":="),
        Node::Branch(b) => b.kind == "binassign_op",
        Node::Text(_) => false,
    }
}

fn starts_with(n: &Node<'_>, chars: &[char]) -> bool {
    n.text().starts_with(chars)
}

/// Ends with a name, a literal or a closing bracket, after which a space changes nothing moc reads.
fn ends_word(n: &Node<'_>) -> bool {
    n.text().ends_with(|c: char| {
        c.is_alphanumeric() || matches!(c, '_' | ')' | ']' | '}' | '>' | '"' | '\'')
    })
}

fn starts_word(n: &Node<'_>) -> bool {
    n.text()
        .starts_with(|c: char| c.is_alphanumeric() || matches!(c, '_' | '"' | '\''))
}

fn ends_with_spaced_gt(n: &Node<'_>) -> bool {
    n.text()
        .strip_suffix('>')
        .is_some_and(|rest| rest.ends_with(char::is_whitespace))
}
