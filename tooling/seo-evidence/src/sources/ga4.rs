//! GA4 Data API.
//!
//! Answers what organic search sessions do after they land, which no crawler
//! can see. `GA4_PROPERTY_ID` is the numeric property id from the GA4 admin —
//! not the `G-` measurement id that the site tag carries.

use anyhow::Result;
use serde_json::json;

use crate::{
    config::{self, RUNBOOK},
    google_auth,
    ledger::{Entry, Evidence},
};

const SCOPE: &str = "https://www.googleapis.com/auth/analytics.readonly";
pub const KEYS: &[&str] = &["GA4_SERVICE_ACCOUNT_JSON", "GA4_PROPERTY_ID"];

pub async fn collect(client: &reqwest::Client, window_days: i64) -> Result<Vec<Entry>> {
    let measures = "sessions, engagement and conversions by landing page for organic search";
    let source = "ga4.organic_landing_pages".to_owned();

    let missing = config::missing(KEYS);
    if !missing.is_empty() {
        return Ok(vec![Entry {
            source,
            measures: measures.to_owned(),
            evidence: Evidence::setup_required(
                missing,
                "a service-account key exists AND that account's email has Viewer on the GA4 property AND GA4_PROPERTY_ID is the numeric property id",
                RUNBOOK,
            ),
        }]);
    }

    let key_json = config::env_opt("GA4_SERVICE_ACCOUNT_JSON").unwrap_or_default();
    let property_id = config::env_opt("GA4_PROPERTY_ID").unwrap_or_default();

    let token = match google_auth::access_token(client, &key_json, SCOPE).await {
        Ok(token) => token,
        Err(error) => {
            return Ok(vec![Entry {
                source,
                measures: measures.to_owned(),
                evidence: Evidence::failed(None, error.to_string()),
            }]);
        }
    };

    let body = json!({
        "dateRanges": [{ "startDate": format!("{window_days}daysAgo"), "endDate": "yesterday" }],
        "dimensions": [{ "name": "landingPagePlusQueryString" }, { "name": "sessionDefaultChannelGroup" }],
        "metrics": [
            { "name": "sessions" },
            { "name": "engagedSessions" },
            { "name": "averageSessionDuration" },
            { "name": "keyEvents" }
        ],
        "dimensionFilter": {
            "filter": {
                "fieldName": "sessionDefaultChannelGroup",
                "stringFilter": { "matchType": "EXACT", "value": "Organic Search" }
            }
        },
        "limit": 200,
    });

    let url =
        format!("https://analyticsdata.googleapis.com/v1beta/properties/{property_id}:runReport");
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
                        "property_id": property_id,
                        "window_days": window_days,
                        "row_count": rows.len(),
                        "rows": rows,
                    })),
                    _ => Evidence::observed_empty(format!(
                        "GA4 property {property_id} recorded no organic-search sessions in the last {window_days} days"
                    )),
                }
            } else {
                Evidence::failed(Some(status.as_u16()), text)
            }
        }
        Err(error) => Evidence::failed(None, error.to_string()),
    };

    Ok(vec![Entry {
        source,
        measures: measures.to_owned(),
        evidence,
    }])
}
