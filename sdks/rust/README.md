# AllSource Rust Client

Typed Rust client for the [AllSource](https://github.com/all-source-os/all-source) event store — query, ingest, fold events, and build custom projections.

## Install

```toml
[dependencies]
allsource = "0.26"
```

## Which client do I want?

AllSource has **one public front door** (the gateway at `https://api.all-source.xyz` for our hosted offering, or your self-hosted equivalent) and **one internal fast path** (Core, reachable only on your network). The SDK exposes a client for each. Pick based on where your caller runs:

| Caller location | Use | URL shape | Why |
|---|---|---|---|
| Inside your network, same cluster/VPC as Core | `CoreClient` | `http://core.allsource.svc.cluster.local:3900` or internal DNS equivalent | One hop, ~μs key validation, no gateway overhead |
| Outside your network (public API, mobile, third-party) | `QueryClient` | `https://api.all-source.xyz` | Gateway enforces rate limits, quotas, billing |
| Mixed — internal workers + external customers | Both | internal + public URLs | Each for what it's good at |

**Core should never be on public DNS** — it's trust-the-caller by design. If you need internal services to reach Core from outside your network, use a VPN / Tailscale / bastion; don't publish `core.*` records.

```rust
use allsource::{QueryClient, CoreClient, IngestEventInput, QueryEventsParams};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), allsource::Error> {
    let core = CoreClient::new("http://localhost:3900", "your-api-key")?;
    let query = QueryClient::new("http://localhost:3902", "your-api-key")?;

    core.ingest_event(IngestEventInput::new(
        "user.signup",
        "user-123",
        json!({"email": "alice@example.com"}),
    )).await?;

    let events = query.query_events(
        QueryEventsParams::new().entity_id("user-123").limit(10),
    ).await?;
    println!("Found {} events", events.count);

    Ok(())
}
```

See the [connection-path guide](https://all-source.xyz/blog/connection-path) for a deeper treatment of the direct-vs-gateway tradeoff, including latency numbers and when each pattern is the wrong default.

### Paginating large result sets

`query_events` and `list_entities` page with `limit`/`offset` and return a `has_more` flag. Rather than hand-rolling offset arithmetic — and risking a silent truncation at the first page — let the SDK drive the loop:

```rust
use allsource::{QueryClient, QueryEventsParams};

async fn process_all(query: &QueryClient) -> Result<(), allsource::Error> {
    // Page through, one batch at a time:
    let mut pages = query.query_events_paged(
        QueryEventsParams::new().event_type_prefix("auth.org.").limit(500),
    );
    while let Some(events) = pages.next_page().await? {
        println!("page of {} events", events.len());
    }

    // …or drain everything into one Vec:
    let all = query
        .query_events_paged(QueryEventsParams::new().event_type_prefix("auth.org."))
        .collect_all()
        .await?;
    println!("{} events total", all.len());
    Ok(())
}
```

Results are ordered — `query_events` by `(timestamp, version)`, `list_entities` by last-event time with an `entity_id` tie-break — so paging is stable: no row is skipped or repeated across pages. Pass `.order(SortOrder::Desc)` for newest-first. `list_entities_paged` works the same way over `ListEntitiesParams`.

## Latest state without folding history

The calls below implement the [latest-state contract](../../docs/contracts/latest-state/README.md) and are on both `QueryClient` and `CoreClient`.

```rust,no_run
use allsource::{EntityStateParams, LatestRow, QueryClient, SyncChange, SyncOptions, ViewEntitiesParams};
use serde_json::json;
use std::time::Duration;

async fn run(qs: &QueryClient) -> Result<(), allsource::Error> {
    // One entity's current state, no history. Send the ETag back to get a 304.
    let state = qs.entity_state("skill:abc", &EntityStateParams::new()).await?;

    // A view: a declarative fold the Query Service keeps current.
    qs.define_view("ops_status", &json!({
        "source": { "prefixes": ["ops.probe."] },
        "key": "{payload.probe}:{day:timestamp}",
        "ops": [{ "on": "ops.probe.observed", "increment": { "seen": 1 } }]
    })).await?;

    match qs.latest("ops_status", "probe-a:2026-10-27", None).await? {
        LatestRow::Found(row) => println!("{} at version {}", row.state, row.version),
        LatestRow::NotFound { ready } => println!("no row yet (view ready: {ready})"),
        LatestRow::NotModified { .. } => {}
    }

    let all = qs.latest_many("ops_status", ViewEntitiesParams::new()).collect_all().await?;
    println!("{} rows", all.len());

    // Keep a local copy current: entity pages, then long-polled changes.
    let mut sync = qs.sync("ops_status", SyncOptions::new().wait(Duration::from_secs(20)));
    let batch = sync.next().await?;
    for change in batch.changes {
        match change {
            SyncChange::Upsert { key, .. } => println!("upsert {key}"),
            SyncChange::Delete { key, .. } => println!("delete {key}"),
        }
    }
    // Store batch.checkpoint; pass it to SyncOptions::checkpoint on restart.
    let _ = state;
    Ok(())
}
```

`sync` re-reads the entity pages (with `batch.reset == true`) when the view was reset past your checkpoint, and waits out `Retry-After` on 429 and 503. `feed(...)` and `read_model(name).page(...)` / `.commit(...)` expose the event feed and read-model protocol a custom folder needs.

## Performance cheatsheet

The SDK defaults are tuned for typical use. A few things worth knowing:

- **`.clone()` is cheap.** Both `CoreClient` and `QueryClient` wrap a `reqwest::Client` in `Arc`; cloning shares the connection pool. Don't wrap them in your own `Arc<Mutex<...>>` — you'll serialize requests unnecessarily.
- **Batch writes.** `core.ingest_batch(Vec<IngestEventInput>)` is one round-trip for N events. Looping `ingest_event` is N round-trips.
- **Use `ProjectionWorker` for long-lived read models** instead of polling `QueryClient::query_events`. The worker holds a WebSocket + Core-tracked cursor, so cold starts are O(events-since-last-ack) rather than O(total-events).
- **Bootstrap the API key on Core** via `ALLSOURCE_BOOTSTRAP_API_KEY` if you go direct. Key validation becomes an in-memory DashMap hash lookup (~1μs) with zero QS roundtrip. See [AUTH_CHAIN.md](https://github.com/all-source-os/all-source/blob/main/docs/current/AUTH_CHAIN.md) for the full mechanics.
- **Tune retry / circuit breaker** via `CoreClient::with_config(ClientConfig { retry, circuit_breaker_threshold, ... })` when your upstream SLO differs from the 3-retry / 5-failure default.
- **Real-time reads** — subscribe to the WebSocket (`ProjectionWorker` or `EventStreamClient`) rather than polling. Sub-millisecond delivery after Core's broadcast vs. whatever interval you poll at.

## API keys

- **Provision** via `ALLSOURCE_BOOTSTRAP_API_KEY` on Core's first boot (idempotent — safe to leave set). Keys persist in Core's system WAL.
- **Authenticate** with `Authorization: Bearer ask_...` (preferred) or the legacy `X-API-Key` header. The SDK uses `Authorization: Bearer` automatically.
- **Validation** — Core does in-process hash lookup. Query Service adds a 120s ETS cache (SHA-256 keyed) before falling back to Core, so repeat calls through QS don't hammer Core.
- **Rotate** by provisioning a new key, updating clients, then revoking the old one. See the [AllSource API key patterns blog post](https://all-source.xyz/blog/api-keys-and-connections) for rotation without downtime.

## Analyze and rebuild a projection

Replay operations belong to `QueryClient`: Query Service applies the tenant
boundary and rebuilds into a shadow generation before publishing atomically.

```rust,no_run
let analysis = query.analyze_projection_replay("event-count").await?;

if analysis.ready_to_replay && analysis.total_events > 0 {
    let run = query.start_projection_replay(&analysis.projection_name).await?;
    let progress = query.get_projection_replay(&run.replay_id).await?;
    println!("{}%", progress.progress_percentage);
}
# Ok::<(), allsource::Error>(())
```

Use `list_projection_replays` for tenant run history and
`cancel_projection_replay` to stop a running rebuild without publishing partial
state.

## Building custom projections

`ProjectionWorker` is a first-party worker for reducing events into domain state without reimplementing WebSocket subscription, checkpointing, dedup, and reconnection boilerplate.

```rust,no_run
use std::collections::HashMap;
use allsource::{CoreClient, Error, Event, ProjectionWorker};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct AssetState {
    symbol: String,
    altname: String,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let core = CoreClient::new("http://localhost:3900", "your-api-key")?;

    let worker = ProjectionWorker::<HashMap<String, AssetState>>::builder(core)
        .name("assets")                                  // durable consumer id
        .event_types(&["asset.registered", "asset.updated"])
        .reducer(|state, event: &Event| {
            if event.event_type == "asset.registered" {
                let symbol = event.payload.get("symbol")
                    .and_then(|v| v.as_str()).unwrap_or("").to_string();
                state.insert(symbol.clone(), AssetState { symbol, ..Default::default() });
            }
            Ok(())
        })
        .checkpoint_interval(100)
        .build()?;

    let handle = worker.start().await?;

    // Wait for catch-up replay to finish.
    while !handle.is_caught_up() {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // Read reduced state directly.
    if let Some(asset) = handle.state().read().await.get("BTC") {
        println!("BTC: {asset:?}");
    }

    handle.stop().await?;
    Ok(())
}
```

See the runnable [`examples/asset_projection.rs`](examples/asset_projection.rs) for a full worker covering `asset.registered`, `asset.updated`, and `exchange_mapping.set` — the shape originally requested in [issue #155](https://github.com/all-source-os/all-source/issues/155).

### When to reach for what

| You want... | Use |
|---|---|
| Ad-hoc queries, one-shot reads | `QueryClient::query_events` |
| Fold events into state, one call | `QueryClient::query_and_fold` with `EventFolder` |
| A long-lived read model with live updates | **`ProjectionWorker`** |
| Server-managed projections (Entity Snapshot, Event Counter) | Query Service built-in projections |

`ProjectionWorker` is the right answer when the read model should stay warm and caught-up, when replay-on-cold-start would be O(n) expensive, or when you want live updates without polling.

### Lifecycle notes

- **Checkpointing**: the worker registers a durable consumer with Core. Core tracks the cursor server-side; the SDK `save_checkpoint` call is an `ack_consumer` under the hood. Restart → Core replays only what was added after the last ack.
- **Reconnection**: exponential backoff (100ms → 30s) on WS or HTTP error. Core's consumer replay fills any gaps when the connection returns.
- **Dedup**: per-entity version tracking in the worker. Events with `version ≤ last_applied` are skipped in case of replay overlap.
- **State push-back** (optional, off by default): enable via `.state_flush_entities(...)` to periodically upsert reduced state into the Core read model named after the worker. Other consumers read it with `core.read_model("assets").page(...)`, and `ProjectionHandle::get_state(id)` reads one row of it.

### Cargo features

- `projection-worker` (default) — enables `ProjectionWorker` + WebSocket support
- `ws` — WebSocket client only (without the worker)
- `rustls` (default) — rustls TLS backend
- `native-tls` — platform-native TLS

To build without WebSocket support (smaller dep tree):

```toml
allsource = { version = "0.26", default-features = false, features = ["rustls"] }
```

For a deeper guide on projection design, operational concerns, and migration from polling, see [`docs/use-cases/custom-projections.md`](../../docs/use-cases/custom-projections.md).

## API surface

| Client | Method | Purpose |
|--------|--------|---------|
| `CoreClient` | `ingest_event` / `ingest_batch` | Write events |
| Both | `entity_state` | One entity's current state, conditional |
| Both | `feed`, `read_model(name).page` / `.commit` | Event feed and read models |
| Both | `define_view`, `get_view`, `delete_view`, `list_views` | View definitions |
| Both | `latest`, `latest_many`, `view_entities`, `view_changes`, `sync` | View rows |
| `CoreClient` | `register/get/ack_consumer` | Durable consumer registry |
| `CoreClient` | `poll_consumer_events` | Pull-based alternative to the WS stream |
| `CoreClient` | `save_checkpoint` / `load_checkpoint` | Worker checkpoint helpers |
| `QueryClient` | `query_events` | Filter events |
| `QueryClient` | `query_and_fold::<F: EventFolder>` | Fold → state in one call |
| `QueryClient` | `list_entities`, `detect_duplicates` | Entity discovery |
| `ProjectionWorker` | `builder(core)` | Start configuring a worker |
| `ProjectionHandle` | `state()`, `get_state(id)`, `is_caught_up()`, `stop()` | Control running worker |

## License

[MIT](../../LICENSE)
