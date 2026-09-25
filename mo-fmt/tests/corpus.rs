//! Formats the compiler's tests and motoko-core, checked out beside this repository as `../motoko` and `../motoko-core`.
//! Skipped when they're missing, unless `MOTOKO_CORPUS_REQUIRED` is set.

use std::path::{Path, PathBuf};

use mo_fmt::{Config, Error, Syntax, format};

// Valid Motoko the grammar can't parse: an `@`-privileged name, which only privileged mode accepts.
const KNOWN_REJECTIONS: &[&str] = &["motoko/test/run-drun/timer.mo"];

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name != "_out" && name != "node_modules" && !name.starts_with('.') {
                files(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "mo") {
            out.push(path);
        }
    }
}

fn leaves(n: &mo_fmt::tree::Node<'_>) -> String {
    match n {
        mo_fmt::tree::Node::Branch(b) => b.children.iter().map(leaves).collect(),
        _ => n.text().to_string(),
    }
}

#[test]
fn every_file_round_trips_formats_stably_and_motoko_core_is_unchanged() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let core = parent.join("motoko-core");
    let mut all = Vec::new();
    files(&parent.join("motoko/test"), &mut all);
    files(&core, &mut all);
    if std::env::var_os("MOTOKO_CORPUS_REQUIRED").is_some() {
        assert!(
            all.len() > 500,
            "the corpus is not checked out: {} files",
            all.len()
        );
    } else if all.is_empty() {
        return;
    }

    let mut failures = Vec::new();
    for syntax in [Syntax::Preserve, Syntax::Moc2] {
        let config = Config {
            syntax,
            ..Config::default()
        };
        for file in &all {
            let name = file.strip_prefix(&parent).unwrap().display().to_string();
            let source = std::fs::read_to_string(file).unwrap();
            if syntax == Syntax::Preserve
                && let Ok(root) = mo_fmt::tree::parse(&source)
                && leaves(&root) != source
            {
                failures.push(format!("{name}: the tree doesn't reproduce the source"));
            }
            match format(&source, &config) {
                Ok(once) => {
                    if format(&once, &config).ok().as_ref() != Some(&once) {
                        failures.push(format!(
                            "{syntax:?} {name}: a second format changed the output"
                        ));
                    }
                    if syntax == Syntax::Preserve && file.starts_with(&core) && once != source {
                        failures.push(format!(
                            "{syntax:?} {name}: motoko-core, already formatted, changed"
                        ));
                    }
                }
                Err(Error::Syntax(_))
                    if name.contains("/test/fail/")
                        || KNOWN_REJECTIONS.contains(&name.as_str()) => {}
                Err(e) => failures.push(format!("{syntax:?} {name}: {e}")),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
