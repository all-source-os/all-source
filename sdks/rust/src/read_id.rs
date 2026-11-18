use std::{fmt, str::FromStr};

/// Identifies one read walk. Every page of an [`EventPaginator`](crate::EventPaginator)
/// carries the same id, so the server can count a walk's pages together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadId(String);

impl ReadId {
    /// A new id: the first 16 hex characters of a random UUID v4.
    pub fn new() -> Self {
        let simple = uuid::Uuid::new_v4().simple().to_string();
        Self(simple[..16].to_string())
    }

    /// The id in `text` when it is exactly 16 lowercase hex characters, the
    /// pattern Core and the gateway accept; `None` otherwise.
    pub fn parse(text: &str) -> Option<Self> {
        (text.len() == 16 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
            .then(|| Self(text.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The text is not 16 lowercase hex characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidReadId;

impl fmt::Display for InvalidReadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a read id is 16 lowercase hex characters")
    }
}

impl std::error::Error for InvalidReadId {}

impl FromStr for ReadId {
    type Err = InvalidReadId;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text).ok_or(InvalidReadId)
    }
}

impl TryFrom<&str> for ReadId {
    type Error = InvalidReadId;

    fn try_from(text: &str) -> Result<Self, Self::Error> {
        Self::parse(text).ok_or(InvalidReadId)
    }
}

impl Default for ReadId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ReadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
#[path = "read_id_tests.rs"]
mod tests;
