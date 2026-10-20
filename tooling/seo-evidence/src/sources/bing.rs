//! Bing Webmaster Tools.
//!
//! Worth a source of its own rather than treating Google as a proxy for all
//! search: Bing's index is what ChatGPT search and Copilot read, so an AI-
//! citability claim rests on Bing coverage as much as on Google's.

use anyhow::Result;
use serde_json::json;

use crate::{
    config::{self, RUNBOOK},
    ledger::{Entry, Evidence},
    pct,
};

pub const KEYS: &[&str] = &["BING_WEBMASTER_API_KEY"];

/// `GetQueryStats` is the query-level report; `GetUrlInfo` answers indexation
/// for one URL. Both take the same apikey query parameter.
const ENDPOINTS: &[(&str, &str, &str)] = &[
    (
        "bing.query_stats",
        "GetQueryStats",
        "queries Bing serves this site for: impressions, clicks, average position",
    ),
    (
        "bing.rank_and_traffic",
        "GetRankAndTrafficStats",
        "Bing impressions and clicks over time — the indexation trend",
    ),
];

pub async fn collect(client: &reqwest::Client, site_url: &str) -> Result<Vec<Entry>> {
    let Some(key) = config::env_opt("BING_WEBMASTER_API_KEY") else {
        return Ok(ENDPOINTS
            .iter()
            .map(|(source, _, measures)| Entry {
                source: (*source).to_owned(),
                measures: (*measures).to_owned(),
                evidence: Evidence::setup_required(
                    config::missing(KEYS),
                    "the site is verified in Bing Webmaster Tools AND an API key is generated under Settings -> API access",
                    RUNBOOK,
                ),
            })
            .collect());
    };

    let mut entries = Vec::new();
    for (source, method, measures) in ENDPOINTS {
        let url = format!(
            "https://ssl.bing.com/webmaster/api.svc/json/{method}?siteUrl={}&apikey={}",
            pct::encode(site_url),
            pct::encode(&key),
        );
        let evidence = match client.get(&url).send().await {
            Ok(response) => {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                if status.is_success() {
                    let parsed: serde_json::Value =
                        serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
                    let rows = parsed.get("d").and_then(|d| d.as_array());
                    match rows {
                        Some(rows) if !rows.is_empty() => Evidence::observed(json!({
                            "site_url": site_url,
                            "method": method,
                            "row_count": rows.len(),
                            "rows": rows,
                        })),
                        _ => Evidence::observed_empty(format!(
                            "Bing returned zero rows for {method} on {site_url}"
                        )),
                    }
                } else {
                    Evidence::failed(Some(status.as_u16()), text)
                }
            }
            Err(error) => Evidence::failed(None, error.to_string()),
        };
        entries.push(Entry {
            source: (*source).to_owned(),
            measures: (*measures).to_owned(),
            evidence,
        });
    }
    Ok(entries)
}
