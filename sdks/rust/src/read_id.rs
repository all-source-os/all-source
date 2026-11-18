use std::fmt;

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

    pub fn as_str(&self) -> &str {
        &self.0
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
