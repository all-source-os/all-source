# allsource-client

Python SDK for the [AllSource Event Store](https://all-source.xyz).

## Install

```bash
pip install allsource-client
```

## Authentication

AllSource uses API keys. Get one from your dashboard at [all-source.xyz](https://all-source.xyz) (Settings → API Keys) or mint one via `POST /api/v1/auth/api-keys`. The SDK sends the key as a bearer token.

```python
import os
from allsource_client import AllSourceClient

client = AllSourceClient(
    api_key=os.environ["ALLSOURCE_API_KEY"],
    base_url="https://api.all-source.xyz",
)
```

## Quickstart

```python
from allsource_client import AllSourceClient

client = AllSourceClient(api_key="as_your_key", base_url="https://api.all-source.xyz")

# Ingest an event
event = client.ingest("user.signup", "user-123", {"plan": "pro", "source": "web"})
print(f"Created event {event.id}")

# Query events
result = client.query(event_type="user.signup", limit=10)
for e in result.events:
    print(f"{e.timestamp} {e.entity_id}: {e.payload}")
```

## Latest state without history

```python
from allsource_client import NotModified

# Folded state of one entity; history is omitted unless history=True.
state = client.entity_state("skill:abc")
print(state.current_state, state.version)

# Conditional read: NotModified when nothing changed since the last ETag.
again = client.entity_state("skill:abc", if_none_match=state.etag)
assert isinstance(again, NotModified)
```

### Views

A view is a declarative reducer. AllSource folds it from your events and
stores one row per key.

```python
client.define_view("ops_status", {
    "source": {"prefixes": ["ops.probe."]},
    "key": "{payload.probe}:{day:timestamp}",
    "ops": [
        {"on": "ops.probe.observed", "when": {"payload.ok": True}, "increment": {"ok": 1}},
        {"on": "ops.probe.observed", "max": {"last_seen_at": "timestamp"}},
    ],
})

row = client.latest("ops_status", "probe-a:2026-10-27")   # ViewLatest, ViewNotFound or NotModified
for row in client.latest_many("ops_status", prefix="probe-a:"):
    print(row.key, row.state)
```

`sync` keeps a local copy current: it reads every row once, then follows the
changes feed, long-polling when caught up.

```python
from allsource_client import SyncCheckpoint, SyncDelete, SyncReset, SyncUpsert

for item in client.sync("ops_status", checkpoint=stored_revision):
    if isinstance(item, SyncReset):
        rows.clear()
    elif isinstance(item, SyncUpsert):
        rows[item.key] = item.state
    elif isinstance(item, SyncDelete):
        rows.pop(item.key, None)
    elif isinstance(item, SyncCheckpoint):
        stored_revision = item.revision
```

### Event feed and read models

For folds a view cannot express, read the feed and commit rows yourself:

```python
from allsource_client import EpochChangedError, ReadModelCommit

page = client.feed(cursor=cursor, prefixes=["skill."], limit=100)
model = client.read_model("skills")
current = model.page(prefix="skill:")
model.commit(ReadModelCommit(
    expected_revision=current.revision,
    checkpoint=page.next_cursor,
    changes={"skill:abc": {"status": "published"}, "skill:old": None},
))
```

`EpochChangedError` means the feed generation changed: rebuild from position 0
of `current_version` and commit with `reset=True`.

## Async Usage

```python
import asyncio
from allsource_client import AllSourceAsyncClient

async def main():
    async with AllSourceAsyncClient(api_key="as_your_key") as client:
        event = await client.ingest("order.placed", "order-456", {"total": 99.99})
        events = await client.query(entity_id="order-456")
        print(f"Found {events.count} events")
        async for item in client.sync("ops_status", follow=False):
            print(item)

asyncio.run(main())
```

## API Reference

### `AllSourceClient(api_key, base_url, timeout)`

| Method | Description |
|--------|-------------|
| `ingest(event_type, entity_id, data)` | Ingest a single event |
| `ingest_batch(events)` | Ingest multiple events |
| `query(event_type, entity_id, event_type_prefix, since, until, order, limit, offset)` | Query events |
| `query_events(..., as_of, payload_filter, if_none_match)` | Query events; `EventList.entity_version` and `etag`, or `NotModified` |
| `query_and_fold(folder, ...)` | Query events and fold them client-side |
| `get_events_by_entity(entity_id)` | Get events for an entity |
| `get_events_by_type(event_type)` | Get events by type |
| `entity_state(entity_id, as_of, history, if_none_match)` | Folded entity state, or `NotModified` |
| `feed(cursor, prefixes, entity_id, limit)` | One event-feed page |
| `read_model(name).page(prefix, after, limit, revision)` | One read-model page |
| `read_model(name).commit(commit)` | Commit read-model rows, returns the new revision |
| `define_view(name, definition)` / `get_view` / `delete_view` / `list_views` | Manage views |
| `latest(view, key, if_none_match)` | One view row, `ViewNotFound` or `NotModified` |
| `latest_many(view, prefix, limit)` | Iterate every row of a view |
| `view_changes(view, after, limit, wait)` | Rows changed after a revision |
| `sync(view, checkpoint, wait, prefix, page_limit, changes_limit, follow)` | Snapshot then follow a view; honours `Retry-After` |
| `list_prime_projections()` / `define_prime_projection(...)` | Prime projections |
| `project_node(node_id)` / `node_field_provenance(node_id, field)` | Prime nodes |
| `health()` | Check API health |

`AllSourceAsyncClient` has the same methods, all returning coroutines;
`latest_many` and `sync` are async iterators.

## Development

```bash
pip install -e ".[dev]"
pytest
mypy src/
```
