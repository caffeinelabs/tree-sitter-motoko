use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};

use clap::Parser;
use mo_fmt::{Config, Error, IndentWidth, Syntax, format, format_markdown};

const CONFIG_FILE: &str = "mo-fmt.toml";

/// Formats Motoko files, and the Motoko code blocks of Markdown files, in place, with the options from
/// `mo-fmt.toml` in the current directory. Flags override it.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Files and directories. Directories are searched for `.mo` files, honouring `.gitignore` and
    /// skipping `node_modules` and dot-directories.
    paths: Vec<PathBuf>,

    /// Search directories for Markdown files (`.md`, `.markdown`) too, as `markdown = true` in `mo-fmt.toml` does
    #[arg(long)]
    markdown: bool,

    /// List the files that would change, and exit 1 if any would.
    #[arg(short, long)]
    check: bool,

    /// Format stdin and print the result; the path names the file in messages.
    #[arg(long, value_name = "PATH", conflicts_with_all = ["paths", "check"])]
    stdin_filepath: Option<PathBuf>,

    /// Overrides `syntax` in `mo-fmt.toml` [default: preserve]
    #[arg(long, value_enum)]
    syntax: Option<Syntax>,

    /// Overrides `indent-width` in `mo-fmt.toml`: spaces per level, 1 to 16 [default: 2]
    #[arg(long, value_name = "N")]
    indent_width: Option<IndentWidth>,

    /// Overrides one rule, as `brace-bodies=false` or `semicolons=minimal`; repeatable. See the README for the rules
    #[arg(long = "rule", value_name = "NAME=VALUE", value_parser = Config::from_rule)]
    rules: Vec<Config>,
}

/// Exit codes: 0 done, 1 `--check` found files that need formatting, 2 a usage error or a file that failed to format.
fn main() -> ExitCode {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // `format` reports a panic on its own thread as an internal error for that file.
        if std::thread::current().name() != Some(mo_fmt::THREAD_NAME) {
            default_hook(info);
        }
    }));
    let args = Args::parse();
    let mut config = match load_config() {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{CONFIG_FILE}: {message}");
            return ExitCode::from(2);
        }
    };
    if let Some(syntax) = args.syntax {
        config.syntax = syntax;
    }
    config.markdown |= args.markdown;
    if let Some(width) = args.indent_width {
        config.indent_width = width;
    }
    for rule in &args.rules {
        config.override_rules(rule);
    }

    if let Some(path) = &args.stdin_filepath {
        return format_stdin(path, &config);
    }

    if args.paths.is_empty() {
        eprintln!("mo-fmt: no files or directories given; see --help");
        return ExitCode::from(2);
    }

    let mut failed = false;
    let mut files = Vec::new();
    for path in &args.paths {
        if path.is_dir() {
            let (found, errors) = source_files(path, config.markdown);
            files.extend(found);
            for e in errors {
                eprintln!("{e}");
                failed = true;
            }
        } else if !path.exists() {
            eprintln!("{}: no such file or directory", path.display());
            failed = true;
        } else if is_motoko(path) || is_markdown(path) {
            files.push(path.clone());
        } else {
            eprintln!("{}: not a Motoko or Markdown file", path.display());
            failed = true;
        }
    }
    files.sort();
    // `./A.mo` and `A.mo` are the same file.
    let mut seen = HashSet::new();
    files.retain(|f| seen.insert(std::fs::canonicalize(f).unwrap_or_else(|_| f.clone())));

    let mut changed = 0;
    let mut errors = 0;
    let mut out = std::io::stdout().lock();
    for file in &files {
        let outcome = format_file(file, &config, args.check);
        if outcome.changed {
            changed += 1;
            let _ = writeln!(out, "{}", file.display());
        }
        for warning in &outcome.warnings {
            eprintln!("{warning}");
        }
        for error in &outcome.errors {
            eprintln!("{error}");
        }
        if !outcome.errors.is_empty() {
            errors += 1;
            failed = true;
        }
    }

    let _ = writeln!(out, "{}", summary(args.check, files.len(), changed, errors));
    if failed {
        ExitCode::from(2)
    } else if args.check && changed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// The line that ends a run over `total` files: `changed` changed, or would have with `check`, and `failed` failed to format.
fn summary(check: bool, total: usize, changed: usize, failed: usize) -> String {
    // A file that failed is not formatted, so `--check` mustn't call it that. A Markdown file can fail and still need
    // formatting, in its other code blocks.
    let failures = match failed {
        0 => String::new(),
        n => format!("; {n} failed to format"),
    };
    if total == 0 {
        "No Motoko files found.".into()
    } else if !check {
        let noun = if total == 1 { "file" } else { "files" };
        format!("Formatted {changed} of {total} {noun}{failures}.")
    } else if total == 1 {
        match (changed, failed) {
            (1, 1) => "The file needs formatting, and part of it failed to format.",
            (1, _) => "The file needs formatting.",
            (_, 1) => "The file failed to format.",
            _ => "The file is formatted.",
        }
        .into()
    } else if changed == 0 && failed == 0 {
        format!("All {total} files are formatted.")
    } else if changed == 0 {
        format!("{failed} of {total} files failed to format.")
    } else {
        let verb = if changed == 1 { "needs" } else { "need" };
        format!("{changed} of {total} files {verb} formatting{failures}.")
    }
}

fn load_config() -> Result<Config, String> {
    match std::fs::read_to_string(CONFIG_FILE) {
        Ok(text) => Config::from_toml(&text).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(e.to_string()),
    }
}

fn format_stdin(path: &Path, config: &Config) -> ExitCode {
    let mut source = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut source) {
        eprintln!("stdin: {e}");
        return ExitCode::from(2);
    }
    let outcome = format_source(path, &source, config);
    for warning in &outcome.warnings {
        eprintln!("{warning}");
    }
    let formatted = match (outcome.formatted, outcome.errors.is_empty()) {
        (Some(formatted), true) => formatted,
        _ => {
            for error in &outcome.errors {
                eprintln!("{error}");
            }
            return ExitCode::from(2);
        }
    };
    let mut out = std::io::stdout().lock();
    // An editor that closed the pipe gets an error, not a panic.
    match out
        .write_all(formatted.as_bytes())
        .and_then(|()| out.flush())
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("stdout: {e}");
            ExitCode::from(2)
        }
    }
}

#[derive(Default)]
struct Outcome {
    /// What to write, if any of it formatted.
    formatted: Option<String>,
    /// Whether the file changed, or would have.
    changed: bool,
    /// One for the file, or one for each Markdown code block that failed to format.
    errors: Vec<String>,
    warnings: Vec<String>,
}

fn format_source(file: &Path, source: &str, config: &Config) -> Outcome {
    if !is_markdown(file) {
        return match format(source, config) {
            Ok(formatted) => Outcome {
                formatted: Some(formatted),
                ..Outcome::default()
            },
            Err(e) => Outcome {
                errors: vec![message(file, &e)],
                ..Outcome::default()
            },
        };
    }
    match format_markdown(source, config) {
        Ok(markdown) => Outcome {
            formatted: Some(markdown.formatted),
            changed: false,
            errors: markdown.errors.iter().map(|e| message(file, e)).collect(),
            warnings: (markdown.skipped.iter())
                .map(|s| {
                    format!(
                        "{}:{}: warning: {}; left as it is",
                        file.display(),
                        s.line,
                        s.reason
                    )
                })
                .collect(),
        },
        Err(e) => Outcome {
            errors: vec![message(file, &e)],
            ..Outcome::default()
        },
    }
}

/// Formats the file, or what of it formats: a Markdown file's other code blocks are formatted even when one fails.
fn format_file(file: &Path, config: &Config, check: bool) -> Outcome {
    let source = match std::fs::read_to_string(file) {
        Ok(source) => source,
        Err(e) => {
            return Outcome {
                errors: vec![format!("{}: {e}", file.display())],
                ..Outcome::default()
            };
        }
    };
    let mut outcome = format_source(file, &source, config);
    let Some(formatted) = outcome.formatted.take().filter(|f| *f != source) else {
        return outcome;
    };
    outcome.changed = true;
    if !check {
        let error = match replace(file, &source, &formatted) {
            Ok(true) => None,
            Ok(false) => Some(format!(
                "{}: changed while being formatted; left as it is",
                file.display()
            )),
            Err(e) => Some(format!("{}: {e}", file.display())),
        };
        if let Some(error) = error {
            outcome.changed = false;
            outcome.errors.push(error);
        }
    }
    outcome
}

fn is_motoko(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "mo")
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e == "md" || e == "markdown")
}

/// Replaces the file's contents if it still holds `expected`, by renaming a temporary file over it, so a reader
/// never sees it half-written and an edit made since it was read isn't overwritten. `Ok(false)` means it changed.
/// A symlink is written through to its target, and the target keeps its permissions. The rename replaces
/// the inode, so hard links to the file and its ownership aren't carried over.
fn replace(file: &Path, expected: &str, contents: &str) -> std::io::Result<bool> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let target = std::fs::canonicalize(file)?;
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let temp = target.with_file_name(format!(
        ".{name}.mo-fmt-{}-{nonce}-{}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    // `create_new` never follows or reuses whatever might already sit at that name.
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let written = out.write_all(contents.as_bytes());
    drop(out);
    let result = written.and_then(|()| {
        std::fs::set_permissions(&temp, std::fs::metadata(&target)?.permissions())?;
        let current = std::fs::read_to_string(&target)?;
        if current != expected {
            // Unless another run got here first with the same result.
            return Ok(current == contents);
        }
        std::fs::rename(&temp, &target)?;
        Ok(true)
    });
    if temp.exists() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn message(file: &Path, e: &Error) -> String {
    match e {
        Error::Syntax(_) => format!("{}:{e}", file.display()),
        Error::Internal(_) => format!("{}: {e}", file.display()),
    }
}

/// The `.mo` files under `dir`, and the Markdown files with `markdown`, and an error for each part of it that
/// couldn't be read.
fn source_files(dir: &Path, markdown: bool) -> (Vec<PathBuf>, Vec<String>) {
    let mut files = Vec::new();
    let mut errors = Vec::new();
    let walk = ignore::WalkBuilder::new(dir)
        .require_git(false)
        // The same result on every machine: only the project's own ignore files count.
        .git_global(false)
        .git_exclude(false)
        .filter_entry(|e| e.file_name() != "node_modules")
        .build();
    for entry in walk {
        match entry {
            Ok(e)
                if e.file_type().is_some_and(|t| t.is_file())
                    && (is_motoko(e.path()) || markdown && is_markdown(e.path())) =>
            {
                files.push(e.into_path());
            }
            Ok(_) => {}
            Err(e) => errors.push(e.to_string()),
        }
    }
    (files, errors)
}

#[cfg(test)]
mod tests {
    use super::{replace, summary};
    use std::fs;

    struct TempDir(std::path::PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn summary_accounts_for_every_file() {
        for (check, total, changed, failed, line) in [
            (false, 0, 0, 0, "No Motoko files found."),
            (true, 0, 0, 0, "No Motoko files found."),
            (false, 1, 1, 0, "Formatted 1 of 1 file."),
            (
                false,
                2,
                0,
                2,
                "Formatted 0 of 2 files; 2 failed to format.",
            ),
            (true, 1, 0, 0, "The file is formatted."),
            (true, 1, 1, 0, "The file needs formatting."),
            (
                true,
                1,
                1,
                1,
                "The file needs formatting, and part of it failed to format.",
            ),
            (true, 1, 0, 1, "The file failed to format."),
            (true, 2, 0, 0, "All 2 files are formatted."),
            (true, 3, 1, 0, "1 of 3 files needs formatting."),
            (
                true,
                3,
                2,
                1,
                "2 of 3 files need formatting; 1 failed to format.",
            ),
            (true, 3, 0, 2, "2 of 3 files failed to format."),
        ] {
            assert_eq!(summary(check, total, changed, failed), line);
        }
    }

    #[test]
    fn replace_keeps_a_concurrent_edit_and_follows_symlinks() {
        let dir = std::env::temp_dir().join(format!("mo-fmt-replace-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let _cleanup = TempDir(dir.clone());
        let file = dir.join("A.mo");
        fs::write(&file, "edited by someone else").unwrap();
        assert!(!replace(&file, "what was read", "formatted").unwrap());
        assert_eq!(fs::read_to_string(&file).unwrap(), "edited by someone else");
        assert_eq!(
            fs::read_dir(&dir).unwrap().count(),
            1,
            "the temporary file is removed"
        );
        // Another run already wrote the same result.
        assert!(replace(&file, "what was read", "edited by someone else").unwrap());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);

        #[cfg(unix)]
        {
            let link = dir.join("link.mo");
            std::os::unix::fs::symlink(&file, &link).unwrap();
            assert!(replace(&link, "edited by someone else", "formatted").unwrap());
            assert!(
                fs::symlink_metadata(&link)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            assert_eq!(fs::read_to_string(&file).unwrap(), "formatted");
        }
    }
}
