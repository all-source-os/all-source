//! Percent-encoding for values that sit in a URL *path*, not a query string.
//!
//! Search Console puts the property in the path segment, so `:` and `/` must be
//! encoded too — a request built with a query-string encoder leaves them bare
//! and 404s on a URL-prefix property.

pub fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}
