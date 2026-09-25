//! mo-fmt: a formatter for Motoko.

use std::borrow::Cow;

pub mod config;
mod doc;
mod moc2;
mod print;
// Exposed for the tests; not a stable API.
#[doc(hidden)]
pub mod tree;
mod verify;

pub use config::{Config, IndentWidth, Syntax};
pub use tree::SyntaxError;

#[derive(Debug)]
pub enum Error {
    /// The input doesn't parse.
    Syntax(SyntaxError),
    /// A bug in mo-fmt; the input is left as it was.
    Internal(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Syntax(e) => write!(f, "{e}"),
            Error::Internal(message) => write!(
                f,
                "internal error: {message}. The file was left unchanged; please report this with the file at \
                 https://github.com/caffeinelabs/tree-sitter-motoko/issues"
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Syntax(e) => Some(e),
            Error::Internal(_) => None,
        }
    }
}

const BOM: &str = "\u{feff}";

/// The name of the thread `format` runs on, for a panic hook that leaves reporting a panic to `format`.
pub const THREAD_NAME: &str = "mo-fmt";

// Parsing, printing and the guard recurse on nesting depth, which a default 8 MiB stack can't hold for
// pathological files; this is virtual memory, committed only as it's used.
const STACK_SIZE: usize = 1 << 30;

/// Formats Motoko source. The result parses to the same tree as the input, or this returns an error.
///
/// It runs on a thread of its own with a large stack, so deeply nested code doesn't overflow, and a bug
/// is returned as `Error::Internal` rather than a panic.
pub fn format(source: &str, config: &Config) -> Result<String, Error> {
    std::thread::scope(|scope| {
        let thread = std::thread::Builder::new()
            .name(THREAD_NAME.into())
            .stack_size(STACK_SIZE)
            .spawn_scoped(scope, || format_here(source, config))
            .map_err(|e| Error::Internal(format!("could not start a formatting thread ({e})")))?;
        thread.join().unwrap_or_else(|panic| {
            let reason = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            Err(Error::Internal(format!("the formatter panicked: {reason}")))
        })
    })
}

fn format_here(source: &str, config: &Config) -> Result<String, Error> {
    let (bom, body) = match source.strip_prefix(BOM) {
        Some(rest) => (BOM, rest),
        None => ("", source),
    };
    let normalized: Cow<'_, str> = if body.contains('\r') {
        body.replace("\r\n", "\n").replace('\r', "\n").into()
    } else {
        body.into()
    };
    let source: Cow<'_, str> = match config.syntax {
        Syntax::Preserve => normalized,
        Syntax::Moc2 => moc2::rewrite(&normalized)?.into(),
    };
    let root = tree::parse(&source).map_err(Error::Syntax)?;
    let printed = doc::print(print::print(&root, &source), config.indent_width.get());
    verify::verify(&root, &source, &printed).map_err(Error::Internal)?;
    Ok(format!("{bom}{printed}"))
}

#[cfg(test)]
mod tests {
    use super::{Config, format};

    #[test]
    fn line_endings_become_lf_and_a_byte_order_mark_is_kept() {
        let config = Config::default();
        assert_eq!(
            format(
                "\u{feff}let x = f(1,2);\r\nlet y = 2;\rlet z = 3;\n",
                &config
            )
            .unwrap(),
            "\u{feff}let x = f(1, 2);\nlet y = 2;\nlet z = 3;\n"
        );
        assert_eq!(format("", &config).unwrap(), "");
        assert_eq!(format(" \n\n", &config).unwrap(), "");
    }
}
