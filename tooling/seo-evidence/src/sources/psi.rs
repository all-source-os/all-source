//! PageSpeed Insights, which carries both lab and field data.
//!
//! One call returns a Lighthouse run (lab) and, when the origin has enough
//! traffic, the CrUX record for that URL (field) under `loadingExperience`.
//! Field data is the one that decides a Core Web Vitals assessment; a lab
//! score is a diagnostic, never the assessment.
//!
//! The anonymous quota is zero — a keyless call answers HTTP 429 with
//! `quota_limit_value: "0"`, so this source cannot be made to work without a
//! key no matter how few URLs it is pointed at.

use anyhow::Result;
use serde_json::json;

use crate::{
    config::{self, RUNBOOK},
    ledger::{Entry, Evidence},
    pct,
};

pub const KEYS: &[&str] = &["PAGESPEED_API_KEY"];

pub async fn collect(client: &reqwest::Client, pages: &[String]) -> Result<Vec<Entry>> {
    let measures = "lab Lighthouse scores and, where CrUX has coverage, field Core Web Vitals";

    let Some(key) = config::env_opt("PAGESPEED_API_KEY") else {
        return Ok(pages
            .iter()
            .map(|page| Entry {
                source: format!("psi.{page}"),
                measures: measures.to_owned(),
                evidence: Evidence::setup_required(
                    config::missing(KEYS),
                    "a Google Cloud API key with the PageSpeed Insights API enabled (the anonymous quota is 0)",
                    RUNBOOK,
                ),
            })
            .collect());
    };

    let mut entries = Vec::new();
    for page in pages {
        for strategy in ["mobile", "desktop"] {
            let url = format!(
                "https://www.googleapis.com/pagespeedonline/v5/runPagespeed?url={}&strategy={strategy}&category=performance&category=seo&category=accessibility&category=best-practices&key={}",
                pct::encode(page),
                pct::encode(&key),
            );
            let evidence = match client.get(&url).send().await {
                Ok(response) => {
                    let status = response.status();
                    let text = response.text().await.unwrap_or_default();
                    if status.is_success() {
                        let parsed: serde_json::Value =
                            serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
                        Evidence::observed(summarize(&parsed, strategy))
                    } else {
                        Evidence::failed(Some(status.as_u16()), text)
                    }
                }
                Err(error) => Evidence::failed(None, error.to_string()),
            };
            entries.push(Entry {
                source: format!("psi.{strategy}.{page}"),
                measures: measures.to_owned(),
                evidence,
            });
        }
    }
    Ok(entries)
}

/// Keeps the category scores and the field block, and discards the full audit
/// list. A single unabridged PSI response is several hundred kilobytes, which
/// would make the ledger unreadable and the drift diff meaningless.
fn summarize(parsed: &serde_json::Value, strategy: &str) -> serde_json::Value {
    let categories = parsed.pointer("/lighthouseResult/categories");
    let score = |name: &str| {
        categories
            .and_then(|c| c.pointer(&format!("/{name}/score")))
            .and_then(serde_json::Value::as_f64)
            .map(|value| (value * 100.0).round())
    };

    json!({
        "strategy": strategy,
        "final_url": parsed.pointer("/lighthouseResult/finalUrl"),
        "lab_scores": {
            "performance": score("performance"),
            "seo": score("seo"),
            "accessibility": score("accessibility"),
            "best_practices": score("best-practices"),
        },
        "field": {
            "overall_category": parsed.pointer("/loadingExperience/overall_category"),
            "metrics": parsed.pointer("/loadingExperience/metrics"),
        },
        "origin_field": {
            "overall_category": parsed.pointer("/originLoadingExperience/overall_category"),
        },
    })
}
