"""Synchronous AllSource Event Store client."""

from __future__ import annotations

import time
from typing import Any, Callable, Dict, Iterator, List, Mapping, Optional, Sequence, TypeVar, Union

import httpx

from allsource_client import _protocol as proto
from allsource_client.fold import EventFolder, fold_events
from allsource_client.types import (
    AllSourceError,
    EntityState,
    Event,
    EventList,
    FeedCursor,
    FeedPage,
    NotModified,
    PrimeProjection,
    PrimeProjectionAck,
    PrimeProvenance,
    PrimeSnapshot,
    ReadModelCommit,
    ReadModelPage,
    SyncItem,
    ViewChanges,
    ViewInfo,
    ViewLatest,
    ViewNotFound,
    ViewRow,
    ViewSummary,
)

S = TypeVar("S")


class ReadModel:
    """Handle on one read model: `page` reads rows, `commit` writes them."""

    def __init__(self, client: AllSourceClient, name: str) -> None:
        self._client = client
        self.name = name

    def page(
        self,
        *,
        prefix: Optional[str] = None,
        after: Optional[str] = None,
        limit: Optional[int] = None,
        revision: Optional[int] = None,
    ) -> ReadModelPage:
        """Read one page of rows; `revision` returns `unchanged` when nothing moved."""
        request = proto.read_model_page_request(
            self.name, prefix=prefix, after=after, limit=limit, revision=revision
        )
        return proto.parse_read_model_page(self._client._send(request))

    def commit(self, commit: Union[ReadModelCommit, Mapping[str, Any]]) -> int:
        """Apply changes at `expected_revision` and return the new revision.

        A stale `expected_revision` raises `VersionConflictError`; it is never retried.
        """
        request = proto.read_model_commit_request(self.name, commit)
        return proto.parse_read_model_commit(self._client._send(request))


class AllSourceClient:
    """Synchronous client for the AllSource Event Store API.

    Usage::

        client = AllSourceClient(api_key="as_xxx", base_url="https://api.all-source.xyz")
        event = client.ingest("user.signup", "user-123", {"plan": "pro"})
        events = client.query(event_type="user.signup")
    """

    def __init__(
        self,
        api_key: str,
        base_url: str = "https://api.all-source.xyz",
        timeout: float = 30.0,
    ) -> None:
        self._base_url = base_url.rstrip("/")
        self._http = httpx.Client(
            base_url=self._base_url,
            headers={
                "Authorization": f"Bearer {api_key}",
                "Content-Type": "application/json",
            },
            timeout=timeout,
        )
        self._sleep: Callable[[float], None] = time.sleep

    def close(self) -> None:
        """Close the underlying HTTP connection."""
        self._http.close()

    def __enter__(self) -> AllSourceClient:
        return self

    def __exit__(self, *args: object) -> None:
        self.close()

    def _send(self, request: proto.Request) -> httpx.Response:
        return self._http.request(
            request.method,
            request.path,
            params=request.params or None,
            json=request.body,
            headers=request.headers or None,
        )

    def _request(self, method: str, path: str, **kwargs: Any) -> Any:
        return proto.parse_json(self._http.request(method, path, **kwargs))

    # --- Events ---

    def ingest(
        self,
        event_type: str,
        entity_id: str,
        data: Dict[str, Any],
    ) -> Event:
        """Ingest a single event into the store."""
        request = proto.ingest_request(event_type, entity_id, data)
        return proto.parse_ingest(self._send(request), request.body or {})

    def ingest_batch(
        self,
        events: List[Dict[str, Any]],
    ) -> List[Event]:
        """Ingest a batch of events.

        Each dict should have keys: event_type, entity_id, payload.
        """
        return proto.parse_ingest_batch(self._send(proto.ingest_batch_request(events)), events)

    def query_events(
        self,
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
    ) -> Union[EventList, NotModified]:
        """Query events. `entity_version` is set when filtering by `entity_id`.

        Returns `NotModified` when `if_none_match` still matches.
        """
        request = proto.query_events_request(
            event_type=event_type,
            entity_id=entity_id,
            event_type_prefix=event_type_prefix,
            since=since,
            until=until,
            as_of=as_of,
            payload_filter=payload_filter,
            order=order,
            limit=limit,
            offset=offset,
            if_none_match=if_none_match,
        )
        return proto.parse_query_events(self._send(request))

    def query(
        self,
        *,
        event_type: Optional[str] = None,
        entity_id: Optional[str] = None,
        event_type_prefix: Optional[str] = None,
        since: Optional[str] = None,
        until: Optional[str] = None,
        order: Optional[str] = None,
        limit: Optional[int] = None,
        offset: Optional[int] = None,
    ) -> EventList:
        """Query events with optional filters."""
        result = self.query_events(
            event_type=event_type,
            entity_id=entity_id,
            event_type_prefix=event_type_prefix,
            since=since,
            until=until,
            order=order,
            limit=limit,
            offset=offset,
        )
        assert isinstance(result, EventList)
        return result

    def get_events_by_entity(self, entity_id: str) -> EventList:
        """Get all events for a specific entity."""
        return self.query(entity_id=entity_id)

    def get_events_by_type(self, event_type: str) -> EventList:
        """Get all events of a specific type."""
        return self.query(event_type=event_type)

    def query_and_fold(
        self,
        folder: EventFolder[S],
        *,
        event_type: Optional[str] = None,
        entity_id: Optional[str] = None,
        event_type_prefix: Optional[str] = None,
        since: Optional[str] = None,
        until: Optional[str] = None,
        limit: Optional[int] = None,
    ) -> Optional[S]:
        """Query events and fold them client-side with `folder`."""
        result = self.query(
            event_type=event_type,
            entity_id=entity_id,
            event_type_prefix=event_type_prefix,
            since=since,
            until=until,
            limit=limit,
        )
        return fold_events(folder, result.events)

    def entity_state(
        self,
        entity_id: str,
        *,
        as_of: Optional[str] = None,
        history: bool = False,
        if_none_match: Optional[str] = None,
    ) -> Union[EntityState, NotModified]:
        """Folded state of one entity, without history unless `history=True`."""
        request = proto.entity_state_request(
            entity_id, as_of=as_of, history=history, if_none_match=if_none_match
        )
        return proto.parse_entity_state(self._send(request))

    # --- Event feed and read models ---

    def feed(
        self,
        *,
        cursor: Optional[FeedCursor] = None,
        prefixes: Optional[Sequence[str]] = None,
        entity_id: Optional[str] = None,
        limit: Optional[int] = None,
    ) -> FeedPage:
        """Read one feed page; resume from `page.next_cursor`.

        Raises `EpochChangedError` when the feed generation changed.
        """
        request = proto.feed_request(
            cursor=cursor, prefixes=prefixes, entity_id=entity_id, limit=limit
        )
        return proto.parse_feed(self._send(request))

    def read_model(self, name: str) -> ReadModel:
        """Handle on the read model `name`."""
        return ReadModel(self, name)

    # --- Views ---

    def define_view(self, view: str, definition: Mapping[str, Any]) -> ViewInfo:
        """Create or replace a view definition. Raises `InvalidViewError` when rejected."""
        return proto.parse_view_info(self._send(proto.define_view_request(view, definition)))

    def get_view(self, view: str) -> ViewInfo:
        """Read a view's definition and fold progress."""
        return proto.parse_view_info(self._send(proto.get_view_request(view)))

    def delete_view(self, view: str) -> None:
        """Remove a view's definition and rows."""
        proto.parse_empty(self._send(proto.delete_view_request(view)))

    def list_views(self) -> List[ViewSummary]:
        """List the tenant's views."""
        return proto.parse_views_list(self._send(proto.list_views_request()))

    def latest(
        self,
        view: str,
        key: str,
        *,
        if_none_match: Optional[str] = None,
    ) -> Union[ViewLatest, ViewNotFound, NotModified]:
        """Latest state of one view row."""
        request = proto.latest_request(view, key, if_none_match=if_none_match)
        return proto.parse_latest(self._send(request), view, key)

    def latest_many(
        self,
        view: str,
        *,
        prefix: Optional[str] = None,
        limit: Optional[int] = None,
    ) -> Iterator[ViewRow]:
        """Every row of a view, following `next` across pages."""
        after: Optional[str] = None
        while True:
            request = proto.view_entities_request(view, prefix=prefix, after=after, limit=limit)
            page = proto.parse_view_entities(self._send(request))
            yield from page.rows
            if page.next is None:
                return
            after = page.next

    def view_changes(
        self,
        view: str,
        *,
        after: int,
        limit: Optional[int] = None,
        wait: Optional[int] = None,
    ) -> ViewChanges:
        """Rows written after revision `after`. Raises `ViewResetError` past a reset."""
        request = proto.view_changes_request(view, after=after, limit=limit, wait=wait)
        return proto.parse_view_changes(self._send(request))

    def sync(
        self,
        view: str,
        *,
        checkpoint: Optional[int] = None,
        wait: int = 20,
        prefix: Optional[str] = None,
        page_limit: Optional[int] = None,
        changes_limit: Optional[int] = None,
        follow: bool = True,
    ) -> Iterator[SyncItem]:
        """Snapshot a view (skipped when `checkpoint` is given), then follow its changes.

        Yields upserts, deletes and checkpoints; store each `SyncCheckpoint.revision`
        and pass it back as `checkpoint` to resume. A `SyncReset` precedes a
        re-snapshot. `follow=False` stops once caught up.
        """
        plan = proto.ViewSync(
            view,
            checkpoint=checkpoint,
            wait=wait,
            prefix=prefix,
            page_limit=page_limit,
            changes_limit=changes_limit,
            follow=follow,
        )
        while not plan.done:
            response = self._send(plan.request())
            delay = proto.retry_after_delay(response)
            if delay is not None:
                self._sleep(delay)
                continue
            yield from plan.accept(response)

    # --- Prime ---

    def list_prime_projections(self) -> List[PrimeProjection]:
        """List all Prime projection definitions."""
        result = self._request("GET", "/api/v1/prime/projections")
        data = result.get("data", result)
        if isinstance(data, list):
            return [PrimeProjection.from_dict(p) for p in data]
        return []

    def define_prime_projection(
        self,
        entity_type: str,
        field_policies: Dict[str, str],
    ) -> PrimeProjectionAck:
        """Define (or update) a Prime projection for an entity type."""
        result = self._request(
            "POST",
            "/api/v1/prime/projections",
            json={"entity_type": entity_type, "field_policies": field_policies},
        )
        data = result.get("data", result)
        return PrimeProjectionAck.from_dict(data)

    def project_node(self, node_id: str) -> PrimeSnapshot:
        """Project the current snapshot for a Prime node."""
        result = self._request("POST", f"/api/v1/prime/nodes/{node_id}/project")
        data = result.get("data", result)
        return PrimeSnapshot.from_dict(data)

    def node_field_provenance(
        self,
        node_id: str,
        field: str,
    ) -> Optional[PrimeProvenance]:
        """Get provenance for a single projected field of a Prime node.

        Returns None if no provenance exists for the field (404).
        """
        try:
            result = self._request(
                "GET",
                f"/api/v1/prime/nodes/{node_id}/fields/{field}/provenance",
            )
        except AllSourceError as e:
            if e.status_code == 404:
                return None
            raise
        data = result.get("data", result)
        return PrimeProvenance.from_dict(data)

    # --- Health ---

    def health(self) -> Dict[str, Any]:
        """Check API health."""
        return self._request("GET", "/health")  # type: ignore[no-any-return]
