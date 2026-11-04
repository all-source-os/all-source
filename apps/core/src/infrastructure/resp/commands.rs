//! Redis command handler for AllSource Core.
//!
//! Maps Redis Streams commands to EventStore operations:
//!
//! | Redis Command | AllSource Operation |
//! |---------------|---------------------|
//! | `XADD stream * field value ...` | `EventStore::ingest()` |
//! | `XRANGE stream - +` | `EventStore::query()` |
//! | `XLEN stream` | event count for entity |
//! | `SUBSCRIBE channel` | WebSocket broadcast subscribe |
//! | `PING` | health check |
//! | `COMMAND` / `COMMAND DOCS` | command metadata |
//! | `INFO` | server info |

use crate::{application::dto::QueryEventsRequest, domain::entities::Event, store::EventStore};
use std::sync::Arc;
use tokio::sync::broadcast;

use super::protocol::RespValue;

/// Subscription info returned by SUBSCRIBE command.
pub struct SubscriptionInfo {
    pub rx: broadcast::Receiver<Arc<Event>>,
    /// Event type prefix filters (e.g. `["scheduler.*"]`). Empty = all events.
    pub filters: Vec<String>,
}

/// Execute a parsed RESP command against the EventStore.
///
/// Returns `(response, Option<subscription>)` — subscription is `Some` only for
/// SUBSCRIBE, giving the caller a broadcast receiver and filters.
pub fn execute(
    args: &[RespValue],
    store: &Arc<EventStore>,
) -> (RespValue, Option<SubscriptionInfo>) {
    if args.is_empty() {
        return (RespValue::err("empty command"), None);
    }

    let Some(s) = args[0].as_str() else {
        return (RespValue::err("invalid command"), None);
    };
    let cmd = s.to_ascii_uppercase();

    match cmd.as_str() {
        "PING" => handle_ping(&args[1..]),
        "XADD" => handle_xadd(&args[1..], store),
        "XRANGE" => handle_xrange(&args[1..], store),
        "XLEN" => handle_xlen(&args[1..], store),
        "SUBSCRIBE" => handle_subscribe(&args[1..], store),
        "COMMAND" => handle_command(&args[1..]),
        "INFO" => handle_info(store),
        "QUIT" => (RespValue::ok(), None),
        _ => (RespValue::err(format!("unknown command '{cmd}'")), None),
    }
}

// ── PING ────────────────────────────────────────────────────────────────────

fn handle_ping(args: &[RespValue]) -> (RespValue, Option<SubscriptionInfo>) {
    if let Some(msg) = args.first().and_then(|v| v.as_str()) {
        (RespValue::bulk_string(msg), None)
    } else {
        (RespValue::SimpleString("PONG".to_string()), None)
    }
}

// ── XADD ────────────────────────────────────────────────────────────────────
// Usage: XADD <stream_key> * event_type <type> entity_id <id> [payload <json>] [metadata <json>]
//
// The stream_key is treated as the tenant_id. The `*` auto-generates an ID.
// Field-value pairs must include `event_type` and `entity_id`.

fn handle_xadd(
    args: &[RespValue],
    store: &Arc<EventStore>,
) -> (RespValue, Option<SubscriptionInfo>) {
    // Minimum: stream_key, id_arg, field, value (4 args for one field pair — but we need 2 fields minimum)
    if args.len() < 6 {
        return (
            RespValue::err(
                "wrong number of arguments for 'XADD' command. Usage: XADD <tenant> * event_type <type> entity_id <id> [payload <json>] [metadata <json>]",
            ),
            None,
        );
    }

    let Some(s) = args[0].as_str() else {
        return (RespValue::err("invalid stream key"), None);
    };
    let tenant_id = s.to_string();

    // args[1] should be "*" or an explicit ID (we only support "*")
    match args[1].as_str() {
        Some("*") => {}
        _ => {
            return (
                RespValue::err("only '*' auto-ID is supported for XADD"),
                None,
            );
        }
    }

    // Parse field-value pairs
    let pairs = &args[2..];
    if !pairs.len().is_multiple_of(2) {
        return (RespValue::err("odd number of field-value pairs"), None);
    }

    let mut event_type: Option<String> = None;
    let mut entity_id: Option<String> = None;
    let mut payload: serde_json::Value = serde_json::json!({});
    let mut metadata: Option<serde_json::Value> = None;

    for chunk in pairs.chunks(2) {
        let Some(field) = chunk[0].as_str() else {
            return (RespValue::err("field name must be a string"), None);
        };
        let Some(value) = chunk[1].as_str() else {
            return (RespValue::err("field value must be a string"), None);
        };

        match field {
            "event_type" => event_type = Some(value.to_string()),
            "entity_id" => entity_id = Some(value.to_string()),
            "payload" => {
                payload = serde_json::from_str(value).unwrap_or_else(|_| {
                    // If not valid JSON, wrap as a string value
                    serde_json::Value::String(value.to_string())
                });
            }
            "metadata" => {
                metadata = Some(
                    serde_json::from_str(value)
                        .unwrap_or_else(|_| serde_json::Value::String(value.to_string())),
                );
            }
            _ => {
                // Unknown fields go into payload
                if let serde_json::Value::Object(ref mut map) = payload {
                    map.insert(
                        field.to_string(),
                        serde_json::Value::String(value.to_string()),
                    );
                }
            }
        }
    }

    let Some(event_type) = event_type else {
        return (RespValue::err("missing required field 'event_type'"), None);
    };
    let Some(entity_id) = entity_id else {
        return (RespValue::err("missing required field 'entity_id'"), None);
    };

    // Create and ingest the event
    let event = match Event::from_strings(event_type, entity_id, tenant_id, payload, metadata) {
        Ok(e) => e,
        Err(e) => return (RespValue::err(format!("event creation failed: {e}")), None),
    };

    let event_id = event.id.to_string();
    let timestamp = event.timestamp.timestamp_millis();

    match store.ingest(&event) {
        Ok(()) => {
            // Return a Redis-style stream ID: "<timestamp>-0"
            let stream_id = format!("{timestamp}-0");
            (RespValue::bulk_string(&stream_id), None)
        }
        Err(e) => (RespValue::err(format!("ingest failed: {e}")), None),
    }
}

// ── XRANGE ──────────────────────────────────────────────────────────────────
// Usage: XRANGE <stream_key> <start> <end> [COUNT <n>]
//
// stream_key = tenant_id (or "entity:<id>" / "type:<type>" for filtered queries)
// start/end: "-"/"+", or timestamps in milliseconds

fn handle_xrange(
    args: &[RespValue],
    store: &Arc<EventStore>,
) -> (RespValue, Option<SubscriptionInfo>) {
    if args.len() < 3 {
        return (
            RespValue::err("wrong number of arguments for 'XRANGE' command"),
            None,
        );
    }

    let Some(stream_key) = args[0].as_str() else {
        return (RespValue::err("invalid stream key"), None);
    };
    let Some(_start) = args[1].as_str() else {
        return (RespValue::err("invalid start"), None);
    };
    let Some(_end) = args[2].as_str() else {
        return (RespValue::err("invalid end"), None);
    };

    // Parse optional COUNT
    let mut limit: Option<usize> = None;
    let mut i = 3;
    while i < args.len() {
        if let Some(kw) = args[i].as_str()
            && kw.eq_ignore_ascii_case("COUNT")
            && i + 1 < args.len()
        {
            if let Some(n) = args[i + 1].as_str().and_then(|s| s.parse::<usize>().ok()) {
                limit = Some(n);
            }
            i += 2;
            continue;
        }
        i += 1;
    }

    // Parse stream_key into query filters
    let mut request = QueryEventsRequest {
        as_of_valid: None,
        since_valid: None,
        until_valid: None,
        entity_id: None,
        event_type: None,
        tenant_id: None,
        as_of: None,
        since: None,
        until: None,
        limit,
        event_type_prefix: None,
        exclude_event_type_prefix: None,
        payload_filter: None,
    };

    if let Some(rest) = stream_key.strip_prefix("entity:") {
        request.entity_id = Some(rest.to_string());
    } else if let Some(rest) = stream_key.strip_prefix("type:") {
        request.event_type = Some(rest.to_string());
    } else {
        request.tenant_id = Some(stream_key.to_string());
    }

    // Parse time range (start/end can be "-"/"+", or millisecond timestamps)
    if _start != "-"
        && let Ok(ms) = _start.split('-').next().unwrap_or(_start).parse::<i64>()
    {
        request.since = chrono::DateTime::from_timestamp_millis(ms);
    }
    if _end != "+"
        && let Ok(ms) = _end.split('-').next().unwrap_or(_end).parse::<i64>()
    {
        request.until = chrono::DateTime::from_timestamp_millis(ms);
    }

    match store.query(&request) {
        Ok(events) => {
            // Return as array of [stream_id, [field, value, ...]] pairs (Redis XRANGE format)
            let entries: Vec<RespValue> = events
                .iter()
                .map(|e| {
                    let stream_id = format!("{}-0", e.timestamp.timestamp_millis());
                    let fields = vec![
                        RespValue::bulk_string("event_id"),
                        RespValue::bulk_string(&e.id.to_string()),
                        RespValue::bulk_string("event_type"),
                        RespValue::bulk_string(e.event_type_str()),
                        RespValue::bulk_string("entity_id"),
                        RespValue::bulk_string(e.entity_id_str()),
                        RespValue::bulk_string("tenant_id"),
                        RespValue::bulk_string(e.tenant_id_str()),
                        RespValue::bulk_string("payload"),
                        RespValue::bulk_string(&e.payload.to_string()),
                        RespValue::bulk_string("timestamp"),
                        RespValue::bulk_string(&e.timestamp.to_rfc3339()),
                    ];
                    RespValue::Array(vec![
                        RespValue::bulk_string(&stream_id),
                        RespValue::Array(fields),
                    ])
                })
                .collect();
            (RespValue::Array(entries), None)
        }
        Err(e) => (RespValue::err(format!("query failed: {e}")), None),
    }
}

// ── XLEN ────────────────────────────────────────────────────────────────────
// Usage: XLEN <stream_key>

fn handle_xlen(
    args: &[RespValue],
    store: &Arc<EventStore>,
) -> (RespValue, Option<SubscriptionInfo>) {
    if args.is_empty() {
        return (
            RespValue::err("wrong number of arguments for 'XLEN' command"),
            None,
        );
    }

    let Some(stream_key) = args[0].as_str() else {
        return (RespValue::err("invalid stream key"), None);
    };

    // Count the stream's events WITHOUT materializing them. This used to run
    // `store.query(&request)` with `limit: None` and return `events.len()` —
    // cloning an entity's entire history to produce a single integer, the same
    // count-by-full-materialization shape as issue #251's `/events/query` bug.
    // `limit: Some(0)` plus `query_window`'s pre-window `total` gives the count
    // with an empty window, so XLEN clones nothing.
    let mut request = QueryEventsRequest {
        as_of_valid: None,
        since_valid: None,
        until_valid: None,
        entity_id: None,
        event_type: None,
        tenant_id: None,
        as_of: None,
        since: None,
        until: None,
        limit: Some(0),
        event_type_prefix: None,
        exclude_event_type_prefix: None,
        payload_filter: None,
    };
    if let Some(rest) = stream_key.strip_prefix("entity:") {
        request.entity_id = Some(rest.to_string());
    } else if let Some(rest) = stream_key.strip_prefix("type:") {
        request.event_type = Some(rest.to_string());
    } else {
        request.tenant_id = Some(stream_key.to_string());
    }

    match store.query_window(&request, 0, false) {
        Ok((_, total)) => (RespValue::Integer(total as i64), None),
        Err(e) => (RespValue::err(format!("query failed: {e}")), None),
    }
}

// ── SUBSCRIBE ───────────────────────────────────────────────────────────────
// Usage: SUBSCRIBE <pattern> [<pattern> ...]
//
// Subscribes to real-time event broadcasts with server-side prefix filtering.
// Patterns use `prefix.*` syntax (e.g. `scheduler.*` matches `scheduler.started`).
// `SUBSCRIBE *` receives all events (backwards compatible).

fn handle_subscribe(
    args: &[RespValue],
    store: &Arc<EventStore>,
) -> (RespValue, Option<SubscriptionInfo>) {
    if args.is_empty() {
        return (
            RespValue::err("wrong number of arguments for 'SUBSCRIBE' command"),
            None,
        );
    }

    let ws_manager = store.websocket_manager();

    // Get a broadcast receiver from the WebSocket manager's event channel
    let rx = ws_manager.subscribe_events();

    // Collect channel patterns as prefix filters
    let filters: Vec<String> = args
        .iter()
        .filter_map(|a| a.as_str().map(String::from))
        .filter(|f| f != "*") // "*" means all events — no filter needed
        .collect();

    // Build subscription confirmation per Redis protocol (one per channel)
    let mut confirmations = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        let channel = arg.as_str().unwrap_or("unknown");
        confirmations.push(RespValue::Array(vec![
            RespValue::bulk_string("subscribe"),
            RespValue::bulk_string(channel),
            RespValue::Integer((i + 1) as i64),
        ]));
    }

    let sub_info = SubscriptionInfo { rx, filters };

    // Return the first confirmation; the server loop will send the rest
    // and then enter subscription mode
    if confirmations.len() == 1 {
        (confirmations.into_iter().next().unwrap(), Some(sub_info))
    } else {
        // For multiple channels, wrap all confirmations
        // The server loop handles writing each one
        (RespValue::Array(confirmations), Some(sub_info))
    }
}

// ── COMMAND ─────────────────────────────────────────────────────────────────

fn handle_command(args: &[RespValue]) -> (RespValue, Option<SubscriptionInfo>) {
    // redis-cli sends `COMMAND DOCS` on connect — return empty array
    (RespValue::Array(vec![]), None)
}

// ── INFO ────────────────────────────────────────────────────────────────────

fn handle_info(store: &Arc<EventStore>) -> (RespValue, Option<SubscriptionInfo>) {
    let info = format!(
        "# Server\r\n\
         redis_version:7.0.0-allsource\r\n\
         allsource_version:{}\r\n\
         # Keyspace\r\n",
        env!("CARGO_PKG_VERSION"),
    );
    (RespValue::bulk_string(&info), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store() -> Arc<EventStore> {
        Arc::new(EventStore::new())
    }

    fn cmd(parts: &[&str]) -> Vec<RespValue> {
        parts.iter().map(|s| RespValue::bulk_string(s)).collect()
    }

    #[test]
    fn test_ping() {
        let store = make_store();
        let (resp, sub) = execute(&cmd(&["PING"]), &store);
        assert_eq!(resp, RespValue::SimpleString("PONG".to_string()));
        assert!(sub.is_none());
    }

    #[test]
    fn test_ping_with_message() {
        let store = make_store();
        let (resp, _) = execute(&cmd(&["PING", "hello"]), &store);
        assert_eq!(resp, RespValue::bulk_string("hello"));
    }

    #[test]
    fn test_xadd_and_xrange() {
        let store = make_store();

        // XADD
        let (resp, _) = execute(
            &cmd(&[
                "XADD",
                "default",
                "*",
                "event_type",
                "user.created",
                "entity_id",
                "user-1",
                "payload",
                r#"{"name":"Alice"}"#,
            ]),
            &store,
        );
        // Should return a stream ID like "<timestamp>-0"
        assert!(resp.as_str().unwrap().ends_with("-0"), "got: {resp:?}");

        // XRANGE
        let (resp, _) = execute(&cmd(&["XRANGE", "default", "-", "+"]), &store);
        match resp {
            RespValue::Array(entries) => {
                assert_eq!(entries.len(), 1);
                // Each entry is [stream_id, [field, value, ...]]
                if let RespValue::Array(ref inner) = entries[0] {
                    assert_eq!(inner.len(), 2);
                    // Check the fields array
                    if let RespValue::Array(ref fields) = inner[1] {
                        // Find event_type field
                        let et_idx = fields
                            .iter()
                            .position(|f| f.as_str() == Some("event_type"))
                            .unwrap();
                        assert_eq!(fields[et_idx + 1].as_str(), Some("user.created"));
                    }
                }
            }
            _ => panic!("expected array, got {resp:?}"),
        }
    }

    #[test]
    fn test_xadd_missing_fields() {
        let store = make_store();
        let (resp, _) = execute(&cmd(&["XADD", "default", "*"]), &store);
        match resp {
            RespValue::Error(_) => {}
            _ => panic!("expected error"),
        }
    }

    #[test]
    fn test_xlen() {
        let store = make_store();

        // Ingest 3 events
        for i in 0..3 {
            execute(
                &cmd(&[
                    "XADD",
                    "default",
                    "*",
                    "event_type",
                    "test.event",
                    "entity_id",
                    &format!("entity-{i}"),
                ]),
                &store,
            );
        }

        let (resp, _) = execute(&cmd(&["XLEN", "default"]), &store);
        assert_eq!(resp, RespValue::Integer(3));
    }

    // Regression guard for issue #251's second site. `XLEN` answers with an
    // integer, so nothing in its response can reveal that it used to build that
    // integer by cloning every matching event (`store.query` with `limit:
    // None`, then `events.len()`) — a 100k-event stream cost 100k `Event`
    // clones to say "100000". Counts materialized events directly
    // (`crate::clone_probe`), so restoring the `store.query(...).len()` shape
    // fails this test while `test_xlen` above stays green.
    #[test]
    fn xlen_counts_without_materializing_the_stream() {
        const HISTORY: usize = 200;
        let store = make_store();
        for _ in 0..HISTORY {
            execute(
                &cmd(&[
                    "XADD",
                    "default",
                    "*",
                    "event_type",
                    "test.event",
                    "entity_id",
                    "entity-hot",
                ]),
                &store,
            );
        }

        // Each addressing form XLEN supports: tenant, entity: and type:.
        for key in ["default", "entity:entity-hot", "type:test.event"] {
            let ((resp, _), materialized) =
                crate::clone_probe::measure(|| execute(&cmd(&["XLEN", key]), &store));
            assert_eq!(
                resp,
                RespValue::Integer(HISTORY as i64),
                "XLEN {key} must still report the true length"
            );
            assert_eq!(
                materialized, 0,
                "XLEN {key} cloned {materialized} events to return an integer — \
                 counting must not materialize the stream"
            );
        }
    }

    #[test]
    fn test_xrange_with_count() {
        let store = make_store();

        for i in 0..5 {
            execute(
                &cmd(&[
                    "XADD",
                    "default",
                    "*",
                    "event_type",
                    "test.event",
                    "entity_id",
                    &format!("entity-{i}"),
                ]),
                &store,
            );
        }

        let (resp, _) = execute(&cmd(&["XRANGE", "default", "-", "+", "COUNT", "2"]), &store);
        match resp {
            RespValue::Array(entries) => assert_eq!(entries.len(), 2),
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn test_xrange_entity_filter() {
        let store = make_store();

        execute(
            &cmd(&[
                "XADD",
                "default",
                "*",
                "event_type",
                "user.created",
                "entity_id",
                "user-1",
            ]),
            &store,
        );
        execute(
            &cmd(&[
                "XADD",
                "default",
                "*",
                "event_type",
                "order.placed",
                "entity_id",
                "order-1",
            ]),
            &store,
        );

        // Query by entity
        let (resp, _) = execute(&cmd(&["XRANGE", "entity:user-1", "-", "+"]), &store);
        match resp {
            RespValue::Array(entries) => assert_eq!(entries.len(), 1),
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn test_subscribe() {
        let store = make_store();
        let (resp, sub) = execute(&cmd(&["SUBSCRIBE", "events"]), &store);
        assert!(sub.is_some(), "subscribe should return a receiver");
        // Confirmation message
        match resp {
            RespValue::Array(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0].as_str(), Some("subscribe"));
                assert_eq!(items[1].as_str(), Some("events"));
            }
            _ => panic!("expected array confirmation"),
        }
    }

    #[test]
    fn test_subscribe_with_prefix_filters() {
        let store = make_store();
        let (_, sub) = execute(&cmd(&["SUBSCRIBE", "scheduler.*", "index.*"]), &store);
        let sub_info = sub.unwrap();
        assert_eq!(sub_info.filters, vec!["scheduler.*", "index.*"]);
    }

    #[test]
    fn test_subscribe_wildcard_has_no_filters() {
        let store = make_store();
        let (_, sub) = execute(&cmd(&["SUBSCRIBE", "*"]), &store);
        let sub_info = sub.unwrap();
        assert!(
            sub_info.filters.is_empty(),
            "wildcard should produce no filters"
        );
    }

    #[test]
    fn test_unknown_command() {
        let store = make_store();
        let (resp, _) = execute(&cmd(&["FLUSHALL"]), &store);
        match resp {
            RespValue::Error(e) => assert!(e.contains("unknown command")),
            _ => panic!("expected error"),
        }
    }

    #[test]
    fn test_info() {
        let store = make_store();
        let (resp, _) = execute(&cmd(&["INFO"]), &store);
        let s = resp.as_str().unwrap();
        assert!(s.contains("allsource_version"));
    }

    #[test]
    fn test_xadd_extra_fields_go_to_payload() {
        let store = make_store();
        let (resp, _) = execute(
            &cmd(&[
                "XADD",
                "default",
                "*",
                "event_type",
                "user.created",
                "entity_id",
                "user-1",
                "name",
                "Alice",
                "age",
                "30",
            ]),
            &store,
        );
        assert!(resp.as_str().unwrap().ends_with("-0"));

        // Query and check payload contains extra fields
        let (resp, _) = execute(&cmd(&["XRANGE", "default", "-", "+"]), &store);
        if let RespValue::Array(entries) = resp
            && let RespValue::Array(ref inner) = entries[0]
            && let RespValue::Array(ref fields) = inner[1]
        {
            let payload_idx = fields
                .iter()
                .position(|f| f.as_str() == Some("payload"))
                .unwrap();
            let payload_str = fields[payload_idx + 1].as_str().unwrap();
            let payload: serde_json::Value = serde_json::from_str(payload_str).unwrap();
            assert_eq!(payload["name"], "Alice");
            assert_eq!(payload["age"], "30");
        }
    }
}
