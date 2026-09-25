use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

const UNFORMATTED: &str = "let x = f(1,2);\n";
const FORMATTED: &str = "let x = f(1, 2);\n";

/// A temporary directory with these files, removed when dropped.
struct Project(PathBuf);

impl std::ops::Deref for Project {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn project(files: &[(&str, &str)]) -> Project {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mo-fmt-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (path, text) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    Project(dir)
}

fn run(dir: &Path, args: &[&str], stdin: &str) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mo-fmt"))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn read(dir: &Path, path: &str) -> String {
    std::fs::read_to_string(dir.join(path)).unwrap()
}

#[test]
fn formats_a_directory_in_place_skipping_ignored_files() {
    let dir = project(&[
        ("src/A.mo", UNFORMATTED),
        ("src/B.mo", FORMATTED),
        ("src/notes.txt", UNFORMATTED),
        ("node_modules/x/C.mo", UNFORMATTED),
        (".mops/core/D.mo", UNFORMATTED),
        ("gen/E.mo", UNFORMATTED),
        (".gitignore", "gen/\n"),
    ]);
    assert_eq!(
        run(&dir, &["."], ""),
        (
            0,
            "./src/A.mo\nFormatted 1 of 2 files.\n".into(),
            String::new()
        )
    );
    assert_eq!(read(&dir, "src/A.mo"), FORMATTED);
    for untouched in ["node_modules/x/C.mo", ".mops/core/D.mo", "gen/E.mo"] {
        assert_eq!(read(&dir, untouched), UNFORMATTED, "{untouched}");
    }
    // A file named on the command line is formatted even when ignored.
    assert_eq!(run(&dir, &["gen/E.mo"], "").0, 0);
    assert_eq!(read(&dir, "gen/E.mo"), FORMATTED);
}

#[test]
fn check_lists_the_files_that_would_change_without_writing() {
    let dir = project(&[("A.mo", UNFORMATTED), ("B.mo", FORMATTED)]);
    assert_eq!(
        run(&dir, &["--check", "A.mo", "B.mo"], ""),
        (
            1,
            "A.mo\n1 of 2 files need formatting.\n".into(),
            String::new()
        )
    );
    assert_eq!(read(&dir, "A.mo"), UNFORMATTED);
    assert_eq!(run(&dir, &["-c", "B.mo"], "").0, 0);
}

#[test]
fn reads_the_syntax_from_mo_fmt_toml() {
    let dir = project(&[
        ("mo-fmt.toml", "syntax = \"moc2\"\n"),
        ("A.mo", "if (c) x else y;\n"),
    ]);
    assert_eq!(run(&dir, &["A.mo"], "").0, 0);
    assert_eq!(read(&dir, "A.mo"), "if c { x } else { y };\n");

    let dir = project(&[("mo-fmt.toml", "indent-width = 0\n"), ("A.mo", FORMATTED)]);
    let (code, _, stderr) = run(&dir, &["A.mo"], "");
    assert_eq!(code, 2);
    assert!(
        stderr.starts_with("mo-fmt.toml: ")
            && stderr.contains("indent-width must be between 1 and 16, not 0"),
        "{stderr}"
    );

    let dir = project(&[("mo-fmt.toml", "sintax = \"moc2\"\n"), ("A.mo", FORMATTED)]);
    let (code, _, stderr) = run(&dir, &["A.mo"], "");
    assert_eq!(code, 2);
    assert!(
        stderr.starts_with("mo-fmt.toml: ") && stderr.contains("unknown field `sintax`"),
        "{stderr}"
    );
}

#[test]
fn a_file_that_fails_to_parse_is_reported_and_the_rest_are_formatted() {
    let dir = project(&[("A.mo", "let x = ;\n"), ("B.mo", UNFORMATTED)]);
    let (code, _, stderr) = run(&dir, &["A.mo", "B.mo"], "");
    assert_eq!(code, 2);
    assert_eq!(
        stderr,
        "A.mo:1:7: unexpected input\n  |\n1 | let x = ;\n  |       ^\n"
    );
    assert_eq!(read(&dir, "B.mo"), FORMATTED);
}

#[test]
fn formats_stdin() {
    let dir = project(&[]);
    assert_eq!(
        run(&dir, &["--stdin-filepath", "A.mo"], UNFORMATTED),
        (0, FORMATTED.into(), String::new())
    );
}

#[test]
fn usage_errors_exit_2() {
    let dir = project(&[("notes.txt", "")]);
    assert_eq!(run(&dir, &[], "").0, 2);
    assert_eq!(run(&dir, &["--nope"], "").0, 2);
    let (code, _, stderr) = run(&dir, &["notes.txt", "missing.mo"], "");
    assert_eq!(
        (code, stderr.as_str()),
        (
            2,
            "notes.txt: not a Motoko file\nmissing.mo: no such file or directory\n"
        )
    );
}

#[test]
fn deeply_nested_code_formats() {
    let depth = 50_000;
    let source = format!("let x = {}1{};\n", "(".repeat(depth), ")".repeat(depth));
    let dir = project(&[("A.mo", &source)]);
    assert_eq!(run(&dir, &["--check", "A.mo"], "").0, 0);
}

#[test]
fn the_same_file_named_twice_is_formatted_once() {
    let dir = project(&[("A.mo", UNFORMATTED)]);
    assert_eq!(
        run(&dir, &["./A.mo", "A.mo"], ""),
        (0, "./A.mo\nFormatted 1 of 1 file.\n".into(), String::new())
    );
}

#[cfg(unix)]
#[test]
fn unreadable_files_and_directories_are_reported() {
    use std::os::unix::fs::PermissionsExt;
    let dir = project(&[
        ("A.mo", FORMATTED),
        ("locked/B.mo", UNFORMATTED),
        ("C.mo", FORMATTED),
    ]);
    std::fs::set_permissions(dir.join("C.mo"), std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(dir.join("C.mo")).is_ok() {
        return; // Running as root, which permissions don't stop.
    }
    let (code, _, stderr) = run(&dir, &["--check", "C.mo"], "");
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.starts_with("C.mo: "), "{stderr}");

    std::fs::set_permissions(dir.join("locked"), std::fs::Permissions::from_mode(0o000)).unwrap();
    let (code, _, stderr) = run(&dir, &["--check", "."], "");
    std::fs::set_permissions(dir.join("locked"), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("locked"), "{stderr}");
}

#[test]
fn a_file_that_is_not_utf8_is_reported() {
    let dir = project(&[]);
    std::fs::write(dir.join("A.mo"), b"let x = \xff;\n").unwrap();
    let (code, _, stderr) = run(&dir, &["A.mo"], "");
    assert_eq!(code, 2);
    assert!(stderr.starts_with("A.mo: "), "{stderr}");
}

#[test]
fn stdin_reports_a_syntax_error_and_a_closed_stdout() {
    let dir = project(&[]);
    let (code, stdout, stderr) = run(&dir, &["--stdin-filepath", "A.mo"], "let x = ;\n");
    assert_eq!((code, stdout.as_str()), (2, ""));
    assert!(stderr.starts_with("A.mo:1:7: unexpected input"), "{stderr}");

    // An editor that stopped reading: mo-fmt reports the write error instead of panicking.
    let mut child = Command::new(env!("CARGO_BIN_EXE_mo-fmt"))
        .args(["--stdin-filepath", "A.mo"])
        .current_dir(&*dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    child
        .stdin
        .take()
        .unwrap()
        .write_all(FORMATTED.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(stderr.starts_with("stdout: "), "{stderr}");
}
