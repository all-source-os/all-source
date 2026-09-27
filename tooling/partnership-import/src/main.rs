//! Convert an existing private evidence/Jev/send-ledger packet to an admin import.
//! No networking, credentials or external sends. Output must be in ignored .local/.
use serde_json::{Value, json};
use std::{collections::HashSet, env, error::Error, fs, path::Path};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .ok_or_else(|| format!("Missing string field: {key}").into())
}
fn strings(v: &Value, key: &str) -> Result<Vec<String>> {
    v[key]
        .as_array()
        .ok_or("Expected string array")?
        .iter()
        .map(|s| {
            s.as_str()
                .map(str::to_owned)
                .ok_or_else(|| "Expected string".into())
        })
        .collect()
}
fn field<'a>(section: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("- {name}: ");
    section.lines().find_map(|l| l.strip_prefix(&prefix))
}
fn section<'a>(ledger: &'a str, name: &str) -> Option<&'a str> {
    ledger.split("\n## ").find_map(|part| {
        let (heading, body) = part.split_once('\n')?;
        (heading.trim().eq_ignore_ascii_case(name)
            || (name == "boldstart ventures" && heading == "boldstart"))
            .then_some(body)
    })
}
fn utc(value: &str) -> Result<String> {
    let value = value
        .strip_suffix(" UTC")
        .ok_or("Expected UTC ledger timestamp")?;
    if value.len() != 19 || !value.is_ascii() || &value[4..5] != "-" || &value[10..11] != " " {
        return Err("Invalid ledger timestamp".into());
    }
    Ok(format!("{}T{}Z", &value[..10], &value[11..]))
}
fn message(section: &str, id: &str, as_of: &str) -> Result<Value> {
    let status = field(section, "Status").ok_or("Missing sent status")?;
    if !status.starts_with("sent;") {
        return Err("Only verified sent ledger sections can migrate as sent".into());
    }
    let destination = field(section, "Destination").ok_or("Missing destination")?;
    let channel = if destination.contains("linkedin.com/") {
        "linkedin"
    } else if destination.starts_with("https://") {
        "form"
    } else {
        "email"
    };
    let start = section
        .lines()
        .position(|l| l.starts_with("Hello ") || l.starts_with("Hi "))
        .ok_or("Missing exact message")?;
    let body = section
        .lines()
        .skip(start)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned();
    let mut proof = section
        .lines()
        .filter(|l| l.starts_with("- ") && !l.starts_with("- Reply address:"))
        .collect::<Vec<_>>()
        .join("\n");
    let occurred_at = if let Some(date) = field(section, "Sent at") {
        utc(date)?
    } else if let Some(date) = field(section, "Verified by") {
        proof.push_str("\nTimestamp represents confirmation time, not an exact send time.");
        utc(date)?
    } else {
        // Preserve date-only precision rather than inventing a timezone conversion.
        proof.push_str("\nTimestamp normalised to midnight UTC from ledger date only; exact UI time retained above.");
        format!("{as_of}T00:00:00Z")
    };
    Ok(
        json!({"id":format!("legacy-{as_of}-{id}"),"channel":channel,"direction":"outbound","outcome":"sent","destination":destination,"subject":field(section,"Subject").unwrap_or(""),"body":body,"occurred_at":occurred_at,"verification":proof,"approval_note":"Historical user instruction: send to each of them. This authorised the recorded batch only, not future messages."}),
    )
}

fn convert(evidence: &Value, judgments: &Value, ledger: &str) -> Result<Value> {
    let as_of = string(evidence, "as_of")?;
    let candidates = evidence["candidates"]
        .as_array()
        .ok_or("Missing candidates")?;
    let mut records = vec![];
    let mut seen = HashSet::new();
    for candidate in candidates {
        let legacy_id = string(candidate, "id")?;
        let name = string(candidate, "name")?;
        let source_urls = strings(candidate, "sources")?;
        let first = url::Url::parse(source_urls.first().ok_or("Missing source URLs")?)?;
        let website = format!("https://{}/", first.host_str().ok_or("Missing host")?);
        let id = first.host_str().unwrap().trim_start_matches("www.");
        if !seen.insert(id.to_owned()) {
            return Err("Duplicate candidate hostname".into());
        }
        let facts = strings(candidate, "facts")?.join("\n• ");
        let source_note = format!(
            "Research packet dated {as_of}; facts were compiled from the listed source set, not individually attributed to this one URL.\n• {facts}"
        );
        let sources:Vec<_> = source_urls.iter().map(|url|json!({"url":url,"title":format!("{name}: research source"),"evidence":source_note,"checked_at":format!("{as_of}T00:00:00Z")})).collect();
        let judgment = &judgments["judgments"][legacy_id];
        let score = if judgment.is_null() {
            Value::Null
        } else {
            let reply = &judgment["reply"];
            let value = |key: &str| -> Result<f64> {
                reply["answers"][key]["score"]
                    .as_f64()
                    .filter(|n| n.is_finite() && (0.0..=3.0).contains(n))
                    .ok_or_else(|| format!("Missing/invalid score {key}").into())
            };
            json!({"model":string(reply,"model")?,"run_at":format!("{as_of}T00:00:00Z"),"fit":value("product_fit")?,"leverage":value("channel_leverage")?,"access":value("commercial_access")?,"paid_demand":value("paid_demand")?,"rationale":format!("Commercial-route evidence rubric v1. Original Jev expected ordinal scores from {as_of}, not conversion probabilities. Date-only run precision. Later contact-route discoveries have NOT overwritten the original scores. Heavybit's explicit discovery-introduction mechanism may have been underweighted; inspect packet before reranking.\nOriginal rubric: {}",serde_json::to_string(&judgment["questions"])?)})
        };
        let sent_section = section(ledger, name);
        let previously_contacted = candidate["previously_contacted"].as_bool().unwrap_or(false);
        let messages = sent_section
            .map(|s| message(s, legacy_id, as_of))
            .transpose()?
            .into_iter()
            .collect::<Vec<_>>();
        let mut notes = format!(
            "Migrated from the dated private evidence packet and outreach ledger. Source-review and score dates have date-only precision, normalised to midnight UTC. Organisation type in original packet: {}. No reply check performed during migration.",
            string(candidate, "type")?
        );
        let route = if let Some(s) = sent_section {
            notes.push_str(" Original scores precede some route improvements; verified send route supersedes the initial route suggestion.");
            format!(
                "Verified send route: {}\n{}\nOriginal research route: {}",
                field(s, "Destination").unwrap_or(""),
                field(s, "Route qualification").unwrap_or("See verification in message history."),
                string(candidate, "route")?
            )
        } else {
            if previously_contacted {
                notes.push_str(" Previous contact is explicitly reported in the evidence packet, but exact message/provider proof is absent. Do NOT resend; reconcile earlier channel history first.");
            } else {
                notes.push_str(" No sent interaction was found in this ledger. This is not proof that the organisation has never been contacted; check channel history before outreach.");
            }
            string(candidate, "route")?.to_owned()
        };
        let kind = match legacy_id {
            "motier" | "finsj" => "family_office",
            "plugandplay" => "accelerator",
            _ => "vc",
        };
        let status = if sent_section.is_some() || previously_contacted {
            "awaiting_reply"
        } else {
            "research"
        };
        records.push(json!({"id":id,"organization":name,"kind":kind,"geography":string(candidate,"geography")?,"website":website,"status":status,"angle":string(evidence,"goal")?,"contact_route":route,"next_action":"Check existing channel history and current evidence before proposing a next action. No automatic sends.","next_action_at":"","notes":notes,"limitations":strings(candidate,"limitations")?.join("\n"),"reply_checked_at":"","sources":sources,"score":score,"messages":messages}));
    }
    Ok(json!({"records":records}))
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 5 {
        return Err(
            "Usage: partnership-import EVIDENCE.json JEV.json SEND-LEDGER.md .local/OUTPUT.json"
                .into(),
        );
    }
    let output = Path::new(&args[4]);
    if !output.components().any(|c| c.as_os_str() == ".local") {
        return Err("Output must be in an ignored .local directory".into());
    }
    if output.exists() {
        return Err(
            "Output exists; choose a new filename rather than overwrite private history".into(),
        );
    }
    let packet = convert(
        &serde_json::from_str(&fs::read_to_string(&args[1])?)?,
        &serde_json::from_str(&fs::read_to_string(&args[2])?)?,
        &fs::read_to_string(&args[3])?,
    )?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, serde_json::to_vec_pretty(&packet)?)?;
    let records = packet["records"].as_array().unwrap();
    println!(
        "Prepared {} private records / {} verified send records. No API calls or sends performed.",
        records.len(),
        records
            .iter()
            .map(|r| r["messages"].as_array().unwrap().len())
            .sum::<usize>()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_message_does_not_invent_a_previous_send() {
        let mut evidence = json!({"as_of":"2026-09-26","goal":"Synthetic", "candidates":[{"id":"example","name":"Example","type":"VC","sources":["https://example.com"],"facts":["Synthetic"],"route":"Unverified","geography":"UK","limitations":["No demand verified"]}]});
        let packet = convert(&evidence, &json!({}), "No sends").unwrap();
        assert_eq!(packet["records"][0]["status"], "research");
        assert_eq!(packet["records"][0]["messages"], json!([]));
        evidence["candidates"][0]["previously_contacted"] = json!(true);
        let packet = convert(&evidence, &json!({}), "No exact message").unwrap();
        assert_eq!(packet["records"][0]["status"], "awaiting_reply");
        assert_eq!(packet["records"][0]["messages"], json!([]));
    }
    #[test]
    fn imports_exact_body_and_proof_without_sending() {
        let ledger = "- Destination: team@example.com\n- Status: sent; provider SENT verified\n- Sent at: 2026-09-26 12:34:56 UTC\n\nHello team,\n\nSynthetic message.\n";
        let m = message(ledger, "example", "2026-09-26").unwrap();
        assert_eq!(m["body"], "Hello team,\n\nSynthetic message.");
        assert_eq!(m["occurred_at"], "2026-09-26T12:34:56Z");
        assert_eq!(m["channel"], "email");
        assert!(
            message(
                &ledger.replace("sent;", "unknown;"),
                "example",
                "2026-09-26"
            )
            .is_err()
        );
    }
    #[test]
    fn unknown_exact_time_keeps_date_precision_visible() {
        let m=message("- Destination: https://example.com/contact\n- Status: sent; confirmation visible\n\nHello team, test.","example","2026-09-26").unwrap();
        assert!(m["verification"].as_str().unwrap().contains("date only"));
    }
}
