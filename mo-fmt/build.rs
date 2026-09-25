//! Extracts the symbol ids of the grammar's head-mode rules from the generated parser.
//! They are aliased onto ordinary node names, so only the id before aliasing reveals head mode.

use std::{env, fs, path::Path};

fn main() {
    let parser_c = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/parser.c");
    println!("cargo::rerun-if-changed={}", parser_c.display());
    let source = fs::read_to_string(&parser_c).expect("read ../src/parser.c");

    let start = source
        .find("enum ts_symbol_identifiers {")
        .expect("`enum ts_symbol_identifiers` in parser.c");
    let body = &source[start..start + source[start..].find("\n};").unwrap()];

    let mut ids: Vec<u16> = body
        .lines()
        .filter_map(|line| {
            let (designator, id) = line.trim().trim_end_matches(',').split_once(" = ")?;
            let rule = designator.strip_prefix("sym_")?;
            let head = rule.ends_with("_head") || rule == "_par_exp_tight";
            head.then(|| id.parse().expect("numeric symbol id"))
        })
        .collect();
    ids.sort_unstable();
    // Head rules are recognised by name, so a new head rule named otherwise would be missed silently;
    // the count makes any change to them fail here, for someone to check the names.
    const EXPECTED: usize = 27;
    assert_eq!(
        ids.len(),
        EXPECTED,
        "expected {EXPECTED} head-mode rules (`*_head` and `_par_exp_tight`) in parser.c; \
         if the grammar changed them on purpose, check every head rule matches and update EXPECTED"
    );

    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("head_symbols.rs");
    fs::write(
        out,
        format!("pub const HEAD_SYMBOL_IDS: &[u16] = &{ids:?};\n"),
    )
    .unwrap();
}
