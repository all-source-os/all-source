"""Request building and response parsing shared by the sync and async clients.

Nothing here performs I/O, so both clients send the same requests and read the
same bodies.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from datetime import datetime, timezone
from email.utils import parsedate_to_datetime
from typing import Any, Dict, List, Mapping, Optional, Sequence, Type, Union
from urllib.parse import quote

import httpx

from allsource_client.types import (
    AllSourceError,
    EntityState,
    EpochChangedError,
    Event,
    EventList,
    FeedCursor,
    FeedPage,
    InvalidViewError,
    NotModified,
    RateLimitedError,
    ReadModelCommit,
    ReadModelPage,
    SyncCheckpoint,
    SyncDelete,
    SyncItem,
    SyncReset,
    SyncUpsert,
    VersionConflictError,
    ViewChanges,
    ViewEntitiesPage,
    ViewInfo,
    ViewLatest,
    ViewNotFound,
    ViewResetError,
    ViewSummary,
)

MAX_FEED_PREFIXES = 8
MAX_CHANGES_WAIT_SECONDS = 20
MAX_RETRY_AFTER_SECONDS = 60.0
RETRY_AFTER_STATUSES = (429, 503)

_VIEW_NAME = re.compile(r"^[a-z0-9_-]{1,48}$")


@dataclass(frozen=True)
class Request:
    method: str
    path: str
    params: Dict[str, str] = field(default_factory=dict)
    body: Optional[Any] = None
    headers: Dict[str, str] = field(default_factory=dict)


def segment(value: str) -> str:
    """Percent-encode one path segment, `:` and `/` included."""
    return quote(value, safe="")


def _flag(value: bool) -> str:
    return "true" if value else "false"


def _conditional(if_none_match: Optional[str]) -> Dict[str, str]:
    return {"If-None-Match": if_none_match} if if_none_match else {}


def _view_path(view: str) -> str:
    if not _VIEW_NAME.match(view):
        raise ValueError(f"view name must be 1..48 characters of [a-z0-9_-], got {view!r}")
    return f"/api/v1/views/{view}"


def _body(response: httpx.Response) -> Any:
    if not response.content:
        return None
    try:
        return response.json()
    except ValueError:
        return None


def _int(value: Any, default: int = 0) -> int:
    return value if isinstance(value, int) else default


def raise_for_status(
    response: httpx.Response,
    conflict: Type[VersionConflictError] = VersionConflictError,
) -> None:
    """Raise the typed error for a 4xx/5xx response."""
    status = response.status_code
    if status < 400:
        return
    body = _body(response)
    code = "api_error"
    message = response.text or response.reason_phrase
    if isinstance(body, dict):
        error = body.get("error")
        if isinstance(error, dict):
            code = str(error.get("code", code))
            message = str(error.get("message", message))
        elif isinstance(error, str):
            code = error
            message = str(body.get("message") or body.get("detail") or error)
    if code == "version_conflict" and isinstance(body, dict):
        raise conflict(
            message,
            status,
            expected_version=_int(body.get("expected_version")),
            current_version=_int(body.get("current_version")),
            body=body,
        )
    if code == "view_reset" and isinstance(body, dict):
        raise ViewResetError(message, status, generation=_int(body.get("generation")), body=body)
    if code == "invalid_view" and isinstance(body, dict):
        raise InvalidViewError(message, status, detail=str(body.get("detail", "")), body=body)
    if status in RETRY_AFTER_STATUSES:
        if code == "api_error":
            code = "rate_limited" if status == 429 else "unavailable"
        raise RateLimitedError(
            code,
            message,
            status,
            retry_after=parse_retry_after(response.headers.get("Retry-After")),
            body=body,
        )
    raise AllSourceError(code=code, message=message, status_code=status, body=body)


def parse_retry_after(value: Optional[str]) -> Optional[float]:
    """Seconds to wait from a `Retry-After` header (delta-seconds or HTTP-date)."""
    if value is None:
        return None
    value = value.strip()
    if not value:
        return None
    try:
        seconds = float(value)
    except ValueError:
        try:
            when = parsedate_to_datetime(value)
        except (TypeError, ValueError):
            return None
        if when.tzinfo is None:
            when = when.replace(tzinfo=timezone.utc)
        seconds = (when - datetime.now(timezone.utc)).total_seconds()
    return min(max(seconds, 0.0), MAX_RETRY_AFTER_SECONDS)


def retry_after_delay(response: httpx.Response) -> Optional[float]:
    """Delay before repeating the request, or `None` when it must not be repeated."""
    if response.status_code not in RETRY_AFTER_STATUSES:
        return None
    return parse_retry_after(response.headers.get("Retry-After"))


def parse_json(response: httpx.Response) -> Any:
    raise_for_status(response)
    return _body(response)


def parse_empty(response: httpx.Response) -> None:
    raise_for_status(response)


def _data(result: Any) -> Any:
    if isinstance(result, dict) and "data" in result:
        return result["data"]
    return result


# --- Events ---


def ingest_request(event_type: str, entity_id: str, payload: Dict[str, Any]) -> Request:
    return Request(
        "POST",
        "/api/v1/events",
        body={"event_type": event_type, "entity_id": entity_id, "payload": payload},
    )


def _event_from_ack(ack: Mapping[str, Any], sent: Mapping[str, Any]) -> Event:
    if "id" in ack:
        return Event.from_dict(dict(ack))
    return Event(
        id=str(ack.get("event_id", "")),
        entity_id=str(sent.get("entity_id", "")),
        event_type=str(sent.get("event_type", "")),
        payload=dict(sent.get("payload") or {}),
        timestamp=str(ack.get("timestamp", "")),
        version=_int(ack.get("version")),
    )


def parse_ingest(response: httpx.Response, sent: Mapping[str, Any]) -> Event:
    result = _data(parse_json(response))
    return _event_from_ack(result if isinstance(result, dict) else {}, sent)


def ingest_batch_request(events: List[Dict[str, Any]]) -> Request:
    return Request("POST", "/api/v1/events/batch", body={"events": events})


def parse_ingest_batch(response: httpx.Response, sent: List[Dict[str, Any]]) -> List[Event]:
    result = _data(parse_json(response))
    acks = result.get("events", []) if isinstance(result, dict) else result
    if not isinstance(acks, list):
        return []
    return [_event_from_ack(ack, event) for ack, event in zip(acks, sent)]


def query_events_request(
    *,
    event_type: Optional[str] = None,
    entity_id: Optional[str] = None,
    event_type_prefix: Optional[str] = None,
    since: Optional[str] = None,
    until: Optional[str] = None,
    as_of: Optional[str] = None,
    payload_filter: Optional[Mapping[str, Any]] = None,
    order: Optional[str] = None,
    limit: Optional[int] = None,
    offset: Optional[int] = None,
    if_none_match: Optional[str] = None,
) -> Request:
    params: Dict[str, str] = {}
    if event_type is not None:
        params["event_type"] = event_type
    if entity_id is not None:
        params["entity_id"] = entity_id
    if event_type_prefix is not None:
        params["event_type_prefix"] = event_type_prefix
    if since is not None:
        params["since"] = since
    if until is not None:
        params["until"] = until
    if as_of is not None:
        params["as_of"] = as_of
    if payload_filter is not None:
        params["payload_filter"] = json.dumps(payload_filter, separators=(",", ":"))
    if order is not None:
        params["order"] = order
    if limit is not None:
        params["limit"] = str(limit)
    if offset is not None:
        params["offset"] = str(offset)
    return Request(
        "GET", "/api/v1/events/query", params=params, headers=_conditional(if_none_match)
    )


def parse_query_events(response: httpx.Response) -> Union[EventList, NotModified]:
    if response.status_code == 304:
        return NotModified(etag=response.headers.get("ETag"))
    result = _data(parse_json(response))
    return EventList.from_dict(result if isinstance(result, dict) else {}, response.headers.get("ETag"))


def entity_state_request(
    entity_id: str,
    *,
    as_of: Optional[str] = None,
    history: bool = False,
    if_none_match: Optional[str] = None,
) -> Request:
    params: Dict[str, str] = {}
    if as_of is not None:
        params["as_of"] = as_of
    if history:
        params["history"] = _flag(True)
    return Request(
        "GET",
        f"/api/v1/entities/{segment(entity_id)}/state",
        params=params,
        headers=_conditional(if_none_match),
    )


def parse_entity_state(response: httpx.Response) -> Union[EntityState, NotModified]:
    if response.status_code == 304:
        return NotModified(etag=response.headers.get("ETag"))
    result = _data(parse_json(response))
    return EntityState.from_dict(result if isinstance(result, dict) else {}, response.headers.get("ETag"))


# --- Event feed and read models ---


def feed_request(
    *,
    cursor: Optional[FeedCursor] = None,
    prefixes: Optional[Sequence[str]] = None,
    entity_id: Optional[str] = None,
    limit: Optional[int] = None,
) -> Request:
    params: Dict[str, str] = {}
    if cursor is not None:
        params["epoch"] = str(cursor.epoch)
        params["position"] = str(cursor.position)
    if limit is not None:
        params["limit"] = str(limit)
    if prefixes:
        if len(prefixes) > MAX_FEED_PREFIXES:
            raise ValueError(f"at most {MAX_FEED_PREFIXES} feed prefixes, got {len(prefixes)}")
        params["prefixes"] = ",".join(prefixes)
    if entity_id is not None:
        params["entity_id"] = entity_id
    return Request("GET", "/api/v1/event-feed", params=params)


def parse_feed(response: httpx.Response) -> FeedPage:
    raise_for_status(response, conflict=EpochChangedError)
    return FeedPage.from_dict(_body(response) or {})


def read_model_page_request(
    name: str,
    *,
    prefix: Optional[str] = None,
    after: Optional[str] = None,
    limit: Optional[int] = None,
    revision: Optional[int] = None,
) -> Request:
    params: Dict[str, str] = {}
    if prefix is not None:
        params["prefix"] = prefix
    if after is not None:
        params["after"] = after
    if limit is not None:
        params["limit"] = str(limit)
    if revision is not None:
        params["revision"] = str(revision)
    return Request("GET", f"/api/v1/read-models/{segment(name)}", params=params)


def parse_read_model_page(response: httpx.Response) -> ReadModelPage:
    raise_for_status(response)
    return ReadModelPage.from_dict(_body(response) or {})


def read_model_commit_request(
    name: str, commit: Union[ReadModelCommit, Mapping[str, Any]]
) -> Request:
    body = commit.to_dict() if isinstance(commit, ReadModelCommit) else dict(commit)
    return Request("PUT", f"/api/v1/read-models/{segment(name)}", body=body)


def parse_read_model_commit(response: httpx.Response) -> int:
    raise_for_status(response)
    body = _body(response) or {}
    return _int(body.get("revision"))


# --- Views ---


def define_view_request(view: str, definition: Mapping[str, Any]) -> Request:
    return Request("PUT", _view_path(view), body=dict(definition))


def get_view_request(view: str) -> Request:
    return Request("GET", _view_path(view))


def delete_view_request(view: str) -> Request:
    return Request("DELETE", _view_path(view))


def list_views_request() -> Request:
    return Request("GET", "/api/v1/views")


def parse_view_info(response: httpx.Response) -> ViewInfo:
    raise_for_status(response)
    return ViewInfo.from_dict(_body(response) or {})


def parse_views_list(response: httpx.Response) -> List[ViewSummary]:
    raise_for_status(response)
    body = _body(response) or {}
    return [ViewSummary.from_dict(v) for v in body.get("views", [])]


def latest_request(view: str, key: str, *, if_none_match: Optional[str] = None) -> Request:
    return Request(
        "GET",
        f"{_view_path(view)}/entities/{segment(key)}",
        headers=_conditional(if_none_match),
    )


def parse_latest(
    response: httpx.Response, view: str, key: str
) -> Union[ViewLatest, ViewNotFound, NotModified]:
    if response.status_code == 304:
        return NotModified(etag=response.headers.get("ETag"))
    if response.status_code == 404:
        body = _body(response)
        if isinstance(body, dict) and body.get("error") == "not_found":
            return ViewNotFound(view=view, key=key, ready=bool(body.get("ready", False)))
    raise_for_status(response)
    return ViewLatest.from_dict(_body(response) or {}, response.headers.get("ETag"))


def view_entities_request(
    view: str,
    *,
    prefix: Optional[str] = None,
    after: Optional[str] = None,
    limit: Optional[int] = None,
) -> Request:
    params: Dict[str, str] = {}
    if prefix is not None:
        params["prefix"] = prefix
    if after is not None:
        params["after"] = after
    if limit is not None:
        params["limit"] = str(limit)
    return Request("GET", f"{_view_path(view)}/entities", params=params)


def parse_view_entities(response: httpx.Response) -> ViewEntitiesPage:
    raise_for_status(response)
    return ViewEntitiesPage.from_dict(_body(response) or {})


def view_changes_request(
    view: str,
    *,
    after: int,
    limit: Optional[int] = None,
    wait: Optional[int] = None,
) -> Request:
    params: Dict[str, str] = {"after": str(after)}
    if limit is not None:
        params["limit"] = str(limit)
    if wait:
        params["wait"] = str(wait)
    return Request("GET", f"{_view_path(view)}/changes", params=params)


def parse_view_changes(response: httpx.Response) -> ViewChanges:
    raise_for_status(response)
    return ViewChanges.from_dict(_body(response) or {})


class ViewSync:
    """State machine behind `sync`: snapshot every page, then follow `changes`.

    The first `changes` call after a snapshot never waits, because the snapshot
    may be behind; every later call long-polls for `wait` seconds.
    """

    def __init__(
        self,
        view: str,
        *,
        checkpoint: Optional[int],
        wait: int,
        prefix: Optional[str],
        page_limit: Optional[int],
        changes_limit: Optional[int],
        follow: bool,
    ) -> None:
        if not 0 <= wait <= MAX_CHANGES_WAIT_SECONDS:
            raise ValueError(f"wait must be 0..{MAX_CHANGES_WAIT_SECONDS} seconds, got {wait}")
        _view_path(view)
        self._view = view
        self._wait = wait
        self._prefix = prefix
        self._page_limit = page_limit
        self._changes_limit = changes_limit
        self._follow = follow
        self._revision = checkpoint if checkpoint is not None else 0
        self._snapshotting = checkpoint is None
        self._snapshot_after: Optional[str] = None
        self._waiting = checkpoint is not None
        self.done = False

    def request(self) -> Request:
        if self._snapshotting:
            return view_entities_request(
                self._view,
                prefix=self._prefix,
                after=self._snapshot_after,
                limit=self._page_limit,
            )
        return view_changes_request(
            self._view,
            after=self._revision,
            limit=self._changes_limit,
            wait=self._wait if self._waiting else None,
        )

    def accept(self, response: httpx.Response) -> List[SyncItem]:
        if self._snapshotting:
            return self._accept_snapshot(parse_view_entities(response))
        try:
            changes = parse_view_changes(response)
        except ViewResetError as reset:
            self._snapshotting = True
            self._snapshot_after = None
            self._waiting = False
            return [SyncReset(generation=reset.generation)]
        return self._accept_changes(changes)

    def _accept_snapshot(self, page: ViewEntitiesPage) -> List[SyncItem]:
        items: List[SyncItem] = [
            SyncUpsert(key=row.key, state=row.state or {}, version=row.version)
            for row in page.rows
        ]
        if page.next is not None:
            self._snapshot_after = page.next
            return items
        self._snapshotting = False
        self._snapshot_after = None
        self._revision = page.revision
        items.append(SyncCheckpoint(revision=page.revision, generation=page.generation))
        return items

    def _accept_changes(self, page: ViewChanges) -> List[SyncItem]:
        items: List[SyncItem] = []
        for row in page.changes:
            if row.state is None:
                items.append(SyncDelete(key=row.key, version=row.version))
            else:
                items.append(SyncUpsert(key=row.key, state=row.state, version=row.version))
        if items or page.revision != self._revision:
            self._revision = page.revision
            items.append(SyncCheckpoint(revision=page.revision, generation=page.generation))
        self._waiting = True
        if page.caught_up and not self._follow:
            self.done = True
        return items
