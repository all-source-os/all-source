//! Comparing two ledgers.
//!
//! The regression this catches is a source that *stops* answering. A run whose
//! credential was revoked produces a ledger full of `setup_required` entries
//! and no error, which reads as a clean report unless something compares it
//! with the run before it.

use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Transition {
    pub source: String,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Serialize)]
pub struct Drift {
    pub regressions: Vec<Transition>,
    pub improvements: Vec<Transition>,
    pub appeared: Vec<String>,
    pub disappeared: Vec<String>,
}

impl Drift {
    pub fn has_regressions(&self) -> bool {
        !self.regressions.is_empty() || !self.disappeared.is_empty()
    }
}

fn states(ledger: &serde_json::Value) -> Vec<(String, String)> {
    ledger
        .get("entries")
        .and_then(|entries| entries.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    Some((
                        entry.get("source")?.as_str()?.to_owned(),
                        entry.get("state")?.as_str()?.to_owned(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Ranks the states so a transition can be called a regression or an
/// improvement. `observed` outranks everything; losing an observation is the
/// regression this exists to catch.
fn rank(state: &str) -> u8 {
    match state {
        "observed" => 4,
        "observed_empty" => 3,
        "failed" => 2,
        _ => 1,
    }
}

pub fn compare(before_json: &str, after_json: &str) -> Result<Drift> {
    let before: serde_json::Value =
        serde_json::from_str(before_json).context("--before is not a ledger JSON file")?;
    let after: serde_json::Value =
        serde_json::from_str(after_json).context("--after is not a ledger JSON file")?;

    let before_states = states(&before);
    let after_states = states(&after);

    let mut drift = Drift {
        regressions: Vec::new(),
        improvements: Vec::new(),
        appeared: Vec::new(),
        disappeared: Vec::new(),
    };

    for (source, after_state) in &after_states {
        match before_states.iter().find(|(name, _)| name == source) {
            None => drift.appeared.push(source.clone()),
            Some((_, before_state)) => {
                if before_state == after_state {
                    continue;
                }
                let transition = Transition {
                    source: source.clone(),
                    before: before_state.clone(),
                    after: after_state.clone(),
                };
                if rank(after_state) < rank(before_state) {
                    drift.regressions.push(transition);
                } else {
                    drift.improvements.push(transition);
                }
            }
        }
    }

    for (source, _) in &before_states {
        if !after_states.iter().any(|(name, _)| name == source) {
            drift.disappeared.push(source.clone());
        }
    }

    Ok(drift)
}
