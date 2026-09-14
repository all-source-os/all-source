"""LangGraph checkpoint saver backed by immutable AllSource events."""

from __future__ import annotations

import asyncio
import base64
import hashlib
from collections.abc import AsyncIterator, Iterator, Sequence
from contextlib import AbstractContextManager
from dataclasses import dataclass
from typing import Any, Protocol, cast
from urllib.parse import quote

import httpx
from langchain_core.runnables import RunnableConfig
from langgraph.checkpoint.base import (
    WRITES_IDX_MAP,
    BaseCheckpointSaver,
    ChannelVersions,
    Checkpoint,
    CheckpointMetadata,
    CheckpointTuple,
    get_checkpoint_id,
    get_checkpoint_metadata,
)
from langgraph.checkpoint.serde.base import SerializerProtocol
from typing_extensions import Self

CHECKPOINT_EVENT = "langgraph.checkpoint.saved.v1"
WRITES_EVENT = "langgraph.checkpoint.writes.saved.v1"
DELETE_EVENT = "langgraph.thread.deleted.v1"


@dataclass(frozen=True)
class _Event:
    id: str
    event_type: str
    payload: dict[str, Any]
    timestamp: str
    version: int


class _EventClient(Protocol):
    def ingest(
        self, event_type: str, entity_id: str, payload: dict[str, Any]
    ) -> None: ...

    def by_entity(self, entity_id: str) -> list[_Event]: ...

    def by_type(self, event_type: str, limit: int) -> list[_Event]: ...

    def close(self) -> None: ...


class _HTTPEventClient:
    def __init__(
        self,
        api_key: str,
        base_url: str,
        timeout: float,
    ) -> None:
        self._http = httpx.Client(
            base_url=base_url.rstrip("/"),
            headers={
                "Authorization": f"Bearer {api_key}",
                "Content-Type": "application/json",
            },
            timeout=timeout,
        )

    def ingest(self, event_type: str, entity_id: str, payload: dict[str, Any]) -> None:
        response = self._http.post(
            "/api/events",
            json={
                "event_type": event_type,
                "entity_id": entity_id,
                "payload": payload,
            },
        )
        response.raise_for_status()

    def by_entity(self, entity_id: str) -> list[_Event]:
        response = self._http.get(f"/api/events/entity/{quote(entity_id, safe='')}")
        response.raise_for_status()
        return _parse_events(response.json())

    def by_type(self, event_type: str, limit: int) -> list[_Event]:
        response = self._http.get(
            "/api/events", params={"event_type": event_type, "limit": limit}
        )
        response.raise_for_status()
        return _parse_events(response.json())

    def close(self) -> None:
        self._http.close()


def _parse_events(body: Any) -> list[_Event]:
    data = body.get("data", body) if isinstance(body, dict) else body
    if isinstance(data, dict):
        data = data.get("events", data.get("data", []))
    if not isinstance(data, list):
        return []
    return [
        _Event(
            id=str(item.get("id", "")),
            event_type=str(item.get("event_type", "")),
            payload=cast(dict[str, Any], item.get("payload", {})),
            timestamp=str(item.get("timestamp", "")),
            version=int(item.get("version", 0)),
        )
        for item in data
        if isinstance(item, dict)
    ]


def _event_key(event: _Event) -> tuple[int, str, str]:
    return (event.version, event.timestamp, event.id)


def _entity_id(thread_id: str) -> str:
    digest = hashlib.sha256(thread_id.encode("utf-8")).hexdigest()
    return f"langgraph-thread-{digest}"


def _pack(serde: SerializerProtocol, value: Any) -> dict[str, str]:
    type_name, data = serde.dumps_typed(value)
    return {
        "type": type_name,
        "data": base64.b64encode(data).decode("ascii"),
    }


def _unpack(serde: SerializerProtocol, value: dict[str, str]) -> Any:
    return serde.loads_typed(
        (value["type"], base64.b64decode(value["data"].encode("ascii")))
    )


class AllSourceSaver(
    BaseCheckpointSaver[str], AbstractContextManager["AllSourceSaver"]
):
    """Persist LangGraph checkpoints as immutable AllSource events.

    `client` exists for tests and custom transports. Normal callers provide an
    AllSource API key and optional self-hosted base URL.
    """

    def __init__(
        self,
        api_key: str | None = None,
        *,
        base_url: str = "https://api.all-source.xyz",
        timeout: float = 30.0,
        scan_limit: int = 10_000,
        serde: SerializerProtocol | None = None,
        client: _EventClient | None = None,
    ) -> None:
        super().__init__(serde=serde)
        if client is None and not api_key:
            raise ValueError("api_key is required when client is not supplied")
        if scan_limit <= 0:
            raise ValueError("scan_limit must be positive")
        self._client = client or _HTTPEventClient(cast(str, api_key), base_url, timeout)
        self._scan_limit = scan_limit

    def __enter__(self) -> Self:
        return self

    def __exit__(self, *args: object) -> None:
        self.close()

    def close(self) -> None:
        self._client.close()

    def _thread_events(self, thread_id: str) -> list[_Event]:
        events = sorted(self._client.by_entity(_entity_id(thread_id)), key=_event_key)
        delete_key = max(
            (_event_key(event) for event in events if event.event_type == DELETE_EVENT),
            default=None,
        )
        if delete_key is not None:
            events = [event for event in events if _event_key(event) > delete_key]
        return events

    def _checkpoint_events(self, config: RunnableConfig | None) -> list[_Event]:
        if config is not None:
            thread_id = str(config["configurable"]["thread_id"])
            return [
                event
                for event in self._thread_events(thread_id)
                if event.event_type == CHECKPOINT_EVENT
            ]

        events = self._client.by_type(CHECKPOINT_EVENT, self._scan_limit)
        thread_ids = {str(event.payload.get("thread_id", "")) for event in events}
        active: list[_Event] = []
        for thread_id in thread_ids:
            if thread_id:
                active.extend(
                    event
                    for event in self._thread_events(thread_id)
                    if event.event_type == CHECKPOINT_EVENT
                )
        return active

    def _tuple(self, event: _Event, thread_events: list[_Event]) -> CheckpointTuple:
        payload = event.payload
        thread_id = str(payload["thread_id"])
        checkpoint_ns = str(payload.get("checkpoint_ns", ""))
        checkpoint_id = str(payload["checkpoint_id"])
        parent_checkpoint_id = payload.get("parent_checkpoint_id")
        pending: dict[tuple[str, int], tuple[str, str, Any]] = {}

        for write_event in thread_events:
            write_payload = write_event.payload
            if (
                write_event.event_type != WRITES_EVENT
                or write_payload.get("checkpoint_ns", "") != checkpoint_ns
                or write_payload.get("checkpoint_id") != checkpoint_id
            ):
                continue
            task_id = str(write_payload["task_id"])
            for item in write_payload.get("writes", []):
                channel = str(item["channel"])
                index = int(item["index"])
                key = (task_id, WRITES_IDX_MAP.get(channel, index))
                if key[1] < 0 or key not in pending:
                    pending[key] = (
                        task_id,
                        channel,
                        _unpack(self.serde, item["value"]),
                    )

        config: RunnableConfig = {
            "configurable": {
                "thread_id": thread_id,
                "checkpoint_ns": checkpoint_ns,
                "checkpoint_id": checkpoint_id,
            }
        }
        parent_config: RunnableConfig | None = None
        if parent_checkpoint_id:
            parent_config = {
                "configurable": {
                    "thread_id": thread_id,
                    "checkpoint_ns": checkpoint_ns,
                    "checkpoint_id": str(parent_checkpoint_id),
                }
            }
        return CheckpointTuple(
            config=config,
            checkpoint=cast(Checkpoint, _unpack(self.serde, payload["checkpoint"])),
            metadata=cast(CheckpointMetadata, _unpack(self.serde, payload["metadata"])),
            parent_config=parent_config,
            pending_writes=list(pending.values()),
        )

    def get_tuple(self, config: RunnableConfig) -> CheckpointTuple | None:
        thread_id = str(config["configurable"]["thread_id"])
        checkpoint_ns = str(config["configurable"].get("checkpoint_ns", ""))
        checkpoint_id = get_checkpoint_id(config)
        thread_events = self._thread_events(thread_id)
        matches = [
            event
            for event in thread_events
            if event.event_type == CHECKPOINT_EVENT
            and event.payload.get("checkpoint_ns", "") == checkpoint_ns
            and (
                checkpoint_id is None
                or event.payload.get("checkpoint_id") == checkpoint_id
            )
        ]
        if not matches:
            return None
        if checkpoint_id is None:
            event = max(matches, key=lambda item: str(item.payload["checkpoint_id"]))
        else:
            event = max(matches, key=_event_key)
        return self._tuple(event, thread_events)

    def list(
        self,
        config: RunnableConfig | None,
        *,
        filter: dict[str, Any] | None = None,
        before: RunnableConfig | None = None,
        limit: int | None = None,
    ) -> Iterator[CheckpointTuple]:
        config_ns = config["configurable"].get("checkpoint_ns") if config else None
        config_id = get_checkpoint_id(config) if config else None
        before_id = get_checkpoint_id(before) if before else None
        events = sorted(
            self._checkpoint_events(config),
            key=lambda item: str(item.payload["checkpoint_id"]),
            reverse=True,
        )
        yielded = 0
        for event in events:
            payload = event.payload
            checkpoint_id = str(payload["checkpoint_id"])
            if config_ns is not None and payload.get("checkpoint_ns", "") != config_ns:
                continue
            if config_id is not None and checkpoint_id != config_id:
                continue
            if before_id is not None and checkpoint_id >= before_id:
                continue
            metadata = cast(
                CheckpointMetadata, _unpack(self.serde, payload["metadata"])
            )
            if filter and not all(
                metadata.get(key) == value for key, value in filter.items()
            ):
                continue
            if limit is not None and yielded >= limit:
                break
            thread_events = self._thread_events(str(payload["thread_id"]))
            yield self._tuple(event, thread_events)
            yielded += 1

    def put(
        self,
        config: RunnableConfig,
        checkpoint: Checkpoint,
        metadata: CheckpointMetadata,
        new_versions: ChannelVersions,
    ) -> RunnableConfig:
        del new_versions
        thread_id = str(config["configurable"]["thread_id"])
        checkpoint_ns = str(config["configurable"].get("checkpoint_ns", ""))
        checkpoint_id = str(checkpoint["id"])
        self._client.ingest(
            CHECKPOINT_EVENT,
            _entity_id(thread_id),
            {
                "schema_version": 1,
                "thread_id": thread_id,
                "checkpoint_ns": checkpoint_ns,
                "checkpoint_id": checkpoint_id,
                "parent_checkpoint_id": config["configurable"].get("checkpoint_id"),
                "checkpoint": _pack(self.serde, checkpoint),
                "metadata": _pack(
                    self.serde, get_checkpoint_metadata(config, metadata)
                ),
            },
        )
        return {
            "configurable": {
                "thread_id": thread_id,
                "checkpoint_ns": checkpoint_ns,
                "checkpoint_id": checkpoint_id,
            }
        }

    def put_writes(
        self,
        config: RunnableConfig,
        writes: Sequence[tuple[str, Any]],
        task_id: str,
        task_path: str = "",
    ) -> None:
        thread_id = str(config["configurable"]["thread_id"])
        checkpoint_ns = str(config["configurable"].get("checkpoint_ns", ""))
        checkpoint_id = str(config["configurable"]["checkpoint_id"])
        self._client.ingest(
            WRITES_EVENT,
            _entity_id(thread_id),
            {
                "schema_version": 1,
                "thread_id": thread_id,
                "checkpoint_ns": checkpoint_ns,
                "checkpoint_id": checkpoint_id,
                "task_id": task_id,
                "task_path": task_path,
                "writes": [
                    {
                        "index": index,
                        "channel": channel,
                        "value": _pack(self.serde, value),
                    }
                    for index, (channel, value) in enumerate(writes)
                ],
            },
        )

    def delete_thread(self, thread_id: str) -> None:
        self._client.ingest(
            DELETE_EVENT,
            _entity_id(thread_id),
            {"schema_version": 1, "thread_id": thread_id},
        )

    async def aget_tuple(self, config: RunnableConfig) -> CheckpointTuple | None:
        return await asyncio.to_thread(self.get_tuple, config)

    async def alist(
        self,
        config: RunnableConfig | None,
        *,
        filter: dict[str, Any] | None = None,
        before: RunnableConfig | None = None,
        limit: int | None = None,
    ) -> AsyncIterator[CheckpointTuple]:
        items = await asyncio.to_thread(
            lambda: list(self.list(config, filter=filter, before=before, limit=limit))
        )
        for item in items:
            yield item

    async def aput(
        self,
        config: RunnableConfig,
        checkpoint: Checkpoint,
        metadata: CheckpointMetadata,
        new_versions: ChannelVersions,
    ) -> RunnableConfig:
        return await asyncio.to_thread(
            self.put, config, checkpoint, metadata, new_versions
        )

    async def aput_writes(
        self,
        config: RunnableConfig,
        writes: Sequence[tuple[str, Any]],
        task_id: str,
        task_path: str = "",
    ) -> None:
        await asyncio.to_thread(self.put_writes, config, writes, task_id, task_path)

    async def adelete_thread(self, thread_id: str) -> None:
        await asyncio.to_thread(self.delete_thread, thread_id)
