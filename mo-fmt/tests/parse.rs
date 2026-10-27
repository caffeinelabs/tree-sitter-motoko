use mo_fmt::tree::{Branch, Mode, Node, parse};

fn leaves<'a>(n: &Node<'a>, out: &mut String) {
    match n {
        Node::Branch(b) => b.children.iter().for_each(|c| leaves(c, out)),
        _ => out.push_str(n.text()),
    }
}

fn round_trips(source: &str) {
    let root = parse(source).unwrap_or_else(|e| panic!("{e}"));
    let mut out = String::new();
    leaves(&root, &mut out);
    assert_eq!(out, source);
}

#[test]
fn accepted_input_round_trips() {
    for source in [
        "",
        "let x = 1;\n",
        "\n\n\nactor {};\n",
        "let x = 1;",
        "let x = if c { 1 } else { 2 };\n",
        "let a = f(x);\n",
        "let x = 1; // hi\nlet y = 2;\n",
        "/* hi */ let x = 1;\n",
        "/* a /* b */ c */ let x = 1;\n",
        "let z = f /*c*/ (x);\n",
        "let o = { a = 1; b = 2; };\n",
        "actor { public func f() : async () {} };\n",
        "switch (x) { case (1) 2; case (_) 3 };\n",
        "module { public let x = 1; };\n",
        "func f() { func g() { 1 } };\n",
        "let x = \"é你好\"; // é你\n",
        "let s = \"😀🎉\";\n",
        "let s = \"a\\nb\\t\\\"c\";\n",
        "actor { a(); };\n",
        "let r = { a = 1; b = 2; };\n",
    ] {
        round_trips(source);
    }
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/parse")).unwrap() {
        round_trips(&std::fs::read_to_string(entry.unwrap().path()).unwrap());
    }
}

#[test]
fn broken_input_never_panics() {
    // Cut short or commented out from any point, the samples exercise error recovery's odd tree shapes.
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/parse")).unwrap() {
        let source = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        for (at, _) in source.char_indices() {
            let _ = parse(&source[..at]);
            let _ = parse(&format!("{}/* {}", &source[..at], &source[at..]));
        }
    }
}

#[test]
fn rejected_input_is_located() {
    for (source, message, line) in [
        ("let x = { a = 1;\n", "", 1),
        ("let x = @@@ ;\n", "unexpected input", 1),
        ("let x = 1;;\n", "unexpected input", 1),
        // A zero-width recovery node, reported rather than blamed on the normaliser.
        ("import I \"x\";\ninclude I;\n", "unexpected input", 2),
        (
            "let a = 1;\nlet b = 2;\nlet c = @@@ ;\n",
            "unexpected input",
            3,
        ),
    ] {
        let e = parse(source).expect_err(source);
        assert!(e.message.contains(message), "{source:?}: {}", e.message);
        assert_eq!(e.line, line, "{source:?}");
    }
    let e = parse("let x = @@@ ;\n").unwrap_err();
    assert_eq!((e.column, e.frame.contains("let x = @@@ ;")), (9, true));
    // The caret keeps a tab as a tab, so it lines up under the error.
    let e = parse("\tlet x = @@@ ;\n").unwrap_err();
    assert!(e.frame.ends_with("| \t        ^"), "{:?}", e.frame);
}

#[test]
fn an_unclosed_block_comment_is_located_at_its_opening() {
    // Recovery can drop the comment's text from the tree, which once panicked the normaliser.
    for (source, line, column) in [
        ("/* never closed\n", 1, 1),
        ("let y = { /* unclosed\n};\n", 1, 11),
        ("/* a /* b */ c\n", 1, 1),
        ("let x = 1; /* a */\nlet y = { /* b\n", 2, 11),
        ("let x = f(/* a\n", 1, 11),
        // A stray `*/` before it closes nothing.
        ("*/ /* a\n", 1, 4),
        ("/* a\n*/ */ /* b", 2, 7),
        // A truncated test file, shrunk: recovery ends the cut-short declaration's `ERROR` just before the comment.
        (
            "type C<A<:B>=A->B;let f=(func():t=0):C<>;func<>():C=func</**/>C=x;\nfunc<C,/*<",
            2,
            8,
        ),
    ] {
        let e = parse(source).expect_err(source);
        assert_eq!(
            (e.message.as_str(), e.line, e.column),
            ("unclosed block comment", line, column),
            "{source:?}"
        );
    }
    // An earlier error is reported first, even when recovery from it opens a comment inside a later string.
    for source in [
        "let x = @@@;\n/* c\n",
        "let a = \"bad \\q escape\";\nlet b = \"src/*.mo\";\n",
    ] {
        let e = parse(source).expect_err(source);
        assert_eq!(
            (e.message.as_str(), e.line, e.column),
            ("unexpected input", 1, 9),
            "{source:?}"
        );
    }
}

fn mode_of(source: &str, kind: &str) -> Option<Option<Mode>> {
    let root = parse(source).unwrap();
    let branches = root.as_branch().unwrap().branches();
    branches
        .into_iter()
        .find(|b| b.kind == kind)
        .map(|b| b.mode)
}

fn head_mode(source: &str) -> Option<Mode> {
    let root = parse(source).unwrap();
    let branches = root.as_branch().unwrap().branches();
    let head: &Branch<'_> = branches
        .iter()
        .find(|b| b.kind == "if_exp")
        .unwrap()
        .field("condition")
        .unwrap();
    head.mode
}

#[test]
fn modes_tell_head_and_object_positions_apart() {
    assert_eq!(
        mode_of("let a = f(x);\n", "call_exp"),
        Some(Some(Mode::Object))
    );
    assert_eq!(mode_of("let a = not x;\n", "not_exp"), Some(None));
    // Head rules are aliased onto ordinary names, some without a mode suffix, like `not_exp`.
    for source in [
        "let a = if not x { 1 } else { 2 };\n",
        "let a = if f(x) { 1 } else { 2 };\n",
        "let a = if x.y { 1 } else { 2 };\n",
        "let a = if x[0] { 1 } else { 2 };\n",
        "let a = if x and y { 1 } else { 2 };\n",
        "let a = if x + y { 1 } else { 2 };\n",
    ] {
        assert_eq!(head_mode(source), Some(Mode::Block), "{source:?}");
    }
}
