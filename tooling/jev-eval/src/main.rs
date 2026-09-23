use jev_eval::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Instant;

#[derive(Deserialize)]
struct Case {
    id: String,
    text: String,
    label: String,
}
struct Offline;
impl Transport for Offline {
    async fn send(&self, _: Value) -> Result<Value, Failure> {
        Err(Failure::Disabled)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let live = match args.as_slice() {
        [] => false,
        [flag] if flag == "--live-synthetic" => true,
        _ => return Err("Usage: jev-eval [--live-synthetic]".into()),
    };
    #[cfg(not(feature = "live"))]
    if live {
        return Err("Live transport not compiled; enable feature live explicitly.".into());
    }
    #[cfg(feature = "live")]
    let transport = if live {
        Some(
            HttpTransport::new(
                std::env::var("TYPESAFE_API_KEY").map_err(|_| "TYPESAFE_API_KEY missing")?,
            )
            .map_err(|_| "Transport configuration failed")?,
        )
    } else {
        None
    };
    let cases: Vec<Case> = serde_json::from_str(include_str!("../fixtures/held-out.json"))?;
    let mut correct = 0;
    let mut false_negatives = 0;
    let mut live_correct = 0;
    let mut classified = 0;
    let mut live_false_negatives = 0;
    let mut latency = vec![];
    let mut records = vec![];
    for case in &cases {
        // Frozen baseline: lexical invoice/payment => billing; otherwise access.
        let lower = case.text.to_lowercase();
        let predicted = if lower.contains("invoice") || lower.contains("payment") {
            "billing"
        } else {
            "access"
        };
        correct += usize::from(predicted == case.label);
        false_negatives += usize::from(case.label == "access" && predicted != "access");
        let events = [Event {
            tenant: "synthetic".into(),
            id: case.id.clone(),
            text: case.text.clone(),
            private_metadata: json!({}),
        }];
        let start = Instant::now();
        #[cfg(feature = "live")]
        let decisions = if let Some(t) = &transport {
            Adapter { enabled: true }
                .classify(t, "synthetic", &events, 1)
                .await
        } else {
            Adapter::default()
                .classify(&Offline, "synthetic", &events, 1)
                .await
        };
        #[cfg(not(feature = "live"))]
        let decisions = Adapter::default()
            .classify(&Offline, "synthetic", &events, 1)
            .await;
        if live {
            latency.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        if let Outcome::Classified(label) = &decisions[0].outcome {
            classified += 1;
            live_correct += usize::from(label == &case.label);
            live_false_negatives += usize::from(case.label == "access" && label != "access");
        }
        records.extend(decisions);
    }
    latency.sort_by(f64::total_cmp);
    let p95 = if latency.is_empty() {
        None
    } else {
        Some(latency[(latency.len() * 95).div_ceil(100) - 1])
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "dataset":"hand-authored-synthetic-v1-not-production-evidence", "samples":cases.len(),
            "rules":{"correct":correct,"accuracy":correct as f64/cases.len() as f64,
                "access_false_negatives":false_negatives},
            "jev":{"run":live,"classified":classified,"correct":live_correct,
                "accuracy_all_samples":if live{Some(live_correct as f64/cases.len() as f64)}else{None},
                "access_false_negatives_among_classified":if live{Some(live_false_negatives)}else{None},
                "unresolved":records.iter().filter(|d|!matches!(d.outcome,Outcome::Classified(_))).count(),
                "p95_ms":p95,"cost_usd":if live{None}else{Some(0.0)},
                "cost_note":"Live price not configured; token usage retained, no cost invented"},
            "decisions":records
        }))?
    );
    Ok(())
}
