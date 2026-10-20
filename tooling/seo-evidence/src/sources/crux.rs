//! Chrome UX Report — origin-level field data.
//!
//! Queried separately from PSI because the origin record survives when no
//! single URL has enough samples to get one. A low-traffic site commonly has
//! an origin record and no per-URL records at all, and CrUX answers 404 for
//! the URL rather than returning empty metrics.

use anyhow::Result;
use serde_json::json;

use crate::{
    config::{self, RUNBOOK},
    ledger::{Entry, Evidence},
    pct,
};

pub const KEYS: &[&str] = &["CRUX_API_KEY"];

pub async fn collect(client: &reqwest::Client, origin: &str) -> Result<Vec<Entry>> {
    let measures = "origin-level field Core Web Vitals: LCP, INP, CLS, TTFB";
    let source = "crux.origin".to_owned();

    let Some(key) = config::env_opt("CRUX_API_KEY") else {
        return Ok(vec![Entry {
            source,
            measures: measures.to_owned(),
            evidence: Evidence::setup_required(
                config::missing(KEYS),
                "a Google Cloud API key with the Chrome UX Report API enabled",
                RUNBOOK,
            ),
        }]);
    };

    let url = format!(
        "https://chromeuxreport.googleapis.com/v1/records:queryRecord?key={}",
        pct::encode(&key)
    );
    let evidence = match client
        .post(&url)
        .json(&json!({ "origin": origin, "formFactor": "PHONE" }))
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            if status.is_success() {
                let parsed: serde_json::Value =
                    serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
                Evidence::observed(json!({
                    "origin": origin,
                    "form_factor": "PHONE",
                    "metrics": parsed.pointer("/record/metrics"),
                    "collection_period": parsed.pointer("/record/collectionPeriod"),
                }))
            } else if status.as_u16() == 404 {
                // CrUX 404s an origin below its sampling threshold, which is a
                // measurement ("too little traffic to assess"), not a failure.
                Evidence::observed_empty(format!(
                    "CrUX has no record for {origin}: the origin is below the public sampling threshold"
                ))
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
