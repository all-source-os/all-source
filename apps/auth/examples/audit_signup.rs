//! Aggregate-only reconciliation. Raw Core records stay in process memory.
//! Run with `cargo run --example audit_signup -- <since> <until>`.
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::{collections::HashSet, process::Command};

fn core(path: &str) -> Result<Value> {
    // Paths are generated from URL-encoded query parameters, never shell input.
    let command = format!(
        "sh -c 'curl --fail --silent --show-error --max-time 20 -H \"X-API-Key: $ALLSOURCE_BOOTSTRAP_API_KEY\" \"http://localhost:3900{path}\"'"
    );
    let output = Command::new("fly")
        .args(["ssh", "console", "-a", "allsource-core", "-C", &command])
        .output()?;
    if !output.status.success() {
        bail!("Core audit request failed; no counts reported");
    }
    let text = String::from_utf8(output.stdout)?;
    // Fly may prepend a connection notice. JSON itself is a single curl body.
    let start = text.find(['[', '{']).context("missing Core JSON")?;
    Ok(serde_json::from_str(&text[start..])?)
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let since = args.get(1).context("since required")?;
    let until = args.get(2).context("until required")?;
    let start = chrono::DateTime::parse_from_rfc3339(since)?;
    let end = chrono::DateTime::parse_from_rfc3339(until)?;
    if end <= start {
        bail!("until must follow since");
    }
    let in_window = |date: &str| -> Result<bool> {
        let date = chrono::DateTime::parse_from_rfc3339(date)?;
        Ok(date >= start && date < end)
    };
    let tenants = core("/api/v1/tenants")?;
    let tenants = tenants.as_array().context("tenant array required")?;
    if tenants.len() >= 10000 {
        bail!("tenant enumeration may be truncated");
    }
    let mut all_auth = 0_u64;
    let mut window_users = HashSet::new();
    let mut qa_users = HashSet::new();
    let mut event_types = std::collections::BTreeMap::<String, u64>::new();
    let mut tenants_created = 0;
    let mut audited_tenants = 0;
    let mut new_customer_tenants = HashSet::new();
    let mut active_tenants = HashSet::new();
    let mut qa_active_tenants = HashSet::new();
    for tenant in tenants {
        let id = tenant["id"].as_str().context("tenant id")?;
        if args.get(3).map(String::as_str) != Some("--all-retained")
            && id != "allsource-auth"
            && !id.starts_with("email-")
        {
            continue;
        }
        audited_tenants += 1;
        let before_auth = all_auth;
        if let Some(date) = tenant["created_at"].as_str() {
            if in_window(date)? && id != "allsource-auth" {
                tenants_created += 1;
            }
        }
        let mut offset = 0;
        loop {
            let query = url::form_urlencoded::Serializer::new(String::new())
                .append_pair("tenant_id", id)
                .append_pair("event_type", "auth.user.created")
                .append_pair("limit", "100")
                .append_pair("offset", &offset.to_string())
                .finish();
            let page = core(&format!("/api/v1/events/query?{query}"))?;
            let events = page["events"].as_array().context("events array")?;
            all_auth += events.len() as u64;
            for event in events {
                let date = event["timestamp"].as_str().context("timestamp")?;
                if in_window(date)? {
                    let entity = format!("{id}/{}", event["entity_id"].as_str().context("entity")?);
                    let email = event["payload"]["email"].as_str().unwrap_or("");
                    if email.ends_with("@example.invalid") && email.starts_with("allsource-qa-") {
                        qa_users.insert(entity);
                    } else {
                        window_users.insert(entity);
                        if id == "allsource-auth" {
                            new_customer_tenants.insert(format!(
                                "email-{}",
                                event["payload"]["id"].as_str().context("auth user id")?
                            ));
                        }
                    }
                }
            }
            if !page["has_more"].as_bool().context("has_more")? {
                break;
            }
            if events.is_empty() {
                bail!("incomplete event page");
            }
            offset += events.len();
            if offset > 100000 {
                bail!("audit bound exceeded");
            }
        }
        if all_auth > before_auth {
            // Only report non-email infrastructure tenant identifiers.
            let label = if [
                "default",
                "allsource",
                "allsource-auth",
                "longhand",
                "system",
            ]
            .contains(&id)
            {
                id
            } else {
                "other-redacted"
            };
            eprintln!(
                "auth_stream_scope={label} user_created_records={}",
                all_auth - before_auth
            );
        }
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("tenant_id", id)
            .append_pair("since", since)
            .append_pair("until", until)
            .append_pair("limit", "1000")
            .finish();
        let page = core(&format!("/api/v1/events/query?{query}"))?;
        if page["has_more"] == true {
            bail!("window has over 1000 events; paginate before claiming counts");
        }
        for event in page["events"].as_array().context("window events")? {
            if !in_window(event["timestamp"].as_str().context("event timestamp")?)? {
                continue;
            }
            let ty = event["event_type"].as_str().context("type")?;
            *event_types.entry(ty.to_owned()).or_default() += 1;
            if id.starts_with("email-") {
                let source = event["metadata"]["source"].as_str().unwrap_or("");
                let qa = ty.starts_with("qa.")
                    || event["metadata"]["analytics_test"] == true
                    || event["payload"]["analytics_test"] == true;
                let seeded = ["demo_seed", "onboarding_sample"].contains(&source);
                let system = ["auth.", "_", "system.", "geo.", "ops.", "marketing."]
                    .iter()
                    .any(|prefix| ty.starts_with(prefix));
                if qa {
                    qa_active_tenants.insert(id.to_owned());
                } else if !seeded && !system {
                    active_tenants.insert(id.to_owned());
                }
            }
        }
    }
    println!(
        "{}",
        serde_json::json!({"since":since,"until_exclusive":until,"audited_tenants":audited_tenants,"tenants_created_in_window_including_qa":tenants_created,"auth_user_created_all_time":all_auth,"non_qa_email_signups_in_window":window_users.len(),"qa_email_signups_in_window":qa_users.len(),"new_email_customer_cohort_activated":new_customer_tenants.intersection(&active_tenants).count(),"qa_workspaces_with_test_event":qa_active_tenants.len(),"window_event_types":event_types,"scope":"AllSource email auth and email workspaces by default; --all-retained expands scan; deleted tenants and OAuth users excluded; activation means a non-seeded non-system event from this signup cohort"})
    );
    Ok(())
}
