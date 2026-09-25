use std::fs;
use std::process;
use std::env;

use anyhow::{Context, Result};
use camino::Utf8Path;
use walkdir::WalkDir;

/// Compiler tests that are genuine parse errors and so cannot become
/// positive corpus tests, next to every `syntax*.mo` (see `is_fail_excluded`).
const TEST_FAIL_EXCLUDES: [&str; 9] = [
    "do-postfix.mo",
    "par-bad-asyncstar.mo",
    "par-bad-nocall.mo",
    "obj-empty-with.mo",
    "lexer-offset-1504.mo",
    "multiline-text-line-number.mo",
    "parse-block-or-record.mo", // Tests error recovery
    "verification-asserts.mo",
    "verification-implies.mo",
];

fn is_fail_excluded(file_name: &str) -> bool {
    TEST_FAIL_EXCLUDES.contains(&file_name) || file_name.starts_with("syntax")
}

fn main() -> Result<()> {
    if !fs::exists("../justfile")? || !fs::exists("../tree-sitter.json")? {
        eprintln!("Error: Expected to run via `just test-generate` in the top-level directory of `tree-sitter-motoko`");
        process::exit(1)
    }

    env::set_current_dir("..")?;

    copy_test_cases(
        Utf8Path::new("../motoko/test/fail"),
        "fail",
        is_fail_excluded,
    )
    .unwrap();

    copy_test_cases(
        Utf8Path::new("../motoko/test/run"),
        "run",
        // Contains a random Ctrl character
        |f| f == "menhir-bug.mo",
    )
    .unwrap();

    copy_test_cases(
        Utf8Path::new("../motoko-core/src"),
        "core/src",
        |_| false,
    )?;

    copy_test_cases(
        Utf8Path::new("../motoko-core/test"),
        "core/test",
        |_| false,
    )?;

    copy_test_cases(
        Utf8Path::new("../motoko-core/bench"),
        "core/bench",
        |_| false,
    )?;
    Ok(())
}

fn copy_test_cases(
    mo_base: &Utf8Path,
    prefix: &str,
    is_excluded: impl Fn(&str) -> bool,
) -> Result<()> {
    let test_dir =
        Utf8Path::new("test")
        .join("corpus")
        .join("generated")
        .join(prefix);
    for entry in WalkDir::new(mo_base) {
        let entry = entry?;
        let path = Utf8Path::from_path(entry.path()).unwrap();
        if !path.is_file() || path.extension() != Some("mo") {
            continue;
        }
        let file_name = path.file_name().unwrap();
        if is_excluded(file_name) {
            continue;
        }
        let test_name = path.strip_prefix(mo_base).unwrap();
        let content = fs::read_to_string(path)?;
        let test = mk_test(test_name.as_str(), &content);
        let out_path = test_dir.join(test_name).with_extension("mo");
        fs::create_dir_all(out_path.parent().unwrap())?;
        fs::write(&out_path, test).context(format!("Failed to create test at: {out_path}"))?
    }
    Ok(())
}

fn mk_test(name: &str, content: &str) -> String {
    format!("=========\n{name}\n=========\n\n{content}\n---\n")
}
