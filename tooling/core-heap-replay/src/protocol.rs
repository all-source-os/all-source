//! Adapter from the evaluator envelope to autoresearch JSONL protocol v1.
//!
//! The runner writes one request line to the evaluator's stdin and reads one
//! response line from its stdout. Every hard gate and metric in a `Spec` is
//! reported on every path. A gate the evaluator did not reach is `passed:
//! false`. A metric it did not reach is `0`, which only happens on a path where
//! a hard gate has already failed, so the candidate is discarded either way.

use crate::{envelope::Envelope, sha256_hex};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{io::BufRead, path::Path};

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub evaluator_id: String,
    pub run_id: String,
    pub baseline_commit: String,
    pub evaluated_commit: String,
    pub candidate_worktree: String,
}

#[derive(Debug, Clone, Copy)]
pub struct Metric {
    pub name: &'static str,
    /// `objective` or `diagnostic`, as the manifest declares it.
    pub kind: &'static str,
    pub direction: &'static str,
}

/// The measurements one evaluator reports. Names must match its manifest entry
/// exactly; `tests/autoresearch_protocol.rs` checks that they do.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    pub evaluator: &'static str,
    pub gates: &'static [&'static str],
    pub metrics: &'static [Metric],
}

const fn diagnostic(name: &'static str) -> Metric {
    Metric {
        name,
        kind: "diagnostic",
        direction: "minimize",
    }
}

/// Diagnostics are deterministic counts only: `verify` compares every
/// measurement exactly, so a byte total that moves between runs cannot be one.
pub const HEAP_REPLAY: Spec = Spec {
    evaluator: "heap_replay",
    gates: &[
        "fixture_present",
        "fixture_verified",
        "build_ok",
        "replay_completed",
        "replay_repeatable",
        "peak_nonzero",
        "replay_expected_status",
        "ledger_two_sided",
        "rows_decoded_ok",
        "eviction_policy_unchanged",
        "policy_digest_pinned",
        "query_digest_equal",
        "prod_hasher_digest_equal",
        "codec_digest_equal",
        "geo_digest_equal",
        "evictions_match",
        "protected_paths_exist",
        "heap_eval_self_sha",
    ],
    metrics: &[
        Metric {
            name: "peak_live_bytes",
            kind: "objective",
            direction: "minimize",
        },
        diagnostic("rows_decoded"),
        diagnostic("requests"),
    ],
};

pub const CORE_TESTS: Spec = Spec {
    evaluator: "core_tests",
    gates: &[
        "tests_pass",
        "expected_tests_ran",
        "tests_unchanged",
        "tests_digest_pinned",
        "tooling_tests_pass",
        "tests_eval_self_sha",
    ],
    metrics: &[
        diagnostic("core_passed"),
        diagnostic("core_failed"),
        diagnostic("core_ignored"),
        diagnostic("tooling_passed"),
    ],
};

/// Reads the request line. Any read or parse failure is the same reason code.
pub fn read_request(mut input: impl BufRead) -> Result<Request, String> {
    let mut line = String::new();
    input
        .read_line(&mut line)
        .map_err(|_| "request_unreadable".to_string())?;
    serde_json::from_str(&line).map_err(|_| "request_unreadable".to_string())
}

/// The success response: one `numeric` or `hard_gate` measurement per `Spec` entry.
pub fn response(request: &Request, envelope: &Envelope, spec: &Spec) -> Value {
    let gates = spec.gates.iter().map(|gate| {
        let passed = envelope.gates.get(*gate).is_some_and(|v| *v == "pass");
        json!({
            "kind": "hard_gate",
            "name": gate,
            "outcome": {"passed": passed, "detail": null},
        })
    });
    let metrics = spec.metrics.iter().map(|m| {
        let value = envelope
            .metrics
            .get(m.name)
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        json!({
            "kind": "numeric",
            "name": m.name,
            "metric_kind": m.kind,
            "direction": m.direction,
            "value": value,
        })
    });
    let measurements: Vec<Value> = gates.chain(metrics).collect();
    json!({
        "protocol_version": 1,
        "result": {
            "status": "success",
            "output": {
                "evaluator_id": request.evaluator_id,
                "run_id": request.run_id,
                "baseline_commit": request.baseline_commit,
                "evaluated_commit": request.evaluated_commit,
                "measurements": measurements,
                "observations": [],
                "artifacts": [],
                "warnings": [],
            },
        },
    })
}

/// A failure the runner keeps only as its class. `reason` is a fixed code.
pub fn failure(reason: &str) -> Value {
    json!({
        "protocol_version": 1,
        "result": {
            "status": "failure",
            "failure": {"class": "reported", "detail": reason},
        },
    })
}

/// `KEY=VALUE` from a literal argument. The key is non-empty and the value may contain `=`.
pub fn split_env(pair: &str) -> Option<(String, String)> {
    let (key, value) = pair.split_once('=')?;
    (!key.is_empty()).then(|| (key.to_string(), value.to_string()))
}

/// SHA-256 of this executable's bytes, for the self-check literal.
pub fn self_sha256() -> Option<String> {
    std::env::current_exe()
        .ok()
        .and_then(|path| std::fs::read(path).ok())
        .map(|bytes| sha256_hex(&bytes))
}

/// SHA-256 of a file's bytes, or `None` when it cannot be read.
pub fn file_sha256(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|bytes| sha256_hex(&bytes))
}
