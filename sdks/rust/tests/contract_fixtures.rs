//! The SDK against the shared latest-state fixtures in `sdks/contract/fixtures/`.
//!
//! Every `*.json` there is loaded, and a fixture with no case below fails the
//! run, so a fixture added for another SDK cannot be skipped here. Each case
//! answers the SDK's request with the fixture's response, then checks the
//! request the SDK sent and the value it returned (`sdks/contract/README.md`).

use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    time::{Duration, Instant},
};

use allsource::{
    ClientConfig, Conditional, EntityStateParams, Error, FeedCursor, FeedParams, LatestRow,
    QueryClient, QueryEventsParams, ReadModelCommit, ReadModelPageParams, SyncChange, SyncOptions,
    ViewChangesParams, ViewEntitiesParams,
};
use serde::Deserialize;
use serde_json::{json, Value};
use wiremock::{
    matchers::{any, method, path, query_param, query_param_is_missing},
    Mock, MockServer, Request, ResponseTemplate,
};

#[derive(Debug, Clone, Deserialize)]
struct Fixture {
    name: String,
    description: String,
    request: FixtureRequest,
    response: FixtureResponse,
}

#[derive(Debug, Clone, Deserialize)]
struct FixtureRequest {
    method: String,
    path: String,
    query: BTreeMap<String, String>,
    headers: BTreeMap<String, String>,
    body: Value,
}

#[derive(Debug, Clone, Deserialize)]
struct FixtureResponse {
    status: u16,
    headers: BTreeMap<String, String>,
    body: Value,
}

impl Fixture {
    fn response_header(&self, name: &str) -> Option<String> {
        self.response
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    }

    fn request_header(&self, name: &str) -> String {
        self.request
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map_or_else(
                || panic!("{}: no request header {name}", self.name),
                |(_, v)| v.clone(),
            )
    }

    fn template(&self) -> ResponseTemplate {
        let mut template = ResponseTemplate::new(self.response.status);
        for (name, value) in &self.response.headers {
            template = template.insert_header(name.as_str(), value.as_str());
        }
        if !self.response.body.is_null() {
            template = template.set_body_json(&self.response.body);
        }
        template
    }
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../contract/fixtures")
}

fn load_all() -> Vec<Fixture> {
    let dir = fixtures_dir();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no fixtures in {}", dir.display());
    paths
        .iter()
        .map(|path| {
            let text = std::fs::read_to_string(path).unwrap();
            let fixture: Fixture =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let stem = path.file_stem().unwrap().to_string_lossy();
            assert_eq!(fixture.name, stem, "fixture name must equal its file name");
            fixture
        })
        .collect()
}

fn load(name: &str) -> Fixture {
    load_all()
        .into_iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no fixture {name}"))
}

fn client(server: &MockServer) -> QueryClient {
    let mut config = ClientConfig::new(&server.uri(), "test-key");
    config.retry.base_delay = Duration::from_millis(1);
    QueryClient::with_config(config).unwrap()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap()
}

fn decoded_segments(path: &str) -> Vec<String> {
    path.split('/').map(percent_decode).collect()
}

fn query_of(request: &Request) -> BTreeMap<String, String> {
    request
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

/// Rules 1-4 of `sdks/contract/README.md` against the one request the SDK
/// sent.
fn assert_request(fixture: &Fixture, request: &Request) {
    let name = &fixture.name;
    assert_eq!(
        request.method.to_string(),
        fixture.request.method,
        "{name}: method"
    );
    assert_eq!(
        decoded_segments(request.url.path()),
        decoded_segments(&fixture.request.path),
        "{name}: path"
    );
    assert_eq!(
        query_of(request),
        fixture.request.query,
        "{name}: query (no description here allows extra parameters)"
    );
    for (header, expected) in &fixture.request.headers {
        let sent = request
            .headers
            .get(header.as_str())
            .and_then(|v| v.to_str().ok());
        assert_eq!(sent, Some(expected.as_str()), "{name}: header {header}");
    }
    if fixture.request.body.is_null() {
        assert!(request.body.is_empty(), "{name}: no body expected");
    } else {
        let sent: Value = serde_json::from_slice(&request.body)
            .unwrap_or_else(|e| panic!("{name}: body is not JSON: {e}"));
        assert_eq!(sent, fixture.request.body, "{name}: body");
    }
}

/// Serve `fixture.response` to whatever the SDK sends, run `call`, and check
/// that exactly one request matching the fixture went out.
async fn served<T, F, Fut>(fixture: &Fixture, call: F) -> T
where
    F: FnOnce(QueryClient) -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(fixture.template())
        .mount(&server)
        .await;
    let result = call(client(&server)).await;
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests.len(),
        1,
        "{}: exactly one request, never a retry",
        fixture.name
    );
    assert_request(fixture, &requests[0]);
    result
}

async fn run_case(fixture: &Fixture) {
    let etag = fixture.response_header("ETag");
    match fixture.name.as_str() {
        "entity_state" => {
            let state = served(fixture, |c| async move {
                c.entity_state("skill:abc", &EntityStateParams::new()).await
            })
            .await
            .unwrap()
            .modified()
            .expect("a 200 body");
            assert_eq!(state.entity_id, "skill:abc");
            assert_eq!(state.current_state["status"], "published");
            assert_eq!(state.version, Some(12));
            assert_eq!(state.event_count, 12);
            assert!(state.history.is_none());
            assert_eq!(state.etag, etag);
        }
        "entity_state_not_modified" => {
            let tag = fixture.request_header("If-None-Match");
            let result = served(fixture, |c| async move {
                c.entity_state("skill:abc", &EntityStateParams::new().if_none_match(&tag))
                    .await
            })
            .await
            .unwrap();
            match result {
                Conditional::NotModified { etag: got } => assert_eq!(got, etag),
                Conditional::Modified(state) => panic!("expected NotModified, got {state:?}"),
            }
        }
        "event_feed" => {
            let page = served(fixture, |c| async move {
                c.feed(
                    &FeedParams::new()
                        .cursor(FeedCursor {
                            epoch: 41,
                            position: 0,
                        })
                        .prefixes(&["skill."])
                        .limit(100),
                )
                .await
            })
            .await
            .unwrap();
            assert_eq!(page.tenant_id, "t1");
            assert_eq!(page.events.len(), 1);
            assert_eq!(page.events[0].event_type, "skill.published");
            assert!(page.caught_up);
            assert_eq!(
                page.head,
                FeedCursor {
                    epoch: 41,
                    position: 3
                }
            );
            assert_eq!(
                page.next_cursor(),
                FeedCursor {
                    epoch: 41,
                    position: 1
                }
            );
        }
        "event_feed_epoch_changed" => {
            let err = served(fixture, |c| async move {
                c.feed(&FeedParams::new().cursor(FeedCursor {
                    epoch: 40,
                    position: 17,
                }))
                .await
            })
            .await
            .unwrap_err();
            assert!(
                matches!(
                    err,
                    Error::FeedEpochChanged {
                        expected: 40,
                        current: 41
                    }
                ),
                "{err:?}"
            );
        }
        "events_query_with_entity_version" => {
            let resp = served(fixture, |c| async move {
                c.query_events(QueryEventsParams::new().entity_id("skill:abc").limit(1))
                    .await
            })
            .await
            .unwrap();
            assert_eq!(resp.entity_version, Some(12));
            assert_eq!(resp.events.len(), 1);
            assert_eq!(resp.has_more, Some(true));
            assert_eq!(resp.meta.etag, etag);
        }
        "rate_limited_retry_after" => {
            let err = served(fixture, |c| async move {
                c.view_changes("ops_status", &ViewChangesParams::after(61))
                    .await
            })
            .await
            .unwrap_err();
            assert!(
                matches!(
                    err,
                    Error::RateLimited {
                        retry_after: Some(d),
                        ..
                    } if d == Duration::from_secs(1)
                ),
                "{err:?}"
            );
        }
        "read_model_commit" => {
            let revision = served(fixture, |c| async move {
                c.read_model("skills")
                    .commit(
                        &ReadModelCommit::new(
                            7,
                            FeedCursor {
                                epoch: 41,
                                position: 3,
                            },
                        )
                        .upsert("skill:abc", json!({"status": "retired"}))
                        .delete("skill:old"),
                    )
                    .await
            })
            .await
            .unwrap();
            assert_eq!(revision, 8);
        }
        "read_model_commit_conflict" => {
            let err = served(fixture, |c| async move {
                c.read_model("skills")
                    .commit(
                        &ReadModelCommit::new(
                            6,
                            FeedCursor {
                                epoch: 41,
                                position: 3,
                            },
                        )
                        .upsert("skill:abc", json!({"status": "retired"})),
                    )
                    .await
            })
            .await
            .unwrap_err();
            assert!(
                matches!(
                    err,
                    Error::VersionConflict {
                        expected: 6,
                        current: 7
                    }
                ),
                "{err:?}"
            );
        }
        "read_model_page" => {
            let page = served(fixture, |c| async move {
                c.read_model("skills")
                    .page(&ReadModelPageParams::new().prefix("skill:").limit(100))
                    .await
            })
            .await
            .unwrap();
            assert_eq!(page.content_revision, 7);
            assert!(!page.unchanged);
            assert_eq!(page.rows["skill:abc"], json!({"status": "published"}));
            assert_eq!(
                page.checkpoint,
                Some(FeedCursor {
                    epoch: 41,
                    position: 3
                })
            );
            assert!(page.next.is_none());
        }
        "read_model_unchanged" => {
            let page = served(fixture, |c| async move {
                c.read_model("skills")
                    .page(&ReadModelPageParams::new().prefix("skill:").revision(7))
                    .await
            })
            .await
            .unwrap();
            assert!(page.unchanged);
            assert!(page.rows.is_empty());
        }
        "view_changes" => {
            let changes = served(fixture, |c| async move {
                c.view_changes("ops_status", &ViewChangesParams::after(61))
                    .await
            })
            .await
            .unwrap();
            assert_eq!(changes.revision, 62);
            assert_eq!(changes.changes.len(), 2);
            assert_eq!(
                changes.changes[0].state,
                Some(json!({"ok": 1431, "failed": 2}))
            );
            assert_eq!(changes.changes[1].key, "probe-z:2026-10-20");
            assert!(
                changes.changes[1].state.is_none(),
                "null state is a deletion"
            );
        }
        "view_changes_empty_after_wait" => {
            let changes = served(fixture, |c| async move {
                c.view_changes(
                    "ops_status",
                    &ViewChangesParams::after(62).wait(Duration::from_secs(20)),
                )
                .await
            })
            .await
            .unwrap();
            assert!(changes.changes.is_empty());
            assert!(changes.caught_up);
            assert_eq!(changes.revision, 62);
        }
        "view_changes_reset" => {
            let err = served(fixture, |c| async move {
                c.view_changes("ops_status", &ViewChangesParams::after(12))
                    .await
            })
            .await
            .unwrap_err();
            assert!(matches!(err, Error::ViewReset { generation: 3 }), "{err:?}");
        }
        "view_define" => {
            let definition = fixture.request.body.clone();
            let info = served(fixture, |c| async move {
                c.define_view("ops_status", &definition).await
            })
            .await
            .unwrap();
            assert_eq!(info.view, "ops_status");
            assert_eq!(info.generation, 1);
            assert_eq!(info.revision, 0);
            assert!(info.checkpoint.is_none());
            assert!(!info.ready);
            assert_eq!(info.definition, fixture.request.body);
        }
        "view_define_invalid" => {
            let definition = fixture.request.body.clone();
            let err = served(fixture, |c| async move {
                c.define_view("ops_status", &definition).await
            })
            .await
            .unwrap_err();
            match err {
                Error::InvalidView { detail } => {
                    assert_eq!(detail, "ops must contain 1..64 operations");
                }
                other => panic!("expected InvalidView, got {other:?}"),
            }
        }
        "view_entities_page_1" => {
            let page = served(fixture, |c| async move {
                c.latest_many("ops_status", ViewEntitiesParams::new().limit(2))
                    .next_page()
                    .await
            })
            .await
            .unwrap()
            .expect("a first page");
            assert_eq!(page.rows.len(), 2);
            assert_eq!(page.next.as_deref(), Some("probe-a:2026-10-27"));
            assert_eq!(page.revision, 61);
        }
        "view_entities_page_2" => {
            let page = served(fixture, |c| async move {
                c.view_entities(
                    "ops_status",
                    &ViewEntitiesParams::new()
                        .after("probe-a:2026-10-27")
                        .limit(2),
                )
                .await
            })
            .await
            .unwrap();
            assert_eq!(page.rows.len(), 1);
            assert!(page.next.is_none());
            assert_eq!(page.revision, 61);
        }
        "view_get" => {
            let info = served(fixture, |c| async move { c.get_view("ops_status").await })
                .await
                .unwrap();
            assert_eq!(info.generation, 2);
            assert_eq!(info.revision, 62);
            assert_eq!(
                info.checkpoint,
                Some(FeedCursor {
                    epoch: 41,
                    position: 9120
                })
            );
            assert!(info.ready);
            assert_eq!(info.definition["key"], "{payload.probe}:{day:timestamp}");
        }
        "view_latest" => {
            let row = served(fixture, |c| async move {
                c.latest("ops_status", "probe-a:2026-10-27", None).await
            })
            .await
            .unwrap();
            match row {
                LatestRow::Found(row) => {
                    assert_eq!(row.key, "probe-a:2026-10-27");
                    assert_eq!(row.state, json!({"ok": 1430, "failed": 2}));
                    assert_eq!(row.version, 57);
                    assert_eq!(row.etag, etag);
                }
                other => panic!("expected Found, got {other:?}"),
            }
        }
        "view_latest_not_found" => {
            let row = served(fixture, |c| async move {
                c.latest("ops_status", "probe-q:2026-10-27", None).await
            })
            .await
            .unwrap();
            assert!(
                matches!(row, LatestRow::NotFound { ready: true }),
                "{row:?}"
            );
        }
        "view_latest_not_modified" => {
            let tag = fixture.request_header("If-None-Match");
            let row = served(fixture, |c| async move {
                c.latest("ops_status", "probe-a:2026-10-27", Some(tag.as_str()))
                    .await
            })
            .await
            .unwrap();
            match row {
                LatestRow::NotModified { etag: got } => assert_eq!(got, etag),
                other => panic!("expected NotModified, got {other:?}"),
            }
        }
        "views_list" => {
            let views = served(fixture, |c| async move { c.list_views().await })
                .await
                .unwrap();
            let names: Vec<_> = views.iter().map(|v| v.view.as_str()).collect();
            assert_eq!(names, ["ops_status", "skill_catalog"]);
            assert!(views[0].ready);
            assert!(!views[1].ready);
            assert_eq!(views[0].revision, 62);
        }
        other => panic!(
            "fixture {other} has no case in tests/contract_fixtures.rs: {}",
            fixture.description
        ),
    }
}

#[tokio::test]
async fn every_fixture_is_consumed_and_honoured() {
    for fixture in load_all() {
        run_case(&fixture).await;
    }
}

async fn mount(server: &MockServer, fixture: &Fixture, mock: wiremock::MockBuilder) {
    mock.respond_with(fixture.template()).mount(server).await;
}

fn view_entities(server_path: &str) -> wiremock::MockBuilder {
    Mock::given(method("GET")).and(path(server_path))
}

const ENTITIES: &str = "/api/v1/views/ops_status/entities";
const CHANGES: &str = "/api/v1/views/ops_status/changes";

/// Fixture-only sync scenario, steps 1, 2, 3, 6 and 4 of
/// `sdks/contract/README.md`.
#[tokio::test]
async fn sync_snapshots_then_follows_changes_and_waits_out_a_429() {
    let server = MockServer::start().await;
    mount(
        &server,
        &load("view_entities_page_1"),
        view_entities(ENTITIES).and(query_param_is_missing("after")),
    )
    .await;
    mount(
        &server,
        &load("view_entities_page_2"),
        view_entities(ENTITIES).and(query_param("after", "probe-a:2026-10-27")),
    )
    .await;
    Mock::given(method("GET"))
        .and(path(CHANGES))
        .and(query_param("after", "61"))
        .respond_with(load("rate_limited_retry_after").template())
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    mount(
        &server,
        &load("view_changes"),
        Mock::given(method("GET"))
            .and(path(CHANGES))
            .and(query_param("after", "61")),
    )
    .await;
    Mock::given(method("GET"))
        .and(path(CHANGES))
        .and(query_param("after", "62"))
        .and(query_param("wait", "20"))
        .respond_with(
            load("view_changes_empty_after_wait")
                .template()
                .set_delay(Duration::from_millis(100)),
        )
        .mount(&server)
        .await;

    let qs = client(&server);
    let mut sync = qs.sync(
        "ops_status",
        SyncOptions::new()
            .page_size(2)
            .wait(Duration::from_secs(20)),
    );
    let mut rows: HashMap<String, (Value, u64)> = HashMap::new();
    let mut apply = |batch: allsource::SyncBatch| {
        if batch.reset {
            rows.clear();
        }
        for change in batch.changes {
            match change {
                SyncChange::Upsert {
                    key,
                    state,
                    version,
                } => {
                    rows.insert(key, (state, version));
                }
                SyncChange::Delete { key, .. } => {
                    rows.remove(&key);
                }
            }
        }
        batch.checkpoint
    };

    let first = sync.next().await.unwrap();
    assert!(first.reset && first.snapshot);
    assert_eq!(
        apply(first),
        None,
        "no checkpoint partway through the pages"
    );

    let second = sync.next().await.unwrap();
    assert!(!second.reset && second.snapshot);
    assert_eq!(apply(second), Some(61));

    let started = Instant::now();
    let third = sync.next().await.unwrap();
    assert!(
        started.elapsed() >= Duration::from_secs(1),
        "waited out Retry-After: 1"
    );
    assert!(!third.snapshot);
    assert_eq!(apply(third), Some(62));

    assert_eq!(rows.len(), 2);
    assert_eq!(rows["probe-a:2026-10-26"].1, 40);
    assert_eq!(
        rows["probe-a:2026-10-27"],
        (json!({"ok": 1431, "failed": 2}), 62)
    );
    assert!(!rows.contains_key("probe-z:2026-10-20"));

    assert!(
        tokio::time::timeout(Duration::from_millis(400), sync.next())
            .await
            .is_err(),
        "an empty long poll yields nothing"
    );
    assert_eq!(sync.checkpoint(), Some(62));

    let requests = server.received_requests().await.unwrap();
    let changes: Vec<BTreeMap<String, String>> = requests
        .iter()
        .filter(|r| r.url.path() == CHANGES)
        .map(query_of)
        .collect();
    let after_61: Vec<_> = changes.iter().filter(|q| q["after"] == "61").collect();
    assert_eq!(after_61.len(), 2, "the 429'd request is repeated once");
    assert!(
        after_61.iter().all(|q| !q.contains_key("wait")),
        "no wait before the first caught-up answer"
    );
    assert!(changes
        .iter()
        .any(|q| q.get("after").map(String::as_str) == Some("62")
            && q.get("wait").map(String::as_str) == Some("20")));
}

/// Step 5: a stale checkpoint is reset and the sync re-reads the pages.
#[tokio::test]
async fn sync_resnapshots_after_view_reset() {
    let server = MockServer::start().await;
    mount(
        &server,
        &load("view_changes_reset"),
        Mock::given(method("GET"))
            .and(path(CHANGES))
            .and(query_param("after", "12")),
    )
    .await;
    mount(
        &server,
        &load("view_entities_page_1"),
        view_entities(ENTITIES).and(query_param_is_missing("after")),
    )
    .await;
    mount(
        &server,
        &load("view_entities_page_2"),
        view_entities(ENTITIES).and(query_param("after", "probe-a:2026-10-27")),
    )
    .await;

    let qs = client(&server);
    let mut sync = qs.sync("ops_status", SyncOptions::new().checkpoint(12).page_size(2));
    let batch = sync.next().await.unwrap();
    assert!(batch.reset, "local rows must be discarded");
    assert!(batch.snapshot);
    let keys: Vec<_> = batch.changes.iter().map(SyncChange::key).collect();
    assert_eq!(keys, ["probe-a:2026-10-26", "probe-a:2026-10-27"]);
    assert_eq!(sync.checkpoint(), None);

    let last = sync.next().await.unwrap();
    assert_eq!(last.checkpoint, Some(61));
}
