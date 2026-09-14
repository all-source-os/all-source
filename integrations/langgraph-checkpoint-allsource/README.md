# LangGraph checkpoint storage for AllSource

Experimental `BaseCheckpointSaver` implementation backed by immutable AllSource
events. Each checkpoint, pending-write batch, and thread deletion becomes an
ordered event. That makes checkpoint history inspectable and replayable without
pretending current state is complete history.

## Install from source

```bash
pip install ./integrations/langgraph-checkpoint-allsource
```

## Use

```python
from langgraph.checkpoint.allsource import AllSourceSaver

with AllSourceSaver(api_key="as_...") as checkpointer:
    graph = builder.compile(checkpointer=checkpointer)
    graph.invoke(
        {"messages": [{"role": "user", "content": "Remember this"}]},
        {"configurable": {"thread_id": "customer-42"}},
    )
```

Default endpoint: `https://api.all-source.xyz`. Pass `base_url` for self-hosted
AllSource.

## Verify restart persistence

Run two separate processes against one thread:

```bash
pip install -e '.[example]'
ALLSOURCE_API_KEY=as_... python examples/restart_roundtrip.py write
ALLSOURCE_API_KEY=as_... python examples/restart_roundtrip.py read
```

Expected read result:

```text
Recovered after restart: ['start', 'persisted']
```

## Event model

| Event | Purpose |
| --- | --- |
| `langgraph.checkpoint.saved.v1` | Serialized checkpoint, metadata, and parent link |
| `langgraph.checkpoint.writes.saved.v1` | Pending writes linked to checkpoint and task |
| `langgraph.thread.deleted.v1` | Logical deletion boundary for previous thread history |

Thread IDs are SHA-256 hashed for AllSource entity IDs. Payloads still contain
the original thread ID because LangGraph's unscoped `list(None)` contract must
reconstruct checkpoint configs. Do not use sensitive data as a thread ID.

`delete_thread()` appends a deletion marker and hides all earlier history from
the adapter. It does not physically erase immutable source events. Configure
AllSource retention or administrative erasure separately when physical deletion
is required.

`list(None)` performs a bounded cross-thread scan. `scan_limit` defaults to
10,000 events. Prefer thread-scoped history reads in production.

## Status

Alpha. Current LangGraph conformance suite passes all 58 tests covering five
base capabilities: put, put-writes, get-tuple, list, and delete-thread. Extended
copy, pruning, and run-deletion capabilities are not implemented. Next release
gate: live AllSource restart test.

```bash
pip install -e '.[dev]'
pytest -q
mypy src
```
