//! Offline synthetic decisions: append, correct, restart, replay without inference.
use allsource_core::embedded::{Config, EmbeddedCore, IngestEvent, Query};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{env, process::Command};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
enum Category {
    Billing,
    Access,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
enum Record {
    Source {
        text: String,
    },
    Classified {
        source_sequence: u64,
        question_version: String,
        policy_version: String,
        provider: String,
        model: String,
        category: Category,
    },
    Corrected {
        decision_sequence: u64,
        category: Category,
        reason: String,
    },
}

impl Record {
    fn event_type(&self) -> &'static str {
        match self {
            Self::Source { .. } => "ticket.recorded",
            Self::Classified { .. } => "ticket.classified",
            Self::Corrected { .. } => "ticket.corrected",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct Entry {
    sequence: u64,
    record: Record,
}

#[derive(Debug, Default, PartialEq)]
struct View {
    category: Option<Category>,
    human_override: bool,
}

// This reducer has no provider argument or network path. Historical results
// depend only on stored events, never on today's model or prompt.
fn replay(entries: &[Entry]) -> Result<View, &'static str> {
    let mut view = View::default();
    for (index, entry) in entries.iter().enumerate() {
        if entry.sequence != index as u64 + 1 {
            return Err("non-contiguous sequence or duplicate");
        }
        match &entry.record {
            Record::Source { .. } if index == 0 => {}
            Record::Source { .. } => return Err("source already recorded"),
            Record::Classified {
                source_sequence,
                category,
                question_version,
                policy_version,
                provider,
                model,
            } => {
                if *source_sequence != 1 || !matches!(entries[0].record, Record::Source { .. }) {
                    return Err("missing source reference");
                }
                if [question_version, policy_version, provider, model]
                    .iter()
                    .any(|value| value.is_empty())
                {
                    return Err("missing decision provenance");
                }
                if !view.human_override {
                    view.category = Some(category.clone());
                }
            }
            Record::Corrected {
                decision_sequence,
                category,
                reason,
            } => {
                if !entries[..index].iter().any(|prior| {
                    prior.sequence == *decision_sequence
                        && matches!(prior.record, Record::Classified { .. })
                }) || reason.is_empty()
                {
                    return Err("invalid correction reference or reason");
                }
                view.category = Some(category.clone());
                view.human_override = true;
            }
        }
    }
    Ok(view)
}

fn fixture() -> Vec<Entry> {
    let classified = |version: &str| Record::Classified {
        source_sequence: 1,
        question_version: version.into(),
        policy_version: "support-routing-v1".into(),
        provider: "offline-fixture-not-jev".into(),
        model: "no-model-called".into(),
        category: Category::Billing,
    };
    [
        Record::Source {
            text: "Synthetic ticket: invoice paid but account remains locked.".into(),
        },
        classified("classify-v1"),
        Record::Corrected {
            decision_sequence: 2,
            category: Category::Access,
            reason: "Synthetic reviewer: payment succeeded; access is the remaining problem."
                .into(),
        },
        // Reevaluation is another event, not an overwrite of history.
        classified("classify-v2"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, record)| Entry {
        sequence: index as u64 + 1,
        record,
    })
    .collect()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() == 1 {
        let directory = tempfile::tempdir()?;
        for phase in ["append", "replay"] {
            let status = Command::new(env::current_exe()?)
                .arg(phase)
                .arg(directory.path())
                .status()?;
            if !status.success() {
                return Err(format!("{phase} failed: {status}").into());
            }
        }
        println!("PASS: restart preserves Billing history and Access correction; no model called.");
        return Ok(());
    }
    if args.len() != 3 || !["append", "replay"].contains(&args[1].as_str()) {
        return Err("Run without arguments for isolated temporary storage.".into());
    }
    let core = EmbeddedCore::open(Config::builder().data_dir(&args[2]).build()?).await?;
    if args[1] == "append" {
        assert!(core
            .query(Query::new().entity_id("synthetic-ticket"))
            .await?
            .is_empty());
        for entry in fixture() {
            core.ingest(IngestEvent {
                entity_id: "synthetic-ticket",
                event_type: entry.record.event_type(),
                payload: serde_json::to_value(&entry)?,
                metadata: Some(json!({"synthetic": true, "inference": "offline-fixture"})),
                tenant_id: None,
            })
            .await?;
        }
    }
    let events = core
        .query(Query::new().entity_id("synthetic-ticket"))
        .await?;
    let mut entries = Vec::new();
    for event in events {
        let entry: Entry = serde_json::from_value(event.payload)?;
        assert_eq!(event.event_type, entry.record.event_type());
        entries.push(entry);
    }
    // Single-writer application sequence, not a global ordering guarantee.
    entries.sort_by_key(|entry| entry.sequence);
    assert_eq!(entries, fixture());
    assert_eq!(replay(&entries[..2])?.category, Some(Category::Billing));
    assert_eq!(replay(&entries[..3])?.category, Some(Category::Access));
    assert_eq!(
        replay(&entries)?,
        View {
            category: Some(Category::Access),
            human_override: true
        }
    );
    println!(
        "{}: stored records=4; historical=Billing; corrected=Access; reevaluated=Access",
        args[1]
    );
    core.shutdown().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_and_override_survive_reevaluation() {
        let entries = fixture();
        assert_eq!(replay(&entries[..1]).unwrap(), View::default());
        assert_eq!(
            replay(&entries[..2]).unwrap().category,
            Some(Category::Billing)
        );
        assert_eq!(replay(&entries).unwrap(), replay(&entries[..3]).unwrap());
    }

    #[test]
    fn broken_source_and_correction_references_rejected() {
        let mut entries = fixture();
        if let Record::Classified {
            source_sequence, ..
        } = &mut entries[1].record
        {
            *source_sequence = 99;
        }
        assert!(replay(&entries).is_err());
        let mut entries = fixture();
        if let Record::Corrected {
            decision_sequence, ..
        } = &mut entries[2].record
        {
            *decision_sequence = 4;
        }
        assert!(replay(&entries).is_err());
    }

    #[test]
    fn duplicate_conflict_and_order_rejected() {
        let mut entries = fixture();
        entries[3].sequence = 2;
        assert!(replay(&entries).is_err());
        let mut entries = fixture();
        entries.swap(1, 2);
        assert!(replay(&entries).is_err());
    }

    #[test]
    fn missing_provenance_rejected() {
        let mut entries = fixture();
        if let Record::Classified {
            question_version, ..
        } = &mut entries[1].record
        {
            question_version.clear();
        }
        assert!(replay(&entries).is_err());
    }
}
