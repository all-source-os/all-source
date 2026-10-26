"""Tests for the synchronous AllSource client."""

import json
from itertools import islice
from typing import List, Optional

import httpx
import pytest
from pytest_httpx import HTTPXMock

from allsource_client import (
    AllSourceClient,
    AllSourceError,
    EntityState,
    Event,
    EventList,
    NotModified,
    RateLimitedError,
    SyncCheckpoint,
    SyncDelete,
    SyncReset,
    SyncUpsert,
)
from tests.contract import load, respond


BASE_URL = "http://localhost:3902"


@pytest.fixture
def client() -> AllSourceClient:
    return AllSourceClient(api_key="test-key", base_url=BASE_URL)


class TestIngest:
    def test_ingest_event(self, client: AllSourceClient, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/events",
            method="POST",
            json={"event_id": "evt-1", "timestamp": "2026-01-01T00:00:00Z", "version": 3},
        )

        event = client.ingest("user.signup", "user-123", {"plan": "pro"})

        assert event.id == "evt-1"
        assert event.entity_id == "user-123"
        assert event.event_type == "user.signup"
        assert event.payload == {"plan": "pro"}
        assert event.version == 3

        request = httpx_mock.get_request()
        assert request is not None
        assert request.headers["Authorization"] == "Bearer test-key"
        body = json.loads(request.content)
        assert body["event_type"] == "user.signup"
        assert body["entity_id"] == "user-123"
        assert body["payload"] == {"plan": "pro"}

    def test_ingest_batch(self, client: AllSourceClient, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/events/batch",
            method="POST",
            json={
                "total": 2,
                "ingested": 2,
                "events": [
                    {"event_id": "evt-1", "timestamp": "2026-01-01T00:00:00Z", "version": 1},
                    {"event_id": "evt-2", "timestamp": "2026-01-01T00:00:01Z", "version": 1},
                ],
            },
        )

        events = client.ingest_batch([
            {"event_type": "user.signup", "entity_id": "user-1", "payload": {}},
            {"event_type": "user.signup", "entity_id": "user-2", "payload": {}},
        ])

        assert [(e.id, e.entity_id) for e in events] == [("evt-1", "user-1"), ("evt-2", "user-2")]
        request = httpx_mock.get_request()
        assert request is not None
        assert len(json.loads(request.content)["events"]) == 2


class TestQuery:
    def test_query_events(self, client: AllSourceClient, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            url=httpx.URL(
                f"{BASE_URL}/api/v1/events/query",
                params={"event_type": "user.signup", "since": "2026-01-01T00:00:00Z", "limit": "10"},
            ),
            method="GET",
            json={
                "events": [
                    {
                        "id": "evt-1",
                        "entity_id": "user-123",
                        "event_type": "user.signup",
                        "payload": {},
                        "timestamp": "2026-01-01T00:00:00Z",
                        "version": 1,
                    }
                ],
                "count": 1,
            },
        )

        result = client.query(event_type="user.signup", since="2026-01-01T00:00:00Z", limit=10)

        assert result.count == 1
        assert len(result.events) == 1
        assert result.events[0].event_type == "user.signup"
        assert result.entity_version is None

    def test_query_by_entity(self, client: AllSourceClient, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            url=httpx.URL(f"{BASE_URL}/api/v1/events/query", params={"entity_id": "user-123"}),
            method="GET",
            json={"events": [], "count": 0},
        )

        result = client.get_events_by_entity("user-123")
        assert result.count == 0
        assert result.events == []

    def test_query_by_type(self, client: AllSourceClient, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            url=httpx.URL(f"{BASE_URL}/api/v1/events/query", params={"event_type": "user.signup"}),
            method="GET",
            json={"events": [], "count": 0},
        )

        result = client.get_events_by_type("user.signup")
        assert result.count == 0

    def test_payload_filter_is_sent_as_compact_json(
        self, client: AllSourceClient, httpx_mock: HTTPXMock
    ) -> None:
        httpx_mock.add_response(json={"events": [], "count": 0})

        client.query_events(payload_filter={"user_id": "abc"}, order="desc")

        request = httpx_mock.get_request()
        assert request is not None
        assert request.url.params["payload_filter"] == '{"user_id":"abc"}'
        assert request.url.params["order"] == "desc"

    def test_query_events_not_modified(
        self, client: AllSourceClient, httpx_mock: HTTPXMock
    ) -> None:
        httpx_mock.add_response(status_code=304, headers={"ETag": '"sha256:aa"'}, content=b"")

        result = client.query_events(entity_id="skill:abc", if_none_match='"sha256:aa"')

        assert result == NotModified(etag='"sha256:aa"')
        request = httpx_mock.get_request()
        assert request is not None
        assert request.headers["If-None-Match"] == '"sha256:aa"'


class CountingFolder:
    def __init__(self) -> None:
        self.names: List[str] = []

    def apply(self, event: Event) -> bool:
        name = event.payload.get("name")
        if not isinstance(name, str):
            return False
        self.names.append(name)
        return True

    def finalize(self) -> Optional[List[str]]:
        return self.names or None


class TestQueryAndFold:
    def test_folds_events_in_order(self, client: AllSourceClient, httpx_mock: HTTPXMock) -> None:
        event = {
            "entity_id": "skill:abc",
            "event_type": "skill.renamed",
            "timestamp": "2026-01-01T00:00:00Z",
            "version": 1,
        }
        httpx_mock.add_response(
            json={
                "events": [
                    {**event, "id": "e1", "payload": {"name": "a"}},
                    {**event, "id": "e2", "payload": {}},
                    {**event, "id": "e3", "payload": {"name": "b"}},
                ],
                "count": 3,
            }
        )

        assert client.query_and_fold(CountingFolder(), entity_id="skill:abc") == ["a", "b"]

    def test_returns_none_when_nothing_applies(
        self, client: AllSourceClient, httpx_mock: HTTPXMock
    ) -> None:
        httpx_mock.add_response(json={"events": [], "count": 0})

        assert client.query_and_fold(CountingFolder(), entity_id="skill:abc") is None


class TestEntityState:
    def test_history_is_sent_only_when_asked(
        self, client: AllSourceClient, httpx_mock: HTTPXMock
    ) -> None:
        body = load("entity_state")["response"]["body"]
        httpx_mock.add_response(json={**body, "history": [{"event_type": "skill.created"}]})

        state = client.entity_state("skill:abc", history=True, as_of="2026-10-27T10:00:00Z")

        assert isinstance(state, EntityState)
        assert state.history == [{"event_type": "skill.created"}]
        request = httpx_mock.get_request()
        assert request is not None
        assert dict(request.url.params) == {"history": "true", "as_of": "2026-10-27T10:00:00Z"}


class TestFeed:
    def test_rejects_more_than_eight_prefixes(self, client: AllSourceClient) -> None:
        with pytest.raises(ValueError):
            client.feed(prefixes=[f"p{i}." for i in range(9)])

    def test_first_page_sends_no_cursor(
        self, client: AllSourceClient, httpx_mock: HTTPXMock
    ) -> None:
        httpx_mock.add_response(json=load("event_feed")["response"]["body"])

        client.feed(entity_id="skill:abc")

        request = httpx_mock.get_request()
        assert request is not None
        assert dict(request.url.params) == {"entity_id": "skill:abc"}


class TestViews:
    def test_rejects_invalid_view_name(self, client: AllSourceClient) -> None:
        with pytest.raises(ValueError):
            client.get_view("Ops Status")

    def test_delete_view(self, client: AllSourceClient, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/views/ops_status", method="DELETE", status_code=204
        )

        client.delete_view("ops_status")

    def test_latest_many_sends_prefix(
        self, client: AllSourceClient, httpx_mock: HTTPXMock
    ) -> None:
        httpx_mock.add_response(json=load("view_entities_page_2")["response"]["body"])

        rows = list(client.latest_many("ops_status", prefix="probe-z:"))

        assert [row.key for row in rows] == ["probe-z:2026-10-20"]
        request = httpx_mock.get_request()
        assert request is not None
        assert dict(request.url.params) == {"prefix": "probe-z:"}


class TestSync:
    def test_snapshot_then_changes(self, client: AllSourceClient, httpx_mock: HTTPXMock) -> None:
        respond(httpx_mock, load("view_entities_page_1"))
        respond(httpx_mock, load("view_entities_page_2"))
        respond(httpx_mock, load("view_changes"))

        items = list(client.sync("ops_status", page_limit=2, follow=False))

        assert items == [
            SyncUpsert(key="probe-a:2026-10-26", state={"ok": 1440, "failed": 0}, version=40),
            SyncUpsert(key="probe-a:2026-10-27", state={"ok": 1430, "failed": 2}, version=57),
            SyncUpsert(key="probe-z:2026-10-20", state={"ok": 12, "failed": 9}, version=33),
            SyncCheckpoint(revision=61, generation=2),
            SyncUpsert(key="probe-a:2026-10-27", state={"ok": 1431, "failed": 2}, version=62),
            SyncDelete(key="probe-z:2026-10-20", version=62),
            SyncCheckpoint(revision=62, generation=2),
        ]
        changes_request = httpx_mock.get_requests()[2]
        assert dict(changes_request.url.params) == {"after": "61"}

    def test_follow_keeps_polling_through_an_empty_long_poll(
        self, client: AllSourceClient, httpx_mock: HTTPXMock
    ) -> None:
        later = {
            "view": "ops_status",
            "changes": [{"key": "probe-b:2026-10-27", "version": 63, "state": {"ok": 1}}],
            "revision": 63,
            "caught_up": True,
            "generation": 2,
        }
        respond(httpx_mock, load("view_changes"))
        respond(httpx_mock, load("view_changes_empty_after_wait"))
        httpx_mock.add_response(json=later)

        stream = client.sync("ops_status", checkpoint=61, wait=20)
        items = list(islice(stream, 4))
        stream.close()

        assert items[2] == SyncCheckpoint(revision=62, generation=2)
        assert items[3] == SyncUpsert(key="probe-b:2026-10-27", state={"ok": 1}, version=63)
        assert [dict(r.url.params) for r in httpx_mock.get_requests()] == [
            {"after": "61", "wait": "20"},
            {"after": "62", "wait": "20"},
            {"after": "62", "wait": "20"},
        ]

    def test_view_reset_resnapshots(self, client: AllSourceClient, httpx_mock: HTTPXMock) -> None:
        respond(httpx_mock, load("view_changes_reset"))
        respond(httpx_mock, load("view_entities_page_2"))
        respond(httpx_mock, load("view_changes"))

        items = list(client.sync("ops_status", checkpoint=12, wait=0, follow=False))

        assert items[:3] == [
            SyncReset(generation=3),
            SyncUpsert(key="probe-z:2026-10-20", state={"ok": 12, "failed": 9}, version=33),
            SyncCheckpoint(revision=61, generation=2),
        ]
        assert items[-1] == SyncCheckpoint(revision=62, generation=2)
        snapshot_request = httpx_mock.get_requests()[1]
        assert snapshot_request.url.path == "/api/v1/views/ops_status/entities"
        assert dict(snapshot_request.url.params) == {}

    def test_rate_limit_without_retry_after_raises(
        self, client: AllSourceClient, httpx_mock: HTTPXMock
    ) -> None:
        httpx_mock.add_response(status_code=503, content=b"")
        sleeps: List[float] = []
        client._sleep = sleeps.append

        with pytest.raises(RateLimitedError) as err:
            list(client.sync("ops_status", checkpoint=61, wait=0, follow=False))

        assert err.value.status_code == 503
        assert err.value.retry_after is None
        assert sleeps == []

    def test_503_retry_after_is_honoured(
        self, client: AllSourceClient, httpx_mock: HTTPXMock
    ) -> None:
        httpx_mock.add_response(status_code=503, headers={"Retry-After": "2"}, content=b"")
        respond(httpx_mock, load("view_changes"))
        sleeps: List[float] = []
        client._sleep = sleeps.append

        items = list(client.sync("ops_status", checkpoint=61, wait=0, follow=False))

        assert sleeps == [2.0]
        assert items[-1] == SyncCheckpoint(revision=62, generation=2)

    def test_rejects_wait_above_twenty_seconds(self, client: AllSourceClient) -> None:
        with pytest.raises(ValueError):
            next(client.sync("ops_status", wait=21))


class TestPrime:
    def test_list_prime_projections(self, client: AllSourceClient, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/prime/projections",
            method="GET",
            json={
                "data": [
                    {
                        "entity_type": "person",
                        "field_policies": {"name": "last_write", "tags": "merge_array"},
                    }
                ],
                "count": 1,
            },
        )

        projections = client.list_prime_projections()

        assert len(projections) == 1
        assert projections[0].entity_type == "person"
        assert projections[0].field_policies["name"] == "last_write"
        assert projections[0].field_policies["tags"] == "merge_array"

    def test_define_prime_projection(self, client: AllSourceClient, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/prime/projections",
            method="POST",
            status_code=201,
            json={"data": {"entity_type": "person", "persisted": True}},
        )

        ack = client.define_prime_projection(
            "person",
            {"name": "highest_priority", "email": "most_specific"},
        )

        assert ack.entity_type == "person"
        assert ack.persisted is True

        request = httpx_mock.get_request()
        assert request is not None
        body = json.loads(request.content)
        assert body["entity_type"] == "person"
        assert body["field_policies"] == {
            "name": "highest_priority",
            "email": "most_specific",
        }

    def test_project_node(self, client: AllSourceClient, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/prime/nodes/person:alice/project",
            method="POST",
            json={
                "data": {
                    "entity_type": "person",
                    "fields": {"name": "Alice", "email": "alice@example.com"},
                    "observation_count": 3,
                }
            },
        )

        snapshot = client.project_node("person:alice")

        assert snapshot.entity_type == "person"
        assert snapshot.fields["name"] == "Alice"
        assert snapshot.observation_count == 3

    def test_node_field_provenance(self, client: AllSourceClient, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/prime/nodes/person:alice/fields/name/provenance",
            method="GET",
            json={
                "data": {
                    "field": "name",
                    "value": "Alice",
                    "source_event_id": "evt-42",
                    "source_event_at": "2026-01-01T00:00:00Z",
                    "merge_policy_applied": "last_write",
                }
            },
        )

        prov = client.node_field_provenance("person:alice", "name")

        assert prov is not None
        assert prov.field == "name"
        assert prov.value == "Alice"
        assert prov.source_event_id == "evt-42"
        assert prov.merge_policy_applied == "last_write"

    def test_node_field_provenance_404(self, client: AllSourceClient, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/prime/nodes/person:alice/fields/missing/provenance",
            method="GET",
            status_code=404,
            json={"error": {"code": "not_found", "message": "no provenance"}},
        )

        prov = client.node_field_provenance("person:alice", "missing")
        assert prov is None


class TestErrorHandling:
    def test_api_error(self, client: AllSourceClient, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/events",
            method="POST",
            status_code=422,
            json={
                "error": {
                    "code": "validation_error",
                    "message": "entity_id is required",
                }
            },
        )

        with pytest.raises(AllSourceError) as exc_info:
            client.ingest("user.signup", "", {})

        assert exc_info.value.code == "validation_error"
        assert exc_info.value.status_code == 422
        assert "entity_id is required" in str(exc_info.value)

    def test_string_error_code(self, client: AllSourceClient, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            status_code=403, json={"error": "reserved_read_model", "message": "reserved"}
        )

        with pytest.raises(AllSourceError) as exc_info:
            client.read_model("views_registry").page()

        assert exc_info.value.code == "reserved_read_model"
        assert exc_info.value.status_code == 403

    def test_non_json_error(self, client: AllSourceClient, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(status_code=502, text="bad gateway")

        with pytest.raises(AllSourceError) as exc_info:
            client.list_views()

        assert exc_info.value.code == "api_error"
        assert str(exc_info.value) == "bad gateway"

    def test_auth_header(self, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/health",
            method="GET",
            json={"status": "ok"},
        )

        c = AllSourceClient(api_key="my-secret-key", base_url=BASE_URL)
        c.health()

        request = httpx_mock.get_request()
        assert request is not None
        assert request.headers["Authorization"] == "Bearer my-secret-key"
        c.close()

    def test_context_manager(self, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/health",
            method="GET",
            json={"status": "ok"},
        )

        with AllSourceClient(api_key="key", base_url=BASE_URL) as c:
            result = c.health()
            assert result["status"] == "ok"


def test_event_list_is_returned_for_plain_query(
    client: AllSourceClient, httpx_mock: HTTPXMock
) -> None:
    httpx_mock.add_response(json={"events": [], "count": 0, "entity_version": 4})

    result = client.query(entity_id="skill:abc")

    assert isinstance(result, EventList)
    assert result.entity_version == 4
