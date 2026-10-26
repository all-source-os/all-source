# Custom Projections with `ProjectionWorker`

_Requires `allsource` Rust SDK v0.19 or later._

## When to use `ProjectionWorker` vs. other options

| Need | Use |
|---|---|
| One-off query across events | `QueryClient::query_events` |
| Fold events to state in a single call (stateless callers) | `QueryClient::query_and_fold` with `EventFolder` |
| Server-tracked Entity Snapshot / Event Counter projections | Query Service built-in projections |
| **Custom read model, live updates, survives process restarts** | **`ProjectionWorker`** |
| Cross-process / cross-service read model | `ProjectionWorker` + `.state_flush_entities` push-back |

`ProjectionWorker` is the right answer when you want a long-lived read model, fed by live events, that doesn't re-replay from scratch on every process start. Under the hood it drives Core's durable-consumer protocol (`POST /consumers`, WebSocket `?consumer_id=...`, `POST /consumers/:id/ack`) so Core tracks your cursor position server-side.

## Reducer design

A reducer is a pure function `(state, event) → state`. Expectations:

- **Idempotent** per event. The SDK delivers each committed event at least once — reconnection and overlap-replay can cause the same event to arrive twice. Design the reducer so reapplying an event is a no-op.
- **Total**. Return `Ok(())` for event types you don't care about (the `_` match arm). The subscription filter is a pre-filter, not a contract — new event types may leak in if your prefix is permissive.
- **Synchronous**. The reducer signature is `FnMut(&mut S, &Event) -> Result<(), Error>`. Reading from a database or a network service inside the reducer is a code smell and blocks the event loop. If you need external lookups, compute them in a side channel and pass the derived facts in as `metadata`.
- **Errors abort the worker**. Returning `Err` stops the worker and logs at `error!`. Save this for genuinely unrecoverable conditions (schema violation, malformed payload that shouldn't exist). Transient errors should be swallowed and logged.

### State shape

For the canonical case — a keyed collection of entity states — use `HashMap<EntityId, State>`:

```rust
struct AssetState { symbol: String, altname: String, /* ... */ }
let worker = ProjectionWorker::<HashMap<String, AssetState>>::builder(core)
    .reducer(|state, event| {
        // state: &mut HashMap<String, AssetState>
        Ok(())
    })
    /* ... */
```

For aggregate counters, a single struct works:

```rust
#[derive(Default, Serialize, Deserialize)]
struct TotalVolume { usd: f64, trade_count: u64 }
let worker = ProjectionWorker::<TotalVolume>::builder(core) /* ... */
```

State must be `Default + Send + Sync + Serialize + DeserializeOwned + 'static`.

## Checkpoint semantics

### What gets stored

Exactly one thing: a WAL offset (the `cursor_position` on Core's durable consumer). The reduced state itself is NOT checkpointed — it's rebuilt from the event stream on every process start.

### How resume works

1. On `start()`, the worker registers the consumer (idempotent) and opens a WebSocket with `?consumer_id=<name>`.
2. Core replays events since the last ack, tagged `{"type":"replay","position":N,...}`.
3. The worker applies each replay event and acks at `checkpoint_interval` boundaries.
4. After `replay_complete`, the worker transitions to live mode and processes events as they arrive.

Checkpoint is written via `POST /api/v1/consumers/:name/ack`. This is a small request (one integer), so `checkpoint_interval = 100` is reasonable for most workloads. Lower it (e.g. `10`) if fast restart-resume matters more than Core throughput; raise it if the reducer is very fast and Core is the bottleneck.

### What happens when the reducer changes

A changed reducer means the projection is a different read model. Three options:

- **Rename the worker.** `ProjectionWorker::builder(core).name("assets_v2")` — fresh consumer, fresh cursor, full replay.
- **Reset the existing consumer.** `POST /api/v1/consumers/:name/ack { "position": 0 }` — next start replays from 0.
- **Run both side-by-side.** Old worker keeps ticking for consumers that still read the old shape; new worker builds the new shape. Swap consumers when the new one is caught up. This is the zero-downtime migration pattern.

The SDK doesn't automate any of this — migration is a domain-specific operational decision.

## Operational concerns

### Delivery guarantees

**At-least-once.** Combined with Core's durable cursor, this means: every committed event reaches your reducer at least once, probably exactly once under normal operation, and possibly more than once across reconnection boundaries.

Design for idempotence. The worker includes per-entity version-based dedup as a safety net (events with `version ≤ last_applied` are skipped), but that only helps for events from the same entity. Cross-entity invariants must be idempotent on their own.

### Reconnection behavior

On WS error or EOF, the worker reconnects with exponential backoff starting at 100ms and capping at 30s. The backoff resets to the base delay on a successful connection.

Core's server-side cursor means reconnection is transparent: the replay that follows a reconnect covers any events that landed during downtime. There's no "missed events" handling the SDK needs to implement.

### Lag handling

If Core's broadcast channel overflows (`{"type":"lagged","missed":N}`), the worker logs at `warn!` and continues. The missed events will be replayed on the next reconnection, since Core's cursor lags behind the write position and replay re-covers them. If you see persistent lagged frames, the fix is usually on the Core side: increase broadcast capacity or reduce concurrent consumers.

### Observability

The worker emits structured tracing events on the `allsource` target:

| Level | When |
|---|---|
| `info!` | Replay complete (with `replayed` count) |
| `warn!` | WS EOF, stream error, lagged notice, reconnect attempt |
| `debug!` | Checkpoint saved, state flushed |
| `error!` | Reducer error (aborts worker), flush failure (non-fatal), connect failure |

Structured fields on every event: `worker` (consumer id), `position`, `error`, etc. With `tracing-subscriber`, filter with `RUST_LOG=allsource=debug` to see the full lifecycle.

Metrics the operator should scrape:

- Worker process up/down (via process supervision, not exposed by SDK)
- Reconnection rate (counter derived from `warn!` events)
- Reducer latency (instrument inside the closure if needed)
- Checkpoint lag — compare `ProjectionHandle::current_position()` to Core's write offset

## Migrating from polling `QueryClient`

The common pre-v0.19 pattern:

```rust
// OLD: O(n) every cold start
async fn rebuild_state(query: &QueryClient) -> HashMap<String, AssetState> {
    let mut state = HashMap::new();
    let mut offset = 0;
    loop {
        let resp = query.query_events(
            QueryEventsParams::new().event_type_prefix("asset.").offset(offset).limit(1000)
        ).await.unwrap();
        if resp.events.is_empty() { break; }
        for e in &resp.events { apply(&mut state, e); }
        offset += resp.events.len() as u32;
    }
    state
}
// …then poll for new events every N seconds, track high-water mark yourself, etc.
```

The v0.19 equivalent:

```rust
// NEW: cursor-tracked, live updates, O(events-since-last-ack) on restart
let worker = ProjectionWorker::<HashMap<String, AssetState>>::builder(core)
    .name("assets")
    .event_types(&["asset."])
    .reducer(|state, event| { apply(state, event); Ok(()) })
    .checkpoint_interval(100)
    .build()?;
let handle = worker.start().await?;
// handle.state() is the HashMap, live.
```

Key differences:
- Cold start is O(events since last ack), not O(total events).
- Live updates via WebSocket instead of polling.
- Core tracks the cursor; no client-side offset bookkeeping.
- Reconnection + dedup are handled for you.

## Known limitations (v0.19)

- **Single-instance only.** Two workers with the same `name` running concurrently will thrash Core's cursor. Use different names (`"assets-shard-0"`, `"assets-shard-1"`) if you need sharding — but that requires partition-aware reducers.
- **No back-pressure to Core.** If your reducer is slower than the live event rate, the worker will fall behind. Core's broadcast channel will eventually lag, triggering replay on reconnect. The SDK doesn't currently slow Core down; scale the reducer or shard.
- **Reducer is sync.** If you need async work per event (e.g. enrichment from an external service), do it via a channel outside the reducer, or batch it. A future release may add an async reducer variant.
- **State push-back reads through Core.** `.state_flush_entities` upserts into the Core read model named after the worker (Rust SDK 0.26 and later). `ProjectionHandle::get_state` reads one row of it; other processes read it with `core.read_model(name).page(...)`. The worker name must be a valid read-model name (1..64 of `[A-Za-z0-9_-]`, not `view_*`).
- **Api key on WebSocket.** The v0.19 WS client connects without auth. Core's `/events/stream` doesn't require a key today; when that changes, the worker will need to thread the API key through.

## Related docs

- Working example: [`sdks/rust/examples/asset_projection.rs`](../../sdks/rust/examples/asset_projection.rs)
- Original feature request: [issue #155](https://github.com/all-source-os/all-source/issues/155)
- Core's durable-consumer protocol: implementation in `apps/core/src/infrastructure/web/websocket.rs` and `api.rs`
- Built-in Query Service projections (for comparison): [`SERVER_SIDE_PROJECTIONS_USE_CASES.md`](./SERVER_SIDE_PROJECTIONS_USE_CASES.md)
