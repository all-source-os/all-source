//! Experimental application-side adapter. No dependency on Core or model calls during replay.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, time::Duration};

pub const QUESTION_VERSION: &str = "ticket-route-v1";
pub const POLICY_VERSION: &str = "review-below-0.8-v1";

pub struct Event {
    pub tenant: String,
    pub id: String,
    pub text: String,
    /// Deliberately not included in outbound requests.
    pub private_metadata: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", content = "detail")]
pub enum Outcome {
    Classified(String),
    Abstained,
    Pending(Failure),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Failure {
    Disabled,
    Timeout,
    Transport,
    Http(u16),
    InvalidResponse,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Decision {
    pub tenant: String,
    pub source_id: String,
    pub revision: u64,
    pub question_version: String,
    pub policy_version: String,
    pub model: Option<String>,
    pub confidence: Option<f64>,
    pub usage: Option<Usage>,
    pub outcome: Outcome,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Deserialize)]
struct Response {
    model: String,
    answers: BTreeMap<String, Choice>,
    usage: Usage,
}

#[derive(Deserialize)]
struct Choice {
    #[serde(rename = "type")]
    kind: String,
    choice: String,
    probabilities: BTreeMap<String, f64>,
    confidence: f64,
}

/// Transport errors are fixed enums, never provider response bodies or key-bearing URLs.
pub trait Transport {
    fn send(&self, request: Value) -> impl Future<Output = Result<Value, Failure>> + Send;
}

#[derive(Default)]
pub struct Adapter {
    pub enabled: bool,
}

impl Adapter {
    pub async fn classify<T: Transport>(
        &self,
        transport: &T,
        tenant: &str,
        events: &[Event],
        revision: u64,
    ) -> Vec<Decision> {
        let mut decisions = Vec::new();
        // Scope before constructing state, including before disabled-mode handling.
        for event in events.iter().filter(|event| event.tenant == tenant) {
            let mut decision = Decision {
                tenant: tenant.into(),
                source_id: event.id.clone(),
                revision,
                question_version: QUESTION_VERSION.into(),
                policy_version: POLICY_VERSION.into(),
                model: None,
                confidence: None,
                usage: None,
                outcome: Outcome::Pending(Failure::Disabled),
            };
            if self.enabled {
                let request = json!({
                    "model": "jev-latest", "state": {"text": event.text},
                    "questions": {"route": {"type": "choice",
                        "instructions": "Choose the unresolved support problem, not incidental keywords.",
                        "criteria": {"billing": "Unresolved payment or invoice problem",
                            "access": "Unable to access an account, including after payment"}}}
                });
                match bounded_send(transport, request).await.and_then(validate) {
                    Ok(response) => {
                        let answer = &response.answers["route"];
                        decision.model = Some(response.model);
                        decision.confidence = Some(answer.confidence);
                        decision.usage = Some(response.usage);
                        decision.outcome = if answer.confidence < 0.8 {
                            Outcome::Abstained
                        } else {
                            Outcome::Classified(answer.choice.clone())
                        };
                    }
                    Err(error) => decision.outcome = Outcome::Pending(error),
                }
            }
            decisions.push(decision);
        }
        decisions
    }
}

async fn bounded_send<T: Transport>(transport: &T, request: Value) -> Result<Value, Failure> {
    // At most two 5s attempts plus one 250ms backoff. A timeout is not retried:
    // provider may already have charged the request even without a response.
    for attempt in 0..2 {
        let result = tokio::time::timeout(Duration::from_secs(5), transport.send(request.clone()))
            .await
            .map_err(|_| Failure::Timeout)?;
        if attempt == 0 && matches!(result, Err(Failure::Http(429 | 529))) {
            tokio::time::sleep(Duration::from_millis(250)).await;
        } else {
            return result;
        }
    }
    unreachable!("second attempt always returns")
}

fn validate(value: Value) -> Result<Response, Failure> {
    let response: Response = serde_json::from_value(value).map_err(|_| Failure::InvalidResponse)?;
    let answer = response
        .answers
        .get("route")
        .ok_or(Failure::InvalidResponse)?;
    let valid_probability = |value: f64| value.is_finite() && (0.0..=1.0).contains(&value);
    if response.model.is_empty()
        || response.answers.len() != 1
        || answer.kind != "choice"
        || !valid_probability(answer.confidence)
        || answer.probabilities.len() != 2
        || !["billing", "access"]
            .iter()
            .all(|key| answer.probabilities.contains_key(*key))
        || !answer.probabilities.values().all(|p| valid_probability(*p))
        || (answer.probabilities.values().sum::<f64>() - 1.0).abs() > 0.0001
    {
        return Err(Failure::InvalidResponse);
    }
    let chosen = answer
        .probabilities
        .get(&answer.choice)
        .ok_or(Failure::InvalidResponse)?;
    if answer.probabilities.values().any(|p| p > chosen) {
        return Err(Failure::InvalidResponse);
    }
    Ok(response)
}

/// Read projection only: never calls a provider. Every source retains a visible
/// pending state until a decision exists. Conflicting same-revision writes fail.
pub fn replay(
    tenant: &str,
    events: &[Event],
    decisions: &[Decision],
) -> Result<Vec<(String, Outcome)>, &'static str> {
    let mut result = Vec::new();
    for event in events.iter().filter(|e| e.tenant == tenant) {
        let mut versions = BTreeMap::new();
        for decision in decisions
            .iter()
            .filter(|d| d.tenant == tenant && d.source_id == event.id)
        {
            if let Some(previous) = versions.insert(decision.revision, decision)
                && previous != decision
            {
                return Err("conflicting decision revision");
            }
        }
        result.push((
            event.id.clone(),
            versions
                .last_key_value()
                .map(|(_, d)| d.outcome.clone())
                .unwrap_or(Outcome::Pending(Failure::Disabled)),
        ));
    }
    Ok(result)
}

#[derive(Debug, Default, PartialEq)]
pub struct Filtered {
    pub matched: Vec<String>,
    pub excluded: Vec<String>,
    pub unresolved: Vec<String>,
}

/// Semantic filtering consumes stored labels, not arbitrary query-time prompts.
/// Callers receive unresolved IDs separately so pending records cannot vanish.
pub fn filter_recorded(
    tenant: &str,
    events: &[Event],
    decisions: &[Decision],
    category: &str,
) -> Result<Filtered, &'static str> {
    if !["access", "billing"].contains(&category) {
        return Err("unknown category");
    }
    let mut filtered = Filtered::default();
    for (id, outcome) in replay(tenant, events, decisions)? {
        match outcome {
            Outcome::Classified(label) if label == category => filtered.matched.push(id),
            Outcome::Classified(_) => filtered.excluded.push(id),
            _ => filtered.unresolved.push(id),
        }
    }
    Ok(filtered)
}

#[cfg(feature = "live")]
pub struct HttpTransport {
    client: reqwest::Client,
    key: String,
}

#[cfg(feature = "live")]
impl HttpTransport {
    pub fn new(key: String) -> Result<Self, Failure> {
        if key.is_empty() {
            return Err(Failure::Transport);
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| Failure::Transport)?;
        Ok(Self { client, key })
    }
}

#[cfg(feature = "live")]
impl Transport for HttpTransport {
    async fn send(&self, request: Value) -> Result<Value, Failure> {
        let response = self
            .client
            .post("https://api.typesafe.ai/v1/systemone")
            .bearer_auth(&self.key)
            .json(&request)
            .send()
            .await
            .map_err(|_| Failure::Transport)?;
        if !response.status().is_success() {
            return Err(Failure::Http(response.status().as_u16()));
        }
        // Bound allocation independently of untrusted Content-Length.
        let mut response = response;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Failure::Transport)? {
            if bytes.len() + chunk.len() > 65536 {
                return Err(Failure::InvalidResponse);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| Failure::InvalidResponse)
    }
}
