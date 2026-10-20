//! Search Console Search Analytics.
//!
//! The only source that can answer "is this page indexed and what does it rank
//! for" about *this* property rather than a third-party crawl of it. Nothing
//! else in this tool substitutes for it.

use anyhow::Result;
use serde_json::json;

use crate::{
    config::{self, RUNBOOK},
    google_auth,
    ledger::{Entry, Evidence},
    pct,
};

const SCOPE: &str = "https://www.googleapis.com/auth/webmasters.readonly";
pub const KEYS: &[&str] = &["GSC_SERVICE_ACCOUNT_JSON"];

/// Search Console has no data for the last ~2 days, and the final 1-2 days of
/// any window are partial. Ending 3 days back keeps a run comparable with the
/// one before it instead of showing a fake decline at the right-hand edge.
const LAG_DAYS: i64 = 3;

pub async fn collect(client: &reqwest::Client, window_days: i64) -> Result<Vec<Entry>> {
    let measures_queries =
        "queries this property is served for: clicks, impressions, CTR, position";
    let measures_pages = "per-page clicks and impressions — the indexation signal";

    let Some(key_json) = config::env_opt("GSC_SERVICE_ACCOUNT_JSON") else {
        let blocked = Evidence::setup_required(
            config::missing(KEYS),
            "a service-account key exists AND that account's email is added as a user on the Search Console property",
            RUNBOOK,
        );
        return Ok(vec![
            Entry {
                source: "gsc.queries".to_owned(),
                measures: measures_queries.to_owned(),
                evidence: blocked.clone(),
            },
            Entry {
                source: "gsc.pages".to_owned(),
                measures: measures_pages.to_owned(),
                evidence: blocked,
            },
        ]);
    };

    let token = match google_auth::access_token(client, &key_json, SCOPE).await {
        Ok(token) => token,
        Err(error) => {
            let failed = Evidence::failed(None, error.to_string());
            return Ok(vec![
                Entry {
                    source: "gsc.queries".to_owned(),
                    measures: measures_queries.to_owned(),
                    evidence: failed.clone(),
                },
                Entry {
                    source: "gsc.pages".to_owned(),
                    measures: measures_pages.to_owned(),
                    evidence: failed,
                },
            ]);
        }
    };

    let end = chrono::Utc::now().date_naive() - chrono::Duration::days(LAG_DAYS);
    let start = end - chrono::Duration::days(window_days);
    let property = config::gsc_property();
    let url = format!(
        "https://searchconsole.googleapis.com/webmasters/v3/sites/{}/searchAnalytics/query",
        pct::encode(&property)
    );

    let mut entries = Vec::new();
    for (source, dimension, measures) in [
        ("gsc.queries", "query", measures_queries),
        ("gsc.pages", "page", measures_pages),
    ] {
        let body = json!({
            "startDate": start.to_string(),
            "endDate": end.to_string(),
            "dimensions": [dimension],
            "rowLimit": 500,
        });
        let evidence = match client
            .post(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                if status.is_success() {
                    let parsed: serde_json::Value =
                        serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
                    let rows = parsed.get("rows").and_then(|r| r.as_array());
                    match rows {
                        Some(rows) if !rows.is_empty() => Evidence::observed(json!({
                            "property": property,
                            "start_date": start.to_string(),
                            "end_date": end.to_string(),
                            "dimension": dimension,
                            "row_count": rows.len(),
                            "rows": rows,
                        })),
                        // Zero rows from an authenticated property is a finding,
                        // not a setup gap — the two demand opposite responses.
                        _ => Evidence::observed_empty(format!(
                            "property {property} returned zero {dimension} rows for {start}..{end}"
                        )),
                    }
                } else {
                    Evidence::failed(Some(status.as_u16()), text)
                }
            }
            Err(error) => Evidence::failed(None, error.to_string()),
        };
        entries.push(Entry {
            source: source.to_owned(),
            measures: measures.to_owned(),
            evidence,
        });
    }
    Ok(entries)
}
