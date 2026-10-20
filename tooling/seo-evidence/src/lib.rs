//! Collects SEO and AI-search evidence from the sources a static HTML audit
//! cannot reach, and records what each source answered *including* the fact
//! that it answered nothing.
//!
//! The point of the crate is the state tag on every entry. A static audit can
//! score a page 100/100 while the site has 12 ranked keywords and 2 clicks a
//! month, because the audit measures the page and not its reception. Anything
//! this tool cannot measure stays labelled `setup_required` with the condition
//! that unblocks it, so the gap is a chore in a queue rather than a sentence in
//! a report nobody acts on.

pub mod config;
pub mod drift;
pub mod google_auth;
pub mod ledger;
pub mod pct;
pub mod sources;
