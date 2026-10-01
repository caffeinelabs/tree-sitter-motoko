//! `mo-fmt.toml`.

use serde::Deserialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Syntax {
    /// Keep the syntax as written.
    #[default]
    Preserve,
    /// Rewrite legacy syntax to the moc 2.0 forms. May change between minor versions while moc 2.0 is in beta.
    Moc2,
}

/// Spaces per indentation level, from 1 to 16: anything wider is almost certainly a typo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "i64")]
pub struct IndentWidth(u8);

impl IndentWidth {
    pub const MAX: u8 = 16;

    pub fn get(self) -> usize {
        self.0.into()
    }
}

impl std::str::FromStr for IndentWidth {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let n = s
            .parse::<i64>()
            .map_err(|_| format!("indent-width must be a number, not {s:?}"))?;
        Self::try_from(n)
    }
}

impl Default for IndentWidth {
    fn default() -> Self {
        IndentWidth(2)
    }
}

impl TryFrom<i64> for IndentWidth {
    type Error = String;

    fn try_from(n: i64) -> Result<Self, String> {
        match u8::try_from(n) {
            Ok(n @ 1..=Self::MAX) => Ok(IndentWidth(n)),
            _ => Err(format!(
                "indent-width must be between 1 and {}, not {n}",
                Self::MAX
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Semicolons {
    #[default]
    Preserve,
    /// Drop every `;` moc doesn't need.
    Minimal,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrailingCommas {
    #[default]
    Preserve,
    /// One after the last item of a list broken one item per line, none on a list on one line.
    Multiline,
    Never,
}

/// Declares each rule once: its `mo-fmt.toml` key in `Config`, and its value under each `syntax` preset in `Rules`.
macro_rules! rules {
    ($($(#[$doc:meta])* $name:ident: $ty:ty = $preserve:expr, $moc2:expr;)*) => {
        #[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
        #[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
        pub struct Config {
            pub syntax: Syntax,
            pub indent_width: IndentWidth,
            $(pub $name: Option<$ty>,)*
        }

        /// What each rule does, once the overrides in `Config` are applied to its `syntax` preset.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct Rules {
            $($(#[$doc])* pub $name: $ty,)*
        }

        impl Config {
            const RULES: &[&str] = &[$(stringify!($name),)*];

            pub fn rules(&self) -> Rules {
                let mut rules = match self.syntax {
                    Syntax::Preserve => Rules { $($name: $preserve,)* },
                    Syntax::Moc2 => Rules { $($name: $moc2,)* },
                };
                $(if let Some(v) = self.$name {
                    rules.$name = v;
                })*
                rules
            }

            /// Takes the rules `other` sets.
            pub fn override_rules(&mut self, other: &Config) {
                $(if other.$name.is_some() {
                    self.$name = other.$name;
                })*
            }
        }
    };
}

rules! {
    /// Every control body becomes a braced block.
    brace_bodies: bool = false, true;
    /// Drops the parentheses around a control head and a `loop … while` condition.
    unparen_heads: bool = false, true;
    /// Drops the parentheses around a `case` or `catch` pattern.
    unparen_patterns: bool = false, true;
    /// A block where the target syntax reads `{` as a record is spelled `do { … }`.
    do_blocks: bool = false, true;
    semicolons: Semicolons = Semicolons::Preserve, Semicolons::Minimal;
    trailing_commas: TrailingCommas = TrailingCommas::Preserve, TrailingCommas::Preserve;
}

impl Config {
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// A config that sets just the rule in `name=value`, as `--rule` takes it, with the value spelled as in `mo-fmt.toml`.
    pub fn from_rule(arg: &str) -> Result<Self, String> {
        let Some((name, value)) = arg.split_once('=') else {
            return Err(format!("expected NAME=VALUE, not {arg:?}"));
        };
        let name = name.trim();
        if !Self::RULES.iter().any(|r| r.replace('_', "-") == name) {
            let known: Vec<_> = Self::RULES.iter().map(|r| r.replace('_', "-")).collect();
            return Err(format!(
                "unknown rule {name:?}; the rules are {}",
                known.join(", ")
            ));
        }
        let value = match value.trim() {
            v @ ("true" | "false") => toml::Value::Boolean(v == "true"),
            v => toml::Value::String(v.into()),
        };
        let table = toml::Table::from_iter([(name.to_string(), value)]);
        table
            .try_into()
            .map_err(|e: toml::de::Error| format!("{name}: {}", e.message().trim_end()))
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, Semicolons, Syntax};

    #[test]
    fn a_rule_overrides_its_preset() {
        let config = Config::from_toml("syntax = \"moc2\"\nunparen-heads = false\n").unwrap();
        let rules = config.rules();
        assert!(rules.brace_bodies && !rules.unparen_heads);
        assert_eq!(rules.semicolons, Semicolons::Minimal);

        let config = Config::from_toml("brace-bodies = true\n").unwrap();
        let rules = config.rules();
        assert!(rules.brace_bodies && !rules.unparen_heads);
        assert_eq!(rules.semicolons, Semicolons::Preserve);
    }

    #[test]
    fn a_rule_flag_overrides_the_file() {
        let mut config = Config::from_toml("syntax = \"moc2\"\nbrace-bodies = false\n").unwrap();
        for arg in ["brace-bodies=true", "semicolons = preserve"] {
            config.override_rules(&Config::from_rule(arg).unwrap());
        }
        assert_eq!(config.syntax, Syntax::Moc2);
        assert!(config.rules().brace_bodies);
        assert_eq!(config.rules().semicolons, Semicolons::Preserve);
    }

    #[test]
    fn a_bad_rule_flag_says_what_is_wrong() {
        for (arg, message) in [
            ("brace-bodies", "expected NAME=VALUE"),
            (
                "syntax=moc2",
                "unknown rule \"syntax\"; the rules are brace-bodies,",
            ),
            (
                "brace-bodies=yes",
                "brace-bodies: invalid type: string \"yes\", expected a boolean",
            ),
            ("semicolons=all", "semicolons: unknown variant `all`"),
        ] {
            let e = Config::from_rule(arg).unwrap_err();
            assert!(e.starts_with(message), "{arg}: {e}");
        }
    }
}
