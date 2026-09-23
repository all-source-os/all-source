use jev_eval::*;
use serde_json::{Value, json};
use std::sync::Mutex;

struct Fake {
    requests: Mutex<Vec<Value>>,
    replies: Mutex<Vec<Result<Value, Failure>>>,
    delay: bool,
}
impl Transport for Fake {
    async fn send(&self, request: Value) -> Result<Value, Failure> {
        self.requests.lock().unwrap().push(request);
        if self.delay {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
        self.replies.lock().unwrap().remove(0)
    }
}
fn fake(replies: Vec<Result<Value, Failure>>) -> Fake {
    Fake {
        requests: Mutex::new(vec![]),
        replies: Mutex::new(replies),
        delay: false,
    }
}
fn response(confidence: f64) -> Value {
    json!({"model":"fixture-not-jev", "answers":{"route":{
        "type":"choice","choice":"access", "confidence":confidence,
        "probabilities":{"access":0.9,"billing":0.1}}},
        "usage":{"input_tokens":10,"output_tokens":5}})
}
fn events() -> Vec<Event> {
    vec![
        Event {
            tenant: "allowed".into(),
            id: "same-id".into(),
            text: "Synthetic locked account".into(),
            private_metadata: json!({"email":"never-send@example.invalid"}),
        },
        Event {
            tenant: "other".into(),
            id: "same-id".into(),
            text: "other-tenant-secret".into(),
            private_metadata: json!({}),
        },
    ]
}

#[tokio::test]
async fn filtering_keeps_unresolved_and_excludes_other_tenants() {
    let fake = fake(vec![Ok(response(0.9))]);
    let decisions = Adapter { enabled: true }
        .classify(&fake, "allowed", &events(), 1)
        .await;
    assert_eq!(
        filter_recorded("allowed", &events(), &decisions, "access")
            .unwrap()
            .matched,
        vec!["same-id"]
    );
    assert_eq!(
        filter_recorded("allowed", &events(), &decisions, "billing")
            .unwrap()
            .excluded,
        vec!["same-id"]
    );
    assert_eq!(
        filter_recorded("other", &events(), &decisions, "access")
            .unwrap()
            .unresolved,
        vec!["same-id"]
    );
    assert!(filter_recorded("allowed", &events(), &decisions, "unknown").is_err());
}

#[tokio::test]
async fn disabled_default_never_calls_transport() {
    let fake = fake(vec![]);
    let results = Adapter::default()
        .classify(&fake, "allowed", &events(), 1)
        .await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].outcome, Outcome::Pending(Failure::Disabled));
    assert!(fake.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn scope_and_allowlist_before_transport() {
    let fake = fake(vec![Ok(response(0.9))]);
    let results = Adapter { enabled: true }
        .classify(&fake, "allowed", &events(), 1)
        .await;
    assert_eq!(results[0].outcome, Outcome::Classified("access".into()));
    let requests = fake.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0]["state"],
        json!({"text":"Synthetic locked account"})
    );
    assert!(!requests[0].to_string().contains("secret"));
    assert!(!requests[0].to_string().contains("email"));
}

#[tokio::test(start_paused = true)]
async fn retries_are_bounded_and_only_for_rate_or_overload() {
    for status in [429, 529, 401, 422, 500] {
        let fake = fake(vec![Err(Failure::Http(status)), Err(Failure::Http(status))]);
        let result = Adapter { enabled: true }
            .classify(&fake, "allowed", &events(), 1)
            .await;
        assert_eq!(result[0].outcome, Outcome::Pending(Failure::Http(status)));
        assert_eq!(
            fake.requests.lock().unwrap().len(),
            if status == 429 || status == 529 { 2 } else { 1 }
        );
    }
}

#[tokio::test(start_paused = true)]
async fn timeout_aborts_without_retry() {
    let mut fake = fake(vec![Ok(response(0.9))]);
    fake.delay = true;
    let result = Adapter { enabled: true }
        .classify(&fake, "allowed", &events(), 1)
        .await;
    assert_eq!(result[0].outcome, Outcome::Pending(Failure::Timeout));
    assert_eq!(fake.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn abstention_and_transport_failure_remain_visible() {
    for (reply, expected) in [
        (Ok(response(0.3)), Outcome::Abstained),
        (
            Err(Failure::Transport),
            Outcome::Pending(Failure::Transport),
        ),
    ] {
        let fake = fake(vec![reply]);
        let results = Adapter { enabled: true }
            .classify(&fake, "allowed", &events(), 1)
            .await;
        assert_eq!(
            replay("allowed", &events(), &results).unwrap()[0].1,
            expected
        );
    }
}

#[tokio::test]
async fn malformed_answers_fail_closed() {
    let mut bad = vec![json!({})];
    for (field, value) in [
        ("type", json!("noul")),
        ("choice", json!("unknown")),
        ("confidence", json!(1.1)),
        ("probabilities", json!({"billing":0.3,"access":0.2})),
        ("probabilities", json!({"billing":0.9,"access":0.1})),
    ] {
        let mut reply = response(0.9);
        reply["answers"]["route"][field] = value;
        bad.push(reply);
    }
    for reply in bad {
        let fake = fake(vec![Ok(reply)]);
        let results = Adapter { enabled: true }
            .classify(&fake, "allowed", &events(), 1)
            .await;
        assert_eq!(
            results[0].outcome,
            Outcome::Pending(Failure::InvalidResponse)
        );
    }
}

#[tokio::test]
async fn replay_is_tenant_scoped_versioned_and_rejects_conflicts() {
    let fake = fake(vec![Ok(response(0.9))]);
    let decisions = Adapter { enabled: true }
        .classify(&fake, "allowed", &events(), 1)
        .await;
    let encoded = serde_json::to_string(&decisions).unwrap();
    let mut restored: Vec<Decision> = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        replay("allowed", &events(), &restored).unwrap()[0].1,
        Outcome::Classified("access".into())
    );
    assert_eq!(
        replay("other", &events(), &restored).unwrap()[0].1,
        Outcome::Pending(Failure::Disabled)
    );
    let mut next = restored[0].clone();
    next.revision = 2;
    next.outcome = Outcome::Abstained;
    restored.push(next.clone());
    assert_eq!(
        replay("allowed", &events(), &restored).unwrap()[0].1,
        Outcome::Abstained
    );
    assert_eq!(
        replay("allowed", &events(), &restored[..1]).unwrap()[0].1,
        Outcome::Classified("access".into())
    );
    next.outcome = Outcome::Classified("billing".into());
    restored.push(next);
    assert!(replay("allowed", &events(), &restored).is_err());
}
