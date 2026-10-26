"""Type definitions for AllSource SDK."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Dict, List, Mapping, Optional, Union


class AllSourceError(Exception):
    """Error returned by the AllSource API."""

    def __init__(
        self,
        code: str,
        message: str,
        status_code: int = 0,
        body: Optional[Any] = None,
    ) -> None:
        super().__init__(message)
        self.code = code
        self.status_code = status_code
        self.body = body


class VersionConflictError(AllSourceError):
    """`409 version_conflict`: a read-model commit raced another writer."""

    def __init__(
        self,
        message: str,
        status_code: int,
        expected_version: int,
        current_version: int,
        body: Optional[Any] = None,
    ) -> None:
        super().__init__("version_conflict", message, status_code, body)
        self.expected_version = expected_version
        self.current_version = current_version


class EpochChangedError(VersionConflictError):
    """The event-feed generation changed.

    Rebuild from position 0 of `current_version` and commit with `reset=True`.
    """


class ViewResetError(AllSourceError):
    """`409 view_reset`: the `after` revision predates the view's last reset."""

    def __init__(
        self,
        message: str,
        status_code: int,
        generation: int,
        body: Optional[Any] = None,
    ) -> None:
        super().__init__("view_reset", message, status_code, body)
        self.generation = generation


class InvalidViewError(AllSourceError):
    """`400 invalid_view`: Core rejected a view definition."""

    def __init__(
        self,
        message: str,
        status_code: int,
        detail: str,
        body: Optional[Any] = None,
    ) -> None:
        super().__init__("invalid_view", message, status_code, body)
        self.detail = detail


class RateLimitedError(AllSourceError):
    """`429` or `503`; `retry_after` is in seconds when the server sent one."""

    def __init__(
        self,
        code: str,
        message: str,
        status_code: int,
        retry_after: Optional[float],
        body: Optional[Any] = None,
    ) -> None:
        super().__init__(code, message, status_code, body)
        self.retry_after = retry_after


@dataclass(frozen=True)
class NotModified:
    """`304 Not Modified`: the state behind `etag` is unchanged."""

    etag: Optional[str]


@dataclass
class Event:
    """An event in the AllSource event store."""

    id: str
    entity_id: str
    event_type: str
    payload: Dict[str, Any]
    timestamp: str
    version: int
    tenant_id: Optional[str] = None
    metadata: Optional[Any] = None

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> Event:
        return cls(
            id=data["id"],
            entity_id=data["entity_id"],
            event_type=data["event_type"],
            payload=data.get("payload", {}),
            timestamp=data["timestamp"],
            version=data.get("version", 0),
            tenant_id=data.get("tenant_id"),
            metadata=data.get("metadata"),
        )


@dataclass
class EventList:
    """A page of events from `GET /api/v1/events/query`."""

    events: List[Event]
    count: int
    total_count: Optional[int] = None
    has_more: bool = False
    entity_version: Optional[int] = None
    etag: Optional[str] = None

    @classmethod
    def from_dict(cls, data: Dict[str, Any], etag: Optional[str] = None) -> EventList:
        events_data = data.get("events", data.get("data", []))
        return cls(
            events=[Event.from_dict(e) for e in events_data],
            count=data.get("count", len(events_data)),
            total_count=data.get("total_count"),
            has_more=bool(data.get("has_more", False)),
            entity_version=data.get("entity_version"),
            etag=etag,
        )


@dataclass
class EntityState:
    """Folded state of one entity, `GET /api/v1/entities/{id}/state`."""

    entity_id: str
    current_state: Dict[str, Any]
    event_count: int
    last_updated: Optional[str]
    as_of: Optional[str]
    version: int
    etag: Optional[str] = None
    history: Optional[List[Dict[str, Any]]] = None

    @classmethod
    def from_dict(cls, data: Dict[str, Any], etag: Optional[str] = None) -> EntityState:
        event_count = data.get("event_count", 0)
        return cls(
            entity_id=data.get("entity_id", ""),
            current_state=data.get("current_state") or {},
            event_count=event_count,
            last_updated=data.get("last_updated"),
            as_of=data.get("as_of"),
            version=data.get("version", event_count),
            etag=etag,
            history=data.get("history"),
        )


@dataclass(frozen=True)
class FeedCursor:
    """A position in the tenant's event feed."""

    epoch: int
    position: int

    @classmethod
    def from_dict(cls, data: Mapping[str, Any]) -> FeedCursor:
        return cls(epoch=data["epoch"], position=data["position"])

    def to_dict(self) -> Dict[str, int]:
        return {"epoch": self.epoch, "position": self.position}


@dataclass
class FeedPage:
    """One page of `GET /api/v1/event-feed`."""

    tenant_id: str
    cursor: FeedCursor
    head: FeedCursor
    events: List[Event]
    positions: List[int]
    caught_up: bool

    @property
    def next_cursor(self) -> FeedCursor:
        """Cursor to resume from: the last element of `positions`."""
        if not self.positions:
            return self.cursor
        return FeedCursor(epoch=self.cursor.epoch, position=self.positions[-1])

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> FeedPage:
        return cls(
            tenant_id=data.get("tenant_id", ""),
            cursor=FeedCursor.from_dict(data["cursor"]),
            head=FeedCursor.from_dict(data["head"]),
            events=[Event.from_dict(e) for e in data.get("events", [])],
            positions=list(data.get("positions", [])),
            caught_up=bool(data.get("caught_up", False)),
        )


@dataclass
class ReadModelPage:
    """One page of `GET /api/v1/read-models/{name}`.

    With `unchanged` true, `rows` is empty and says nothing about deletions.
    """

    revision: int
    content_revision: int
    checkpoint: Optional[FeedCursor]
    ready: bool
    unchanged: bool
    rows: Dict[str, Any]
    next: Optional[str]

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> ReadModelPage:
        checkpoint = data.get("checkpoint")
        return cls(
            revision=data.get("revision", 0),
            content_revision=data.get("content_revision", 0),
            checkpoint=FeedCursor.from_dict(checkpoint) if checkpoint else None,
            ready=bool(data.get("ready", False)),
            unchanged=bool(data.get("unchanged", False)),
            rows=data.get("rows") or {},
            next=data.get("next"),
        )


@dataclass
class ReadModelCommit:
    """Body of `PUT /api/v1/read-models/{name}`. A `None` change deletes the row."""

    expected_revision: int
    checkpoint: FeedCursor
    changes: Dict[str, Optional[Any]]
    ready: bool = True
    reset: bool = False

    def to_dict(self) -> Dict[str, Any]:
        return {
            "expected_revision": self.expected_revision,
            "checkpoint": self.checkpoint.to_dict(),
            "ready": self.ready,
            "reset": self.reset,
            "changes": dict(self.changes),
        }


@dataclass
class ViewInfo:
    """A view definition and its fold progress."""

    view: str
    definition: Dict[str, Any]
    generation: int
    revision: int
    checkpoint: Optional[FeedCursor]
    ready: bool

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> ViewInfo:
        checkpoint = data.get("checkpoint")
        return cls(
            view=data.get("view", ""),
            definition=data.get("definition") or {},
            generation=data.get("generation", 0),
            revision=data.get("revision", 0),
            checkpoint=FeedCursor.from_dict(checkpoint) if checkpoint else None,
            ready=bool(data.get("ready", False)),
        )


@dataclass
class ViewSummary:
    """One entry of `GET /api/v1/views`."""

    view: str
    generation: int
    revision: int
    ready: bool

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> ViewSummary:
        return cls(
            view=data.get("view", ""),
            generation=data.get("generation", 0),
            revision=data.get("revision", 0),
            ready=bool(data.get("ready", False)),
        )


@dataclass
class ViewRow:
    """One row of a view. `state` is `None` for a deletion in a changes page."""

    key: str
    state: Optional[Dict[str, Any]]
    version: int

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> ViewRow:
        return cls(
            key=data["key"],
            state=data.get("state"),
            version=data.get("version", 0),
        )


@dataclass
class ViewLatest:
    """Latest state of one view row."""

    view: str
    key: str
    state: Dict[str, Any]
    version: int
    revision: int
    generation: int
    ready: bool
    etag: Optional[str] = None

    @classmethod
    def from_dict(cls, data: Dict[str, Any], etag: Optional[str] = None) -> ViewLatest:
        return cls(
            view=data.get("view", ""),
            key=data.get("key", ""),
            state=data.get("state") or {},
            version=data.get("version", 0),
            revision=data.get("revision", 0),
            generation=data.get("generation", 0),
            ready=bool(data.get("ready", False)),
            etag=etag,
        )


@dataclass(frozen=True)
class ViewNotFound:
    """No row for `key`. `ready` false means the view may still be rebuilding."""

    view: str
    key: str
    ready: bool


@dataclass
class ViewEntitiesPage:
    """One page of `GET /api/v1/views/{view}/entities`."""

    view: str
    rows: List[ViewRow]
    next: Optional[str]
    revision: int
    content_revision: int
    generation: int
    ready: bool

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> ViewEntitiesPage:
        return cls(
            view=data.get("view", ""),
            rows=[ViewRow.from_dict(r) for r in data.get("rows", [])],
            next=data.get("next"),
            revision=data.get("revision", 0),
            content_revision=data.get("content_revision", 0),
            generation=data.get("generation", 0),
            ready=bool(data.get("ready", False)),
        )


@dataclass
class ViewChanges:
    """One page of `GET /api/v1/views/{view}/changes`."""

    view: str
    changes: List[ViewRow]
    revision: int
    caught_up: bool
    generation: int

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> ViewChanges:
        return cls(
            view=data.get("view", ""),
            changes=[ViewRow.from_dict(r) for r in data.get("changes", [])],
            revision=data.get("revision", 0),
            caught_up=bool(data.get("caught_up", False)),
            generation=data.get("generation", 0),
        )


@dataclass(frozen=True)
class SyncUpsert:
    """A row to create or replace in the caller's copy of the view."""

    key: str
    state: Dict[str, Any]
    version: int


@dataclass(frozen=True)
class SyncDelete:
    """A row to remove from the caller's copy of the view."""

    key: str
    version: int


@dataclass(frozen=True)
class SyncReset:
    """The view was reset; drop every stored row before the snapshot that follows."""

    generation: int


@dataclass(frozen=True)
class SyncCheckpoint:
    """Store `revision` with the rows yielded so far; pass it back as `checkpoint`."""

    revision: int
    generation: int


SyncItem = Union[SyncUpsert, SyncDelete, SyncReset, SyncCheckpoint]


@dataclass
class PrimeProjection:
    """A Prime entity-type projection definition."""

    entity_type: str
    field_policies: Dict[str, str]

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> PrimeProjection:
        return cls(
            entity_type=data.get("entity_type", ""),
            field_policies=data.get("field_policies", {}),
        )


@dataclass
class PrimeProjectionAck:
    """Acknowledgement of a defined Prime projection."""

    entity_type: str
    persisted: bool

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> PrimeProjectionAck:
        return cls(
            entity_type=data.get("entity_type", ""),
            persisted=data.get("persisted", False),
        )


@dataclass
class PrimeSnapshot:
    """A projected snapshot of a Prime node."""

    entity_type: str
    fields: Dict[str, Any]
    observation_count: int

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> PrimeSnapshot:
        return cls(
            entity_type=data.get("entity_type", ""),
            fields=data.get("fields", {}),
            observation_count=data.get("observation_count", 0),
        )


@dataclass
class PrimeProvenance:
    """Provenance for a single projected field of a Prime node."""

    field: str
    value: Any
    source_event_id: str
    source_event_at: str
    merge_policy_applied: str

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> PrimeProvenance:
        return cls(
            field=data.get("field", ""),
            value=data.get("value"),
            source_event_id=data.get("source_event_id", ""),
            source_event_at=data.get("source_event_at", ""),
            merge_policy_applied=data.get("merge_policy_applied", ""),
        )
