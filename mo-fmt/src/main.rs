use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};

use clap::Parser;
use mo_fmt::{Config, Error, format};

const CONFIG_FILE: &str = "mo-fmt.toml";

/// Formats Motoko files in place, with the options from `mo-fmt.toml` in the current directory.
#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Files and directories. Directories are searched for `.mo` files, honouring `.gitignore` and
    /// skipping `node_modules` and dot-directories.
    paths: Vec<PathBuf>,

    /// List the files that would change, and exit 1 if any would.
    #[arg(short, long)]
    check: bool,

    /// Format stdin and print the result; the path names the file in messages.
    #[arg(long, value_name = "PATH", conflicts_with_all = ["paths", "check"])]
    stdin_filepath: Option<PathBuf>,
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
    let config = match load_config() {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{CONFIG_FILE}: {message}");
            return ExitCode::from(2);
        }
    };

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
            let (found, errors) = motoko_files(path);
            files.extend(found);
            for e in errors {
                eprintln!("{e}");
                failed = true;
            }
        } else if !path.exists() {
            eprintln!("{}: no such file or directory", path.display());
            failed = true;
        } else if path.extension().is_some_and(|e| e == "mo") {
            files.push(path.clone());
        } else {
            eprintln!("{}: not a Motoko file", path.display());
            failed = true;
        }
    }
    files.sort();
    // `./A.mo` and `A.mo` are the same file.
    let mut seen = HashSet::new();
    files.retain(|f| seen.insert(std::fs::canonicalize(f).unwrap_or_else(|_| f.clone())));

    let mut changed = 0;
    let mut out = std::io::stdout().lock();
    for file in &files {
        match format_file(file, &config, args.check) {
            Ok(false) => {}
            Ok(true) => {
                changed += 1;
                let _ = writeln!(out, "{}", file.display());
            }
            Err(message) => {
                eprintln!("{message}");
                failed = true;
            }
        }
    }

    let noun = if files.len() == 1 { "file" } else { "files" };
    let _ = if !args.check {
        writeln!(out, "Formatted {changed} of {} {noun}.", files.len())
    } else if changed == 0 && files.len() == 1 {
        writeln!(out, "The file is formatted.")
    } else if changed == 0 {
        writeln!(out, "All {} files are formatted.", files.len())
    } else {
        writeln!(out, "{changed} of {} {noun} need formatting.", files.len())
    };
    if failed {
        ExitCode::from(2)
    } else if args.check && changed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
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
    let formatted = match format(&source, config) {
        Ok(formatted) => formatted,
        Err(e) => {
            eprintln!("{}", message(path, &e));
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

/// Whether the file changed, or would have.
fn format_file(file: &Path, config: &Config, check: bool) -> Result<bool, String> {
    let source = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let formatted = format(&source, config).map_err(|e| message(file, &e))?;
    if formatted == source {
        return Ok(false);
    }
    if !check {
        match replace(file, &source, &formatted) {
            Ok(true) => {}
            Ok(false) => {
                return Err(format!(
                    "{}: changed while being formatted; left as it is",
                    file.display()
                ));
            }
            Err(e) => return Err(format!("{}: {e}", file.display())),
        }
    }
    Ok(true)
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

/// The `.mo` files under `dir`, and an error for each part of it that couldn't be read.
fn motoko_files(dir: &Path) -> (Vec<PathBuf>, Vec<String>) {
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
                    && e.path().extension().is_some_and(|x| x == "mo") =>
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
    use super::replace;
    use std::fs;

    struct TempDir(std::path::PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
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
