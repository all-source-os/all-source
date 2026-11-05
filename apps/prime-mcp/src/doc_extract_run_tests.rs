use allsource_core::{
    embedded::{EventView, Query},
    prime::types::event_types,
};
use serde_json::Value;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path as mpath},
};

use super::*;

fn llm_reply(content: &str) -> Value {
    json!({
        "choices": [{ "message": { "content": content } }],
        "usage": { "prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15 }
    })
}

const TWO_ENTITIES: &str = "{\"entities\":[{\"name\":\"Billing\",\"type\":\"service\",\"confidence\":0.9},{\"name\":\"Queue\",\"type\":\"component\"}],\"relationships\":[{\"from\":\"Billing\",\"to\":\"Queue\",\"relation\":\"depends_on\",\"confidence\":0.7}]}";

fn cfg(server: &MockServer) -> LlmConfig {
    LlmConfig {
        endpoint: format!("{}/v1/chat/completions", server.uri()),
        api_key: None,
        model: "run-model".into(),
    }
}

async fn events(prime: &Prime, event_type: &str) -> Vec<EventView> {
    prime
        .core()
        .query(Query::new().event_type(event_type))
        .await
        .unwrap()
}

fn assertion_of(e: &EventView) -> &Value {
    &e.metadata.as_ref().expect("write metadata")["assertion"]
}

#[tokio::test]
async fn an_extraction_run_is_recorded_started_then_completed_and_stamps_every_fact() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(mpath("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(llm_reply(TWO_ENTITIES)))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("design.md"), "Billing depends on Queue.").unwrap();
    let prime = Prime::open_in_memory().await.unwrap();

    let s = extract_docs_with(&prime, dir.path(), false, &cfg(&server), None)
        .await
        .unwrap();
    assert!(s.run_id.starts_with("extraction-"));

    let runs = events(&prime, RUN_EVENT).await;
    let statuses: Vec<&str> = runs
        .iter()
        .map(|e| e.payload["status"].as_str().unwrap())
        .collect();
    assert_eq!(statuses, ["started", "completed"]);
    for run in &runs {
        assert_eq!(run.entity_id, format!("extraction_run:{}", s.run_id));
        assert_eq!(run.payload["run_id"], json!(s.run_id));
        assert_eq!(run.payload["model"], "run-model");
        assert_eq!(
            run.payload["input_source"],
            json!(dir.path().display().to_string())
        );
        assert_eq!(run.payload["prompt_digest"], json!(prompt_digest()));
    }
    assert!(prompt_digest().starts_with("sha256:"));
    assert_eq!(runs[1].payload["usage"]["total_tokens"], 15);
    assert_eq!(runs[1].payload["entities"], 2);

    let nodes = events(&prime, event_types::NODE_CREATED).await;
    let edges = events(&prime, event_types::EDGE_CREATED).await;
    assert_eq!(nodes.len(), 3);
    assert_eq!(edges.len(), 3);
    for e in nodes.iter().chain(edges.iter()) {
        let a = assertion_of(e);
        assert_eq!(a["source"], "extraction");
        assert_eq!(a["run_id"], json!(s.run_id));
        assert_eq!(a["model"], "run-model");
        assert_eq!(a["derived_from"], json!(["design.md"]));
        assert_eq!(e.metadata.as_ref().unwrap()["source_priority"], 0);
    }

    let confidence_of_node = |name: &str| {
        nodes
            .iter()
            .find(|e| e.payload["properties"]["name"] == name)
            .map(|e| assertion_of(e).get("confidence").cloned())
            .unwrap()
    };
    assert_eq!(confidence_of_node("Billing"), Some(json!(0.9)));
    assert_eq!(confidence_of_node("Queue"), None);
    let depends_on = edges
        .iter()
        .find(|e| e.payload["relation"] == "depends_on")
        .unwrap();
    assert_eq!(assertion_of(depends_on)["confidence"], 0.7);
    let mentions = edges
        .iter()
        .find(|e| e.payload["relation"] == "mentions")
        .unwrap();
    assert!(assertion_of(mentions).get("confidence").is_none());
}

#[tokio::test]
async fn a_failed_extraction_marks_the_run_failed_and_keeps_written_facts_attributable() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(mpath("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(llm_reply(TWO_ENTITIES)))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(mpath("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("model overloaded"))
        .with_priority(2)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.md"), "Billing depends on Queue.").unwrap();
    std::fs::write(dir.path().join("b.md"), "More prose.").unwrap();
    let prime = Prime::open_in_memory().await.unwrap();

    let err = extract_docs_with(&prime, dir.path(), false, &cfg(&server), None)
        .await
        .unwrap_err();
    let msg = format!("{err:#}");

    let runs = events(&prime, RUN_EVENT).await;
    let statuses: Vec<&str> = runs
        .iter()
        .map(|e| e.payload["status"].as_str().unwrap())
        .collect();
    assert_eq!(statuses, ["started", "failed"]);
    let run_id = runs[0].payload["run_id"].as_str().unwrap().to_string();
    assert!(msg.contains(&run_id), "{msg}");
    assert!(
        runs[1].payload["error"]
            .as_str()
            .unwrap()
            .contains("model overloaded")
    );
    assert_eq!(runs[1].payload["usage"]["llm_calls"], 1);

    let nodes = events(&prime, event_types::NODE_CREATED).await;
    assert_eq!(nodes.len(), 3, "facts from the first document stay written");
    for e in &nodes {
        assert_eq!(assertion_of(e)["run_id"], json!(run_id));
    }
    assert!(
        events(&prime, "prime.extraction.usage").await.is_empty(),
        "usage is metered on completed runs only, as before"
    );
}

#[tokio::test]
async fn each_extraction_gets_its_own_run_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(mpath("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(llm_reply(TWO_ENTITIES)))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("design.md"), "Billing depends on Queue.").unwrap();
    let prime = Prime::open_in_memory().await.unwrap();

    let first = extract_docs_with(&prime, dir.path(), false, &cfg(&server), None)
        .await
        .unwrap();
    let second = extract_docs_with(&prime, dir.path(), false, &cfg(&server), None)
        .await
        .unwrap();
    assert_ne!(first.run_id, second.run_id);
    assert_eq!(events(&prime, RUN_EVENT).await.len(), 4);
}

#[test]
fn out_of_range_extractor_confidence_is_not_stamped() {
    let run = ExtractionRun::new("m", Path::new("docs"));
    assert_eq!(run.assertion("a.md", Some(0.5)).confidence, Some(0.5));
    assert_eq!(run.assertion("a.md", Some(1.5)).confidence, None);
    assert_eq!(run.assertion("a.md", Some(f64::NAN)).confidence, None);
    assert_eq!(run.assertion("a.md", None).confidence, None);
}
