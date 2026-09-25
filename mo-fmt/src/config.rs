//! `mo-fmt.toml`.

use serde::Deserialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    pub syntax: Syntax,
    pub indent_width: IndentWidth,
}

impl Config {
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }
}
