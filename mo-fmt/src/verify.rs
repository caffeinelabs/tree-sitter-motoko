//! The runtime guard: the printed code must parse to the same tree as the input, up to layout.

use crate::tree::{Lines, Node, parse};

fn describe(n: &Node<'_>) -> String {
    match n {
        Node::Text(_) => "<gap>".into(),
        Node::Token(t) => format!("{}{}:{}", if t.named { "" } else { "~" }, t.kind, t.text),
        Node::Branch(b) => b.kind.to_string(),
    }
}

// A hard line break trims trailing whitespace, so a line comment can lose its trailing spaces.
fn same_token(a: &Node<'_>, b: &Node<'_>) -> bool {
    let (Node::Token(x), Node::Token(y)) = (a, b) else {
        return false;
    };
    if (x.kind, x.named) != (y.kind, y.named) {
        return false;
    }
    let comment = matches!(x.kind, "line_comment" | "doc_comment");
    x.text == y.text || (comment && x.text.trim_end() == y.text.trim_end())
}

fn compare<'x>(
    input: &'x Node<'_>,
    output: &'x Node<'_>,
) -> Option<(&'x Node<'x>, String, String)> {
    match (input, output) {
        (Node::Token(_), Node::Token(_)) => {
            (!same_token(input, output)).then(|| (input, describe(input), describe(output)))
        }
        (Node::Branch(a), Node::Branch(b)) => {
            if (a.kind, a.mode) != (b.kind, b.mode) {
                return Some((input, describe(input), describe(output)));
            }
            let (xs, ys): (Vec<_>, Vec<_>) = (a.nodes().collect(), b.nodes().collect());
            if xs.len() != ys.len() {
                return Some((
                    input,
                    format!("{} with {} children", a.kind, xs.len()),
                    format!("{} with {} children", b.kind, ys.len()),
                ));
            }
            xs.into_iter().zip(ys).find_map(|(x, y)| compare(x, y))
        }
        _ => Some((input, describe(input), describe(output))),
    }
}

/// `Err` with the first difference, or with the output's own syntax error.
pub fn verify(input: &Node<'_>, source: &str, printed: &str) -> Result<(), String> {
    let output = parse(printed).map_err(|e| format!("the formatted code does not parse ({e})"))?;
    match compare(input, &output) {
        None => Ok(()),
        Some((at, input, output)) => {
            let line = Lines::new(source).pos(at.start()).0 + 1;
            Err(format!(
                "the formatted code means something else, from line {line}: `{input}` became `{output}`"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::verify;
    use crate::tree::parse;

    fn check(input: &str, output: &str) -> Result<(), String> {
        verify(&parse(input).unwrap(), input, output)
    }

    #[test]
    fn accepts_layout_changes_and_trimmed_line_comments() {
        assert_eq!(
            check("let a = 1;\nlet b = 2;\n", "let a = 1; let b = 2;"),
            Ok(())
        );
        assert_eq!(
            check("/// doc \nlet x = 1\n", "/// doc\nlet x = 1\n"),
            Ok(())
        );
        // The guard can't see these spacings, which is why the printer keeps them as written.
        for (input, output) in [
            ("let x : ??Nat = null", "let x : ? ?Nat = null"),
            ("let v = #ok(1)", "let v = # ok(1)"),
            ("let x = a ?? b", "let x = a ??  b"),
        ] {
            assert_eq!(check(input, output), Ok(()), "{input:?}");
        }
    }

    #[test]
    fn rejects_changes_in_meaning() {
        for (input, output) in [
            ("let a = 1;\nlet b = 2;\n", "let a = 1;\nlet b = 3;\n"),
            (
                "let f = func (x : Nat) { }; f (1);\n",
                "let f = func (x : Nat) { }; f 1;\n",
            ),
            ("func f() : Nat = 1;\n", "func f() : Nat { 1 };\n"),
            ("let r = { a = 1 }\n", "let r = { a = 1; }\n"),
            ("if (g(x)) { 1 } else { 2 }", "if g(x) { 1 } else { 2 }"),
            ("let x = 1;\n", "let x = ;\n"),
        ] {
            assert!(check(input, output).is_err(), "{input:?} -> {output:?}");
        }
    }
}
