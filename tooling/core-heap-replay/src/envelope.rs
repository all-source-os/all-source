//! JSONL v1 envelope: one object on stdout per evaluator run.

use serde::Serialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
};

pub const SCHEMA: &str = "jsonl.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pass,
    Fail,
    Error,
    Panic,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Envelope {
    pub schema: &'static str,
    pub evaluator: String,
    pub status: Status,
    /// `pass` or `fail` per hard gate. Names only; never paths, durations or PIDs.
    pub gates: BTreeMap<String, &'static str>,
    pub metrics: BTreeMap<String, Value>,
    /// A fixed code such as `build_failed`, never free text from a child.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Envelope {
    pub fn new(evaluator: &str) -> Self {
        Self {
            schema: SCHEMA,
            evaluator: evaluator.to_string(),
            status: Status::Pass,
            gates: BTreeMap::new(),
            metrics: BTreeMap::new(),
            reason: None,
        }
    }

    pub fn gate(&mut self, name: &str, pass: bool) {
        self.gates
            .insert(name.to_string(), if pass { "pass" } else { "fail" });
        if !pass {
            self.status = Status::Fail;
        }
    }

    pub fn metric(&mut self, name: &str, value: impl Serialize) {
        self.metrics.insert(
            name.to_string(),
            serde_json::to_value(value).unwrap_or(Value::Null),
        );
    }

    /// Mark the run failed with a fixed reason code.
    pub fn fail(&mut self, reason: &str) {
        self.status = Status::Fail;
        self.reason = Some(reason.to_string());
    }

    /// Every outcome exits 0; the verdict is in the envelope.
    pub fn exit_code(&self) -> i32 {
        0
    }

    pub fn to_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            format!("{{\"schema\":\"{SCHEMA}\",\"status\":\"error\",\"reason\":\"serialize\"}}")
        })
    }
}

/// Run an evaluator body. A panic becomes a `panic` envelope with no text from
/// the panic, so a gate can never leak a path or value through a message.
pub fn guarded(evaluator: &str, body: impl FnOnce() -> Envelope) -> Envelope {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(envelope) => envelope,
        Err(_) => {
            let mut envelope = Envelope::new(evaluator);
            envelope.status = Status::Panic;
            envelope.reason = Some("panic".to_string());
            envelope
        }
    }
}
