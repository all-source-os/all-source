"""Tests for the async AllSource client."""

import json
from typing import List, Optional

import pytest
from pytest_httpx import HTTPXMock

from allsource_client import (
    AllSourceAsyncClient,
    AllSourceError,
    EntityState,
    EpochChangedError,
    Event,
    EventList,
    FeedCursor,
    InvalidViewError,
    NotModified,
    ReadModelCommit,
    SyncCheckpoint,
    SyncDelete,
    SyncReset,
    SyncItem,
    SyncUpsert,
    VersionConflictError,
    ViewLatest,
    ViewNotFound,
    ViewResetError,
)
from tests.contract import assert_request, load, respond


BASE_URL = "http://localhost:3902"


@pytest.fixture
def client() -> AllSourceAsyncClient:
    return AllSourceAsyncClient(api_key="test-key", base_url=BASE_URL)


class TestAsyncIngest:
    async def test_ingest_event(self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/events",
            method="POST",
            json={"event_id": "evt-1", "timestamp": "2026-01-01T00:00:00Z", "version": 1},
        )

        event = await client.ingest("user.signup", "user-123", {"plan": "pro"})

        assert event.id == "evt-1"
        assert event.entity_id == "user-123"
        assert event.event_type == "user.signup"
        await client.close()

    async def test_ingest_batch(self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/events/batch",
            method="POST",
            json={
                "total": 1,
                "ingested": 1,
                "events": [{"event_id": "evt-1", "timestamp": "2026-01-01T00:00:00Z", "version": 1}],
            },
        )

        events = await client.ingest_batch([
            {"event_type": "user.signup", "entity_id": "u-1", "payload": {}}
        ])
        assert [(e.id, e.entity_id) for e in events] == [("evt-1", "u-1")]
        await client.close()


class TestAsyncQuery:
    async def test_query_events(self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/events/query",
            method="GET",
            json={"events": [], "count": 0},
        )

        result = await client.query()
        assert result.count == 0
        await client.close()

    async def test_entity_version_and_etag(
        self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock
    ) -> None:
        fx = load("events_query_with_entity_version")
        respond(httpx_mock, fx)

        result = await client.query_events(entity_id="skill:abc", limit=1)

        assert isinstance(result, EventList)
        assert result.entity_version == 12
        assert result.etag == fx["response"]["headers"]["ETag"]
        assert_request(httpx_mock.get_requests()[0], fx)
        await client.close()

    async def test_query_and_fold(
        self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock
    ) -> None:
        respond(httpx_mock, load("events_query_with_entity_version"))

        class LastName:
            def __init__(self) -> None:
                self.name: Optional[str] = None

            def apply(self, event: Event) -> bool:
                self.name = event.payload.get("name")
                return True

            def finalize(self) -> Optional[str]:
                return self.name

        assert await client.query_and_fold(LastName(), entity_id="skill:abc") == "x"
        await client.close()


class TestAsyncEntityState:
    async def test_state_then_not_modified(
        self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock
    ) -> None:
        fresh = load("entity_state")
        unchanged = load("entity_state_not_modified")
        respond(httpx_mock, fresh)
        respond(httpx_mock, unchanged)

        state = await client.entity_state("skill:abc")
        assert isinstance(state, EntityState)
        assert state.version == 12
        again = await client.entity_state("skill:abc", if_none_match=state.etag)

        assert again == NotModified(etag=state.etag)
        requests = httpx_mock.get_requests()
        assert_request(requests[0], fresh)
        assert_request(requests[1], unchanged)
        await client.close()


class TestAsyncFeedAndReadModels:
    async def test_feed(self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock) -> None:
        fx = load("event_feed")
        respond(httpx_mock, fx)

        page = await client.feed(
            cursor=FeedCursor(epoch=41, position=0), prefixes=["skill."], limit=100
        )

        assert page.next_cursor == FeedCursor(epoch=41, position=1)
        assert_request(httpx_mock.get_requests()[0], fx)
        await client.close()

    async def test_feed_epoch_changed(
        self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock
    ) -> None:
        respond(httpx_mock, load("event_feed_epoch_changed"))

        with pytest.raises(EpochChangedError) as err:
            await client.feed(cursor=FeedCursor(epoch=40, position=17))

        assert err.value.current_version == 41
        await client.close()

    async def test_read_model_page_and_commit(
        self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock
    ) -> None:
        page_fx = load("read_model_page")
        commit_fx = load("read_model_commit")
        respond(httpx_mock, page_fx)
        respond(httpx_mock, commit_fx)

        model = client.read_model("skills")
        page = await model.page(prefix="skill:", limit=100)
        assert page.checkpoint is not None
        revision = await model.commit(
            ReadModelCommit(
                expected_revision=page.revision,
                checkpoint=page.checkpoint,
                changes={"skill:abc": {"status": "retired"}, "skill:old": None},
            )
        )

        assert revision == 8
        requests = httpx_mock.get_requests()
        assert_request(requests[0], page_fx)
        assert_request(requests[1], commit_fx)
        await client.close()

    async def test_commit_conflict(
        self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock
    ) -> None:
        fx = load("read_model_commit_conflict")
        respond(httpx_mock, fx)

        with pytest.raises(VersionConflictError) as err:
            await client.read_model("skills").commit(fx["request"]["body"])

        assert err.value.current_version == 7
        await client.close()


class TestAsyncViews:
    async def test_define_get_list_delete(
        self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock
    ) -> None:
        define = load("view_define")
        respond(httpx_mock, define)
        respond(httpx_mock, load("view_get"))
        respond(httpx_mock, load("views_list"))
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/views/ops_status", method="DELETE", status_code=204
        )

        created = await client.define_view("ops_status", define["request"]["body"])
        fetched = await client.get_view("ops_status")
        views = await client.list_views()
        await client.delete_view("ops_status")

        assert created.generation == 1
        assert fetched.generation == 2
        assert [v.view for v in views] == ["ops_status", "skill_catalog"]
        assert_request(httpx_mock.get_requests()[0], define)
        await client.close()

    async def test_define_invalid(
        self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock
    ) -> None:
        fx = load("view_define_invalid")
        respond(httpx_mock, fx)

        with pytest.raises(InvalidViewError) as err:
            await client.define_view("ops_status", fx["request"]["body"])

        assert err.value.detail == "ops must contain 1..64 operations"
        await client.close()

    async def test_latest_variants(
        self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock
    ) -> None:
        respond(httpx_mock, load("view_latest"))
        respond(httpx_mock, load("view_latest_not_modified"))
        respond(httpx_mock, load("view_latest_not_found"))

        row = await client.latest("ops_status", "probe-a:2026-10-27")
        assert isinstance(row, ViewLatest)
        unchanged = await client.latest("ops_status", "probe-a:2026-10-27", if_none_match=row.etag)
        missing = await client.latest("ops_status", "probe-q:2026-10-27")

        assert row.version == 57
        assert unchanged == NotModified(etag=row.etag)
        assert missing == ViewNotFound(view="ops_status", key="probe-q:2026-10-27", ready=True)
        await client.close()

    async def test_latest_many(self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock) -> None:
        page_1 = load("view_entities_page_1")
        page_2 = load("view_entities_page_2")
        respond(httpx_mock, page_1)
        respond(httpx_mock, page_2)

        keys = [row.key async for row in client.latest_many("ops_status", limit=2)]

        assert keys == ["probe-a:2026-10-26", "probe-a:2026-10-27", "probe-z:2026-10-20"]
        requests = httpx_mock.get_requests()
        assert_request(requests[0], page_1)
        assert_request(requests[1], page_2)
        await client.close()

    async def test_view_changes_reset(
        self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock
    ) -> None:
        respond(httpx_mock, load("view_changes_reset"))

        with pytest.raises(ViewResetError) as err:
            await client.view_changes("ops_status", after=12)

        assert err.value.generation == 3
        await client.close()


class TestAsyncSync:
    async def test_snapshot_then_changes(
        self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock
    ) -> None:
        respond(httpx_mock, load("view_entities_page_1"))
        respond(httpx_mock, load("view_entities_page_2"))
        respond(httpx_mock, load("view_changes"))

        items: List[SyncItem] = [
            item async for item in client.sync("ops_status", page_limit=2, follow=False)
        ]

        assert items[3] == SyncCheckpoint(revision=61, generation=2)
        assert items[4:] == [
            SyncUpsert(key="probe-a:2026-10-27", state={"ok": 1431, "failed": 2}, version=62),
            SyncDelete(key="probe-z:2026-10-20", version=62),
            SyncCheckpoint(revision=62, generation=2),
        ]
        await client.close()

    async def test_reset_resnapshots(
        self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock
    ) -> None:
        respond(httpx_mock, load("view_changes_reset"))
        respond(httpx_mock, load("view_entities_page_2"))
        respond(httpx_mock, load("view_changes"))

        items = [
            item
            async for item in client.sync("ops_status", checkpoint=12, wait=0, follow=False)
        ]

        assert items[0] == SyncReset(generation=3)
        assert items[2] == SyncCheckpoint(revision=61, generation=2)
        await client.close()

    async def test_retry_after_is_honoured(
        self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock
    ) -> None:
        limited = load("rate_limited_retry_after")
        respond(httpx_mock, limited)
        respond(httpx_mock, load("view_changes"))
        sleeps: List[float] = []

        async def record(delay: float) -> None:
            sleeps.append(delay)

        client._sleep = record
        items = [
            item
            async for item in client.sync("ops_status", checkpoint=61, wait=0, follow=False)
        ]

        assert sleeps == [1.0]
        assert items[-1] == SyncCheckpoint(revision=62, generation=2)
        for request in httpx_mock.get_requests():
            assert_request(request, limited)
        await client.close()

    async def test_empty_long_poll_yields_nothing(
        self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock
    ) -> None:
        fx = load("view_changes_empty_after_wait")
        respond(httpx_mock, fx)

        items = [
            item
            async for item in client.sync("ops_status", checkpoint=62, wait=20, follow=False)
        ]

        assert items == []
        assert_request(httpx_mock.get_requests()[0], fx)
        await client.close()


class TestAsyncPrime:
    async def test_list_prime_projections(
        self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock
    ) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/prime/projections",
            method="GET",
            json={
                "data": [
                    {
                        "entity_type": "person",
                        "field_policies": {"name": "last_write"},
                    }
                ],
                "count": 1,
            },
        )

        projections = await client.list_prime_projections()
        assert len(projections) == 1
        assert projections[0].entity_type == "person"
        assert projections[0].field_policies["name"] == "last_write"
        await client.close()

    async def test_define_prime_projection(
        self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock
    ) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/prime/projections",
            method="POST",
            status_code=201,
            json={"data": {"entity_type": "person", "persisted": True}},
        )

        ack = await client.define_prime_projection(
            "person", {"name": "merge_array"}
        )
        assert ack.entity_type == "person"
        assert ack.persisted is True

        request = httpx_mock.get_request()
        assert request is not None
        body = json.loads(request.content)
        assert body["entity_type"] == "person"
        assert body["field_policies"] == {"name": "merge_array"}
        await client.close()

    async def test_project_node(
        self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock
    ) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/prime/nodes/person:alice/project",
            method="POST",
            json={
                "data": {
                    "entity_type": "person",
                    "fields": {"name": "Alice"},
                    "observation_count": 2,
                }
            },
        )

        snapshot = await client.project_node("person:alice")
        assert snapshot.entity_type == "person"
        assert snapshot.fields["name"] == "Alice"
        assert snapshot.observation_count == 2
        await client.close()

    async def test_node_field_provenance(
        self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock
    ) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/prime/nodes/person:alice/fields/name/provenance",
            method="GET",
            json={
                "data": {
                    "field": "name",
                    "value": "Alice",
                    "source_event_id": "evt-7",
                    "source_event_at": "2026-01-01T00:00:00Z",
                    "merge_policy_applied": "last_write",
                }
            },
        )

        prov = await client.node_field_provenance("person:alice", "name")
        assert prov is not None
        assert prov.field == "name"
        assert prov.source_event_id == "evt-7"
        await client.close()

    async def test_node_field_provenance_404(
        self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock
    ) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/prime/nodes/person:alice/fields/missing/provenance",
            method="GET",
            status_code=404,
            json={"error": {"code": "not_found", "message": "no provenance"}},
        )

        prov = await client.node_field_provenance("person:alice", "missing")
        assert prov is None
        await client.close()


class TestAsyncErrorHandling:
    async def test_api_error(self, client: AllSourceAsyncClient, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/events",
            method="POST",
            status_code=401,
            json={"error": {"code": "unauthorized", "message": "Invalid API key"}},
        )

        with pytest.raises(AllSourceError) as exc_info:
            await client.ingest("test", "test", {})

        assert exc_info.value.code == "unauthorized"
        assert exc_info.value.status_code == 401
        await client.close()

    async def test_async_context_manager(self, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            url=f"{BASE_URL}/health",
            method="GET",
            json={"status": "ok"},
        )

        async with AllSourceAsyncClient(api_key="key", base_url=BASE_URL) as c:
            result = await c.health()
            assert result["status"] == "ok"
