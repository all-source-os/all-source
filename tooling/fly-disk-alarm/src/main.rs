//! Fails when any Fly volume in the org is at or above a disk-usage threshold.
//!
//! This exists because the obvious home for the alarm — a rule clicked into
//! Fly's managed Grafana — is configuration nobody can review, nobody can
//! diff, and nobody can recreate after an account change. A gate in the repo
//! is versioned with everything else and runs from CI on a schedule.
//!
//! ## The property that matters
//!
//! Every path that cannot establish a volume's usage exits NON-ZERO. A missing
//! token, an HTTP failure, an unparseable body, and an empty result set are all
//! loud. Silence here would be indistinguishable from "every volume is fine",
//! and an alarm that fails open is worse than no alarm, because it is trusted.

use std::process::{Command, ExitCode};

const DEFAULT_ORG: &str = "allsource";
const DEFAULT_THRESHOLD: f64 = 0.70;

/// Used fraction per mounted filesystem.
///
/// Fly publishes no `fly_volume_*` series to the org's Prometheus — checked
/// against the metric-name list on 2026-09-16, where the only disk-capacity
/// series are these filesystem block counters. An attached volume appears here
/// as its mount (`/app/data` on core, `/data` on prime) alongside the rootfs
/// overlay `/.fly-upper-layer`, which is watched too: a full rootfs takes a
/// machine down just as surely as a full volume does.
const QUERY: &str = "1 - (fly_instance_filesystem_blocks_avail / fly_instance_filesystem_blocks)";

#[derive(Debug)]
struct Volume {
    app: String,
    mount: String,
    region: String,
    ratio: f64,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!(
            "fly-disk-alarm [--org ORG] [--threshold 0.0-1.0] [--json]\n\n\
             Reads the Fly token from FLY_API_TOKEN, or from the file named by\n\
             FLY_API_TOKEN_FILE. Exits 1 when any volume is at or above the\n\
             threshold, and 2 when usage could not be read."
        );
        return ExitCode::SUCCESS;
    }

    let org = flag(&args, "--org").unwrap_or_else(|| DEFAULT_ORG.to_string());
    let json_out = args.iter().any(|a| a == "--json");
    let threshold = match flag(&args, "--threshold") {
        None => DEFAULT_THRESHOLD,
        Some(raw) => match raw.parse::<f64>() {
            Ok(t) if (0.0..=1.0).contains(&t) => t,
            _ => {
                eprintln!("--threshold must be a number between 0.0 and 1.0, got {raw:?}");
                return ExitCode::from(2);
            }
        },
    };

    let volumes = match fetch(&org) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("could not read Fly volume usage: {e}");
            return ExitCode::from(2);
        }
    };

    if volumes.is_empty() {
        eprintln!(
            "the query returned no volumes for org {org:?}. That is not the same as \
             'no volume is full' — check the token's org scope and that {QUERY:?} \
             still names the current Fly metrics."
        );
        return ExitCode::from(2);
    }

    let mut over: Vec<&Volume> = volumes.iter().filter(|v| v.ratio >= threshold).collect();
    over.sort_by(|a, b| b.ratio.total_cmp(&a.ratio));

    if json_out {
        let rows: Vec<serde_json::Value> = volumes
            .iter()
            .map(|v| {
                serde_json::json!({
                    "app": v.app,
                    "mount": v.mount,
                    "region": v.region,
                    "used_ratio": v.ratio,
                    "over_threshold": v.ratio >= threshold,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({ "threshold": threshold, "volumes": rows })
        );
    } else {
        println!(
            "{:<26} {:<20} {:<7} {:>7}",
            "APP", "MOUNT", "REGION", "USED"
        );
        let mut sorted: Vec<&Volume> = volumes.iter().collect();
        sorted.sort_by(|a, b| b.ratio.total_cmp(&a.ratio));
        for v in sorted {
            let mark = if v.ratio >= threshold { "  <-- OVER" } else { "" };
            println!(
                "{:<26} {:<20} {:<7} {:>6.1}%{}",
                v.app,
                v.mount,
                v.region,
                v.ratio * 100.0,
                mark
            );
        }
    }

    if over.is_empty() {
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "\n{} volume(s) at or above {:.0}% — extend the volume or free space before it fills.",
            over.len(),
            threshold * 100.0
        );
        ExitCode::from(1)
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    let i = args.iter().position(|a| a == name)?;
    args.get(i + 1).cloned()
}

/// Reads the token from `FLY_API_TOKEN`, or from the file named by
/// `FLY_API_TOKEN_FILE`. The file form keeps a live token out of the process
/// environment and out of shell history when running this by hand.
fn token() -> Result<String, String> {
    let raw = match std::env::var("FLY_API_TOKEN_FILE") {
        Ok(path) if !path.trim().is_empty() => std::fs::read_to_string(&path)
            .map_err(|e| format!("could not read FLY_API_TOKEN_FILE {path:?}: {e}"))?,
        _ => std::env::var("FLY_API_TOKEN")
            .map_err(|_| "neither FLY_API_TOKEN nor FLY_API_TOKEN_FILE is set".to_string())?,
    };
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        return Err("the Fly token is empty".into());
    }
    Ok(trimmed)
}

/// Fly issues macaroon tokens (`fm2_…`, often a comma-separated bundle) which
/// the API rejects under `Bearer`; they authenticate under the `FlyV1` scheme.
/// `fly tokens create` already prints the scheme, so accept a token that
/// carries it and do not double it up.
fn auth_header(token: &str) -> String {
    if token.starts_with("FlyV1 ") || token.starts_with("Bearer ") {
        format!("Authorization: {token}")
    } else if token.starts_with("fm1") || token.starts_with("fm2_") {
        format!("Authorization: FlyV1 {token}")
    } else {
        format!("Authorization: Bearer {token}")
    }
}

fn fetch(org: &str) -> Result<Vec<Volume>, String> {
    let token = token()?;

    let url = format!("https://api.fly.io/prometheus/{org}/api/v1/query");
    let out = Command::new("curl")
        .arg("--silent")
        .arg("--show-error")
        .arg("--fail")
        .arg("--max-time")
        .arg("30")
        .arg("-H")
        .arg(auth_header(&token))
        .arg("--data-urlencode")
        .arg(format!("query={QUERY}"))
        .arg("--get")
        .arg(&url)
        .output()
        .map_err(|e| format!("could not run curl: {e}"))?;

    if !out.status.success() {
        return Err(format!(
            "curl failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    parse(&String::from_utf8_lossy(&out.stdout))
}

fn parse(body: &str) -> Result<Vec<Volume>, String> {
    let root: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("response was not JSON: {e}"))?;

    match root.get("status").and_then(|s| s.as_str()) {
        Some("success") => {}
        other => {
            return Err(format!(
                "Prometheus status was {other:?}, not \"success\": {}",
                root.get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("no error field")
            ));
        }
    }

    let result = root
        .get("data")
        .and_then(|d| d.get("result"))
        .and_then(|r| r.as_array())
        .ok_or("response had no data.result array")?;

    let mut volumes = Vec::with_capacity(result.len());
    for series in result {
        let metric = series.get("metric");
        let label = |k: &str| {
            metric
                .and_then(|m| m.get(k))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string()
        };
        // Prometheus encodes the sample value as a string, so a plain
        // `as_f64` silently yields None and would drop the series.
        let raw = series
            .get("value")
            .and_then(|v| v.as_array())
            .and_then(|v| v.get(1))
            .and_then(|v| v.as_str())
            .ok_or("a series had no string sample value")?;
        let ratio: f64 = raw
            .parse()
            .map_err(|_| format!("sample value {raw:?} was not a number"))?;

        volumes.push(Volume {
            app: label("app"),
            mount: label("mount"),
            region: label("region"),
            ratio,
        });
    }
    Ok(volumes)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shape copied from a live response on 2026-09-16, so a Fly-side change
    // that breaks parsing shows up here rather than as a quiet all-clear.
    const OK_BODY: &str = r#"{
        "status": "success",
        "isPartial": false,
        "data": {
            "resultType": "vector",
            "result": [
                {"metric": {"app": "allsource-core", "host": "7c49", "instance": "78176", "mount": "/app/data", "region": "iad"}, "value": [1789517264, "0.23754085808146097"]},
                {"metric": {"app": "allsource-prime", "host": "e7b9", "instance": "2861d", "mount": "/data", "region": "iad"}, "value": [1789517264, "0.0682395323927163"]}
            ]
        }
    }"#;

    #[test]
    fn parses_labels_and_string_sample_values() {
        let v = parse(OK_BODY).expect("parses");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].app, "allsource-core");
        assert_eq!(v[0].mount, "/app/data");
        assert_eq!(v[0].region, "iad");
        assert!((v[0].ratio - 0.237_540_858_081_460_97).abs() < f64::EPSILON);
    }

    #[test]
    fn an_error_status_is_an_error_not_an_empty_list() {
        let body = r#"{"status":"error","error":"bad_data: parse error"}"#;
        let err = parse(body).unwrap_err();
        assert!(err.contains("parse error"), "{err}");
    }

    #[test]
    fn a_non_json_body_is_an_error() {
        assert!(parse("<html>502</html>").is_err());
    }

    #[test]
    fn a_missing_result_array_is_an_error() {
        let err = parse(r#"{"status":"success","data":{}}"#).unwrap_err();
        assert!(err.contains("data.result"), "{err}");
    }

    #[test]
    fn a_numeric_sample_value_is_rejected_rather_than_skipped() {
        // Prometheus sends strings. If that ever changes, the tool must say so
        // instead of reporting a volume-free org.
        let body = r#"{"status":"success","data":{"result":[
            {"metric":{"app":"a","mount":"/data"},"value":[1, 0.9]}]}}"#;
        assert!(parse(body).is_err());
    }

    #[test]
    fn a_macaroon_authenticates_under_flyv1_not_bearer() {
        assert_eq!(auth_header("fm2_abc"), "Authorization: FlyV1 fm2_abc");
        assert_eq!(auth_header("FlyV1 fm2_abc"), "Authorization: FlyV1 fm2_abc");
        assert_eq!(auth_header("plain"), "Authorization: Bearer plain");
    }

    #[test]
    fn an_empty_result_parses_but_carries_no_volumes() {
        let v = parse(r#"{"status":"success","data":{"result":[]}}"#).expect("parses");
        assert!(v.is_empty());
    }
}
