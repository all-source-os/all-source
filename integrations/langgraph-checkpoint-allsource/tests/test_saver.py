from __future__ import annotations

import operator
from dataclasses import dataclass
from typing import Annotated, Any, TypedDict

import pytest
from langgraph.graph import END, START, StateGraph

from langgraph.checkpoint.allsource.saver import (
    CHECKPOINT_EVENT,
    DELETE_EVENT,
    WRITES_EVENT,
    AllSourceSaver,
)


@dataclass(frozen=True)
class FakeEvent:
    id: str
    event_type: str
    payload: dict[str, Any]
    timestamp: str
    version: int


class FakeClient:
    def __init__(self) -> None:
        self.events: dict[str, list[FakeEvent]] = {}
        self.closed = False

    def ingest(self, event_type: str, entity_id: str, payload: dict[str, Any]) -> None:
        stream = self.events.setdefault(entity_id, [])
        version = len(stream) + 1
        stream.append(
            FakeEvent(
                id=f"event-{version}",
                event_type=event_type,
                payload=payload,
                timestamp=f"2026-09-11T00:00:{version:02d}Z",
                version=version,
            )
        )

    def by_entity(self, entity_id: str) -> list[FakeEvent]:
        return list(self.events.get(entity_id, []))

    def by_type(self, event_type: str, limit: int) -> list[FakeEvent]:
        return [
            event
            for stream in self.events.values()
            for event in stream
            if event.event_type == event_type
        ][:limit]

    def close(self) -> None:
        self.closed = True


class GraphState(TypedDict):
    messages: Annotated[list[str], operator.add]


def checkpoint(checkpoint_id: str, value: str) -> dict[str, Any]:
    return {
        "v": 4,
        "id": checkpoint_id,
        "ts": f"2026-09-11T00:00:{checkpoint_id[-2:]}Z",
        "channel_values": {"messages": [value]},
        "channel_versions": {"messages": checkpoint_id},
        "versions_seen": {},
        "updated_channels": ["messages"],
    }


def config(thread_id: str, checkpoint_id: str | None = None) -> dict[str, Any]:
    configurable: dict[str, Any] = {"thread_id": thread_id, "checkpoint_ns": ""}
    if checkpoint_id is not None:
        configurable["checkpoint_id"] = checkpoint_id
    return {"configurable": configurable}


def test_put_get_latest_and_exact_checkpoint() -> None:
    client = FakeClient()
    saver = AllSourceSaver(client=client)
    first = saver.put(config("thread-1"), checkpoint("0001", "first"), {"step": 1}, {})
    saver.put(first, checkpoint("0002", "second"), {"step": 2}, {})

    latest = saver.get_tuple(config("thread-1"))
    exact = saver.get_tuple(config("thread-1", "0001"))

    assert latest is not None
    assert latest.checkpoint["channel_values"] == {"messages": ["second"]}
    assert latest.metadata["step"] == 2
    assert latest.parent_config == config("thread-1", "0001")
    assert exact is not None
    assert exact.checkpoint["channel_values"] == {"messages": ["first"]}
    assert client.by_type(CHECKPOINT_EVENT, 10)


def test_list_filters_orders_and_honors_before() -> None:
    client = FakeClient()
    saver = AllSourceSaver(client=client)
    saver.put(config("thread-a"), checkpoint("0001", "a1"), {"source": "test"}, {})
    saver.put(
        config("thread-a", "0001"), checkpoint("0002", "a2"), {"source": "live"}, {}
    )
    saver.put(config("thread-b"), checkpoint("0003", "b1"), {"source": "test"}, {})

    scoped = list(
        saver.list(
            config("thread-a"),
            filter={"source": "test"},
            before=config("thread-a", "0002"),
        )
    )
    unscoped = list(saver.list(None, limit=2))

    assert [item.config["configurable"]["checkpoint_id"] for item in scoped] == ["0001"]
    assert [item.config["configurable"]["checkpoint_id"] for item in unscoped] == [
        "0003",
        "0002",
    ]


def test_pending_writes_use_langgraph_deduplication_rules() -> None:
    client = FakeClient()
    saver = AllSourceSaver(client=client)
    saved = saver.put(config("thread-1"), checkpoint("0001", "first"), {}, {})
    saver.put_writes(
        saved, [("messages", "first-write"), ("__error__", "old")], "task-1"
    )
    saver.put_writes(saved, [("messages", "duplicate"), ("__error__", "new")], "task-1")

    result = saver.get_tuple(saved)

    assert result is not None
    assert ("task-1", "messages", "first-write") in result.pending_writes
    assert ("task-1", "messages", "duplicate") not in result.pending_writes
    assert ("task-1", "__error__", "new") in result.pending_writes
    assert client.by_type(WRITES_EVENT, 10)


def test_delete_thread_hides_old_history_but_allows_new_history() -> None:
    client = FakeClient()
    saver = AllSourceSaver(client=client)
    saver.put(config("thread-1"), checkpoint("0001", "old"), {}, {})

    saver.delete_thread("thread-1")

    assert saver.get_tuple(config("thread-1")) is None
    assert client.by_type(DELETE_EVENT, 10)

    saver.put(config("thread-1"), checkpoint("0002", "new"), {}, {})
    current = saver.get_tuple(config("thread-1"))
    assert current is not None
    assert current.checkpoint["channel_values"] == {"messages": ["new"]}


def test_langgraph_recovers_state_with_new_saver_instance() -> None:
    client = FakeClient()
    builder = StateGraph(GraphState)
    builder.add_node("persist", lambda _: {"messages": ["persisted"]})
    builder.add_edge(START, "persist")
    builder.add_edge("persist", END)
    graph = builder.compile(checkpointer=AllSourceSaver(client=client))
    graph.invoke(
        {"messages": ["start"]},
        {"configurable": {"thread_id": "restart-proof"}},
    )

    restarted = builder.compile(checkpointer=AllSourceSaver(client=client))
    state = restarted.get_state({"configurable": {"thread_id": "restart-proof"}})

    assert state.values["messages"] == ["start", "persisted"]


@pytest.mark.asyncio
async def test_async_api_and_context_close() -> None:
    client = FakeClient()
    with AllSourceSaver(client=client) as saver:
        saved = await saver.aput(
            config("thread-1"), checkpoint("0001", "async"), {}, {}
        )
        await saver.aput_writes(saved, [("messages", "write")], "task-1")
        result = await saver.aget_tuple(saved)
        history = [item async for item in saver.alist(config("thread-1"))]

    assert result is not None
    assert result.pending_writes == [("task-1", "messages", "write")]
    assert len(history) == 1
    assert client.closed is True


def test_constructor_validation() -> None:
    with pytest.raises(ValueError, match="api_key"):
        AllSourceSaver()
    with pytest.raises(ValueError, match="scan_limit"):
        AllSourceSaver(client=FakeClient(), scan_limit=0)
