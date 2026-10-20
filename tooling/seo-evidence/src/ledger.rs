//! The evidence ledger.
//!
//! Every source resolves to exactly one of three states, and the state travels
//! with the value instead of being asserted in prose next to it. A consumer
//! that forgets to branch on the state cannot read a number out of an entry
//! that has none.

use std::fmt::Write as _;

use chrono::{DateTime, Utc};
use serde::Serialize;

/// Why an entry carries no measurement, stated so a reader can act on it.
///
/// `missing` names the env vars that were absent or empty; `unblock` is the
/// condition under which this source starts producing observations. Both are
/// required: a blocked entry without an unblock condition is indistinguishable
/// from one nobody has looked at.
#[derive(Debug, Clone, Serialize)]
pub struct Blocked {
    pub missing: Vec<String>,
    pub unblock: String,
    pub runbook: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Evidence {
    /// Measured. `value` came back from the named source at `captured_at`.
    Observed {
        captured_at: DateTime<Utc>,
        value: serde_json::Value,
    },
    /// A credential, export, or binary is absent. No value, by construction.
    SetupRequired {
        #[serde(flatten)]
        blocked: Blocked,
    },
    /// The source was reachable and authoritatively answered "nothing here".
    ///
    /// Distinct from `SetupRequired` because the two demand opposite responses:
    /// this one is a finding, that one is a chore.
    ObservedEmpty {
        captured_at: DateTime<Utc>,
        note: String,
    },
    /// The source was configured and refused. Keeps the provider's own message.
    Failed {
        attempted_at: DateTime<Utc>,
        status: Option<u16>,
        message: String,
    },
}

impl Evidence {
    pub fn observed(value: serde_json::Value) -> Self {
        Self::Observed {
            captured_at: Utc::now(),
            value,
        }
    }

    pub fn observed_empty(note: impl Into<String>) -> Self {
        Self::ObservedEmpty {
            captured_at: Utc::now(),
            note: note.into(),
        }
    }

    pub fn setup_required(
        missing: Vec<String>,
        unblock: impl Into<String>,
        runbook: &'static str,
    ) -> Self {
        Self::SetupRequired {
            blocked: Blocked {
                missing,
                unblock: unblock.into(),
                runbook,
            },
        }
    }

    pub fn failed(status: Option<u16>, message: impl Into<String>) -> Self {
        Self::Failed {
            attempted_at: Utc::now(),
            status,
            message: message.into(),
        }
    }

    pub fn is_observed(&self) -> bool {
        matches!(self, Self::Observed { .. } | Self::ObservedEmpty { .. })
    }

    /// True when the entry is blocked on something a human must provide.
    /// `doctor --strict` exits non-zero on any of these.
    pub fn is_setup_required(&self) -> bool {
        matches!(self, Self::SetupRequired { .. })
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    /// Stable identifier, e.g. `gsc.search_analytics`. Referenced by findings.
    pub source: String,
    /// What this entry is evidence *of*, in one clause.
    pub measures: String,
    #[serde(flatten)]
    pub evidence: Evidence,
}

#[derive(Debug, Clone, Serialize)]
pub struct Coverage {
    pub observed: usize,
    pub setup_required: usize,
    pub failed: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Ledger {
    pub property: String,
    pub generated_at: DateTime<Utc>,
    pub schema_version: u32,
    pub coverage: Coverage,
    pub entries: Vec<Entry>,
}

impl Ledger {
    pub const SCHEMA_VERSION: u32 = 1;

    pub fn new(property: impl Into<String>, entries: Vec<Entry>) -> Self {
        let coverage = Coverage {
            observed: entries.iter().filter(|e| e.evidence.is_observed()).count(),
            setup_required: entries
                .iter()
                .filter(|e| e.evidence.is_setup_required())
                .count(),
            failed: entries.iter().filter(|e| e.evidence.is_failed()).count(),
            total: entries.len(),
        };
        Self {
            property: property.into(),
            generated_at: Utc::now(),
            schema_version: Self::SCHEMA_VERSION,
            coverage,
            entries,
        }
    }

    pub fn to_markdown(&self) -> String {
        let mut out = format!(
            "# SEO evidence ledger — {}\n\nCaptured {}. {} of {} sources observed, {} awaiting setup, {} failed.\n\n| Source | Measures | State | Detail |\n|---|---|---|---|\n",
            self.property,
            self.generated_at.to_rfc3339(),
            self.coverage.observed,
            self.coverage.total,
            self.coverage.setup_required,
            self.coverage.failed,
        );
        for entry in &self.entries {
            let (state, detail) = match &entry.evidence {
                Evidence::Observed { .. } => ("observed", "see JSON".to_owned()),
                Evidence::ObservedEmpty { note, .. } => ("observed (empty)", note.clone()),
                Evidence::SetupRequired { blocked } => (
                    "setup required",
                    format!(
                        "missing {}; {}",
                        blocked.missing.join(", "),
                        blocked.unblock
                    ),
                ),
                Evidence::Failed {
                    status, message, ..
                } => (
                    "failed",
                    status
                        .map_or_else(|| message.clone(), |code| format!("HTTP {code}: {message}")),
                ),
            };
            let _ = writeln!(
                out,
                "| `{}` | {} | {state} | {detail} |",
                entry.source, entry.measures
            );
        }
        out
    }
}
