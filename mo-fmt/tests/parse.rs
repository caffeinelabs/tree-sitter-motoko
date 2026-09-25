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
fn rejected_input_is_located() {
    for (source, message, line) in [
        ("let x = { a = 1;\n", "", 1),
        ("let x = @@@ ;\n", "unexpected input", 1),
        ("/* never closed\n", "missing `*/`", 2),
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
