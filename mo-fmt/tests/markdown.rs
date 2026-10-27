use mo_fmt::{Config, Error, Markdown, Skipped, format_markdown};

fn markdown(source: &str) -> Markdown {
    let config = Config::default();
    let out = format_markdown(source, &config).unwrap();
    let again = format_markdown(&out.formatted, &config).unwrap();
    assert_eq!(again.formatted, out.formatted, "not a fixed point");
    out
}

/// The formatted file, which must have had no errors and skipped nothing.
fn formatted(source: &str) -> String {
    let out = markdown(source);
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    assert!(out.skipped.is_empty(), "{:?}", out.skipped);
    out.formatted
}

#[test]
fn formats_motoko_and_mo_blocks_and_nothing_else() {
    let source = "\
# Title  with  spaces

```motoko no-repl title=\"a.mo\"
let x = f(1,2);
```

```mo
let x = f(1,2);
```

``` motoko
let x = f(1,2);
```

```mo:pkg/lib.mo
public func new() : State;
```

```
let x = f(1,2);
```

```ts
let x = f(1,2);
```

    let x = f(1,2);

Inline `let x = f(1,2);` code.
";
    let expected = source.replacen("f(1,2)", "f(1, 2)", 3);
    assert_eq!(formatted(source), expected);
}

#[test]
fn leaves_empty_blocks_filled_in_at_build_time() {
    let source = "```motoko file=<motokoExamples>/a.mo#L1-L9\n```\n\n```motoko\n\n```\n";
    assert_eq!(formatted(source), source);
}

#[test]
fn keeps_the_indentation_of_a_list_item_and_a_blockquote() {
    let source = "\
- An item:

    ```motoko
    actor {
       let x = f(1,2);

       public func g() {};
    };
    ```

> ```motoko
> if (c) {
>     a()
> } else { b() };
>
> x
> ```
";
    let expected = "\
- An item:

    ```motoko
    actor {
      let x = f(1, 2);

      public func g() {};
    };
    ```

> ```motoko
> if (c) {
>   a()
> } else { b() };
>
> x
> ```
";
    assert_eq!(formatted(source), expected);
}

#[test]
fn a_fence_inside_the_code_is_code() {
    let source = "\
```motoko
/// ```motoko include=import
/// let x = f(1,2);
/// ```
public func f(x : Nat, y : Nat) : Nat { x+y };
```
";
    assert_eq!(formatted(source), source.replace("x+y", "x + y"));
}

#[test]
fn a_comment_ignores_the_next_block() {
    let source = "\
<!-- mo-fmt-ignore -->
```motoko
let x = f(1,2);
```

<!--   prettier-ignore   -->

```motoko
let x = f(1,2);
```

```motoko
let x = f(1,2);
```

<!-- mo-fmt-ignore -->

Some prose.

```motoko
let x = f(1,2);
```

<!-- mo-fmt-ignore -->
- ```motoko
  let x = f(1,2);
  ```

<!-- something else -->
```motoko
let x = f(1,2);
```
";
    let out = formatted(source);
    let pieces: Vec<_> = out.split("f(1").map(|s| &s[..2]).skip(1).collect();
    assert_eq!(pieces, [",2", ",2", ", ", ", ", ",2", ", "]);
}

#[test]
fn an_ignored_block_need_not_parse() {
    let source = "<!-- mo-fmt-ignore -->\n```motoko\ntype T = ...;\n```\n";
    assert_eq!(formatted(source), source);
}

#[test]
fn formats_a_lone_member_of_an_actor_or_module() {
    let source = "\
```motoko
  public query func greet(name : Text) : async Text {
      return name;
  };
system func timer(set : Nat64 -> ()) : async () {
  set(1)
};
```
";
    let expected = "\
```motoko
public query func greet(name : Text) : async Text {
  return name;
};
system func timer(set : Nat64 -> ()) : async () {
  set(1)
};
```
";
    assert_eq!(formatted(source), expected);
}

#[test]
fn a_member_keeps_the_lines_of_its_block_comments() {
    let source = "```motoko\npublic func f() {\n    /* a\n       b */\n    g();\n};\n```\n";
    assert_eq!(
        formatted(source),
        "```motoko\npublic func f() {\n  /* a\n       b */\n  g();\n};\n```\n"
    );
}

#[test]
fn a_block_that_does_not_parse_is_an_error_at_its_line_and_the_rest_are_formatted() {
    let source = "\
Text.

- Item:

  ```motoko
  let x = f(1,2);
  type T = ...;
  ```

```motoko
let x = f(1,2);
```
";
    let out = markdown(source);
    assert_eq!(
        out.formatted,
        source.replacen("let x = f(1,2);\n```", "let x = f(1, 2);\n```", 1)
    );
    let [Error::Syntax(e)] = &out.errors[..] else {
        panic!("{:?}", out.errors);
    };
    assert_eq!(
        (e.line, e.column, e.message.as_str()),
        (7, 3, "unexpected input")
    );
    assert_eq!(e.frame, "  |\n7 |   type T = ...;\n  |   ^");
    assert!(out.skipped.is_empty());
}

#[test]
fn skips_a_block_it_could_not_write_back() {
    let source = "\
- Item:

    ```motoko
    let x = f(1,2);
  let y = 1;
    ```

```motoko
let x = f(1,2);
";
    let out = markdown(source);
    assert_eq!(out.formatted, source);
    assert!(out.errors.is_empty());
    assert_eq!(
        out.skipped,
        [
            Skipped {
                line: 3,
                reason: "the code block's lines aren't indented alike"
            },
            Skipped {
                line: 8,
                reason: "the code block isn't closed"
            },
        ]
    );
}

#[test]
fn line_endings_become_lf_and_a_byte_order_mark_is_kept() {
    let source = "\u{feff}Text.\r\n\r\n```motoko\r\nlet x = f(1,2);\r\n```\r\n";
    assert_eq!(
        formatted(source),
        "\u{feff}Text.\n\n```motoko\nlet x = f(1, 2);\n```\n"
    );
}

#[test]
fn the_readme_is_formatted() {
    let readme = include_str!("../README.md");
    assert_eq!(formatted(readme), readme);
}
