//! Credential discovery.
//!
//! An absent GitHub Actions secret is interpolated as an EMPTY STRING, not an
//! unset variable, so `env::var("X").is_err()` is false for a secret that does
//! not exist and every downstream call then fails with an unhelpful 401. Read
//! every credential through [`env_opt`], which treats empty as absent.

use std::env;

pub const RUNBOOK: &str = "docs/runbooks/SEO_EVIDENCE_SETUP.md";

pub fn env_opt(key: &str) -> Option<String> {
    match env::var(key) {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => None,
    }
}

/// Names of the required vars that are absent or empty.
pub fn missing(keys: &[&str]) -> Vec<String> {
    keys.iter()
        .filter(|key| env_opt(key).is_none())
        .map(|key| (*key).to_owned())
        .collect()
}

/// The property the whole run is about. Everything else is derived from it.
pub fn site_url() -> String {
    env_opt("SEO_SITE_URL").unwrap_or_else(|| "https://www.all-source.xyz".to_owned())
}

/// Search Console wants the property exactly as it is registered, which for a
/// URL-prefix property includes the scheme and the trailing slash.
pub fn gsc_property() -> String {
    env_opt("GSC_PROPERTY").unwrap_or_else(|| {
        let site = site_url();
        if site.ends_with('/') {
            site
        } else {
            format!("{site}/")
        }
    })
}

/// Pages audited when the caller names none. Chosen as one per template rather
/// than the highest-traffic pages: a template defect repeats across every page
/// built from it, and traffic is exactly what this tool cannot see yet.
pub fn default_pages() -> Vec<String> {
    let site = site_url();
    [
        "/",
        "/pricing",
        "/docs",
        "/prime",
        "/what-is-an-event-store",
        "/platform/event-sourcing",
        "/install/cursor",
        "/blog/why-your-agents-memory-returned-nothing",
    ]
    .iter()
    .map(|path| format!("{}{path}", site.trim_end_matches('/')))
    .collect()
}
