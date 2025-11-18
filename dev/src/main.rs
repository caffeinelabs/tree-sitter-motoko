use std::fs;

use anyhow::{Context, Result};
use camino::Utf8Path;
use walkdir::WalkDir;

/// Tests for parse errors not labeled as syntax-*
const TEST_FAIL_EXCLUDES: [&str; 16] = [
    "par-bad-asyncstar.mo",
    "obj-empty-with.mo",
    "lexer-offset-1504.mo",
    "multiline-text-line-number.mo",
    "parse-block-or-record.mo", // Tests error recovery
    "syntax1.mo",
    "syntax2.mo",
    "syntax3.mo",
    "syntax3.mo",
    "syntax4.mo",
    "syntax5.mo",
    "syntax6.mo",
    "syntax7.mo",
    "syntax8.mo",
    "verification-asserts.mo",
    "verification-implies.mo",
];

fn main() -> Result<()> {
    let ts_base = Utf8Path::new("/Users/christoph.hegemann/work/tree-sitter-motoko");
    copy_test_cases(
        Utf8Path::new("/Users/christoph.hegemann/work/motoko/test/fail"),
        ts_base,
        "fail",
        &TEST_FAIL_EXCLUDES,
    )
    .unwrap();

    copy_test_cases(
        Utf8Path::new("/Users/christoph.hegemann/work/motoko/test/run"),
        ts_base,
        "run",
        // Contains a random Ctrl character
        &["menhir-bug.mo"],
    )
    .unwrap();

    copy_test_cases(
        Utf8Path::new("/Users/christoph.hegemann/work/motoko-core/src"),
        ts_base,
        "core/src",
        &[],
    )?;

    copy_test_cases(
        Utf8Path::new("/Users/christoph.hegemann/work/motoko-core/test"),
        ts_base,
        "core/test",
        &[],
    )?;

    copy_test_cases(
        Utf8Path::new("/Users/christoph.hegemann/work/motoko-core/bench"),
        ts_base,
        "core/bench",
        &[],
    )?;

    copy_test_cases(
        Utf8Path::new("/Users/christoph.hegemann/work/motoko-snafu/src"),
        ts_base,
        "snafu/src",
        &[],
    )?;

    copy_test_cases(
        Utf8Path::new("/Users/christoph.hegemann/work/motoko-snafu/test"),
        ts_base,
        "snafu/test",
        &[]
    )?;
    Ok(())
}

fn copy_test_cases(
    mo_base: &Utf8Path,
    ts_base: &Utf8Path,
    prefix: &str,
    excludes: &[&str],
) -> Result<()> {
    let test_dir = ts_base
        .join("test")
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
        if excludes.contains(&file_name) {
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
