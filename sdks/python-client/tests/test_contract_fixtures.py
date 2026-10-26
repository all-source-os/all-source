"""Every fixture in sdks/contract/fixtures, driven through the sync client."""

from typing import Any, Callable, Dict, List

import pytest
from pytest_httpx import HTTPXMock

from allsource_client import (
    AllSourceClient,
    EntityState,
    EpochChangedError,
    EventList,
    FeedCursor,
    InvalidViewError,
    NotModified,
    ReadModelCommit,
    SyncCheckpoint,
    SyncDelete,
    SyncUpsert,
    VersionConflictError,
    ViewLatest,
    ViewNotFound,
    ViewResetError,
)
from tests.contract import assert_request, fixture_names, load, respond

BASE_URL = "http://localhost:3902"

Driver = Callable[[AllSourceClient, HTTPXMock, Dict[str, Any]], None]


def _only_request(httpx_mock: HTTPXMock, fixture: Dict[str, Any]) -> None:
    requests = httpx_mock.get_requests()
    assert len(requests) == 1
    assert_request(requests[0], fixture)


def entity_state(client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]) -> None:
    respond(httpx_mock, fx)
    state = client.entity_state("skill:abc")
    assert isinstance(state, EntityState)
    assert state.current_state == {"name": "x", "status": "published"}
    assert state.version == 12
    assert state.event_count == 12
    assert state.history is None
    assert state.etag == fx["response"]["headers"]["ETag"]
    _only_request(httpx_mock, fx)


def entity_state_not_modified(
    client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]
) -> None:
    respond(httpx_mock, fx)
    tag = fx["request"]["headers"]["If-None-Match"]
    assert client.entity_state("skill:abc", if_none_match=tag) == NotModified(etag=tag)
    _only_request(httpx_mock, fx)


def event_feed(client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]) -> None:
    respond(httpx_mock, fx)
    page = client.feed(cursor=FeedCursor(epoch=41, position=0), prefixes=["skill."], limit=100)
    assert page.next_cursor == FeedCursor(epoch=41, position=1)
    assert page.head == FeedCursor(epoch=41, position=3)
    assert page.caught_up is True
    assert [e.event_type for e in page.events] == ["skill.published"]
    _only_request(httpx_mock, fx)


def event_feed_epoch_changed(
    client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]
) -> None:
    respond(httpx_mock, fx)
    with pytest.raises(EpochChangedError) as err:
        client.feed(cursor=FeedCursor(epoch=40, position=17))
    assert err.value.expected_version == 40
    assert err.value.current_version == 41
    _only_request(httpx_mock, fx)


def events_query_with_entity_version(
    client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]
) -> None:
    respond(httpx_mock, fx)
    result = client.query_events(entity_id="skill:abc", limit=1)
    assert isinstance(result, EventList)
    assert result.entity_version == 12
    assert result.total_count == 12
    assert result.has_more is True
    assert result.etag == fx["response"]["headers"]["ETag"]
    assert result.events[0].payload == {"name": "x"}
    _only_request(httpx_mock, fx)


def rate_limited_retry_after(
    client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]
) -> None:
    sleeps: List[float] = []
    client._sleep = sleeps.append
    respond(httpx_mock, fx)
    respond(httpx_mock, load("view_changes"))
    items = list(client.sync("ops_status", checkpoint=61, wait=0, follow=False))
    assert sleeps == [1.0]
    assert items[-1] == SyncCheckpoint(revision=62, generation=2)
    requests = httpx_mock.get_requests()
    assert len(requests) == 2
    assert_request(requests[0], fx)
    assert_request(requests[1], fx)


def read_model_commit(client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]) -> None:
    respond(httpx_mock, fx)
    revision = client.read_model("skills").commit(
        ReadModelCommit(
            expected_revision=7,
            checkpoint=FeedCursor(epoch=41, position=3),
            changes={"skill:abc": {"status": "retired"}, "skill:old": None},
        )
    )
    assert revision == 8
    _only_request(httpx_mock, fx)


def read_model_commit_conflict(
    client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]
) -> None:
    respond(httpx_mock, fx)
    with pytest.raises(VersionConflictError) as err:
        client.read_model("skills").commit(
            ReadModelCommit(
                expected_revision=6,
                checkpoint=FeedCursor(epoch=41, position=3),
                changes={"skill:abc": {"status": "retired"}},
            )
        )
    assert not isinstance(err.value, EpochChangedError)
    assert err.value.current_version == 7
    _only_request(httpx_mock, fx)


def read_model_page(client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]) -> None:
    respond(httpx_mock, fx)
    page = client.read_model("skills").page(prefix="skill:", limit=100)
    assert page.rows == {"skill:abc": {"status": "published"}}
    assert page.content_revision == 7
    assert page.checkpoint == FeedCursor(epoch=41, position=3)
    assert page.next is None
    assert page.unchanged is False
    _only_request(httpx_mock, fx)


def read_model_unchanged(
    client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]
) -> None:
    respond(httpx_mock, fx)
    page = client.read_model("skills").page(prefix="skill:", revision=7)
    assert page.unchanged is True
    assert page.rows == {}
    _only_request(httpx_mock, fx)


def view_changes(client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]) -> None:
    respond(httpx_mock, fx)
    items = list(client.sync("ops_status", checkpoint=61, wait=0, follow=False))
    assert items == [
        SyncUpsert(key="probe-a:2026-10-27", state={"ok": 1431, "failed": 2}, version=62),
        SyncDelete(key="probe-z:2026-10-20", version=62),
        SyncCheckpoint(revision=62, generation=2),
    ]
    _only_request(httpx_mock, fx)


def view_changes_empty_after_wait(
    client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]
) -> None:
    respond(httpx_mock, fx)
    assert list(client.sync("ops_status", checkpoint=62, wait=20, follow=False)) == []
    _only_request(httpx_mock, fx)


def view_changes_reset(client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]) -> None:
    respond(httpx_mock, fx)
    with pytest.raises(ViewResetError) as err:
        client.view_changes("ops_status", after=12)
    assert err.value.generation == 3
    _only_request(httpx_mock, fx)


def view_define(client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]) -> None:
    respond(httpx_mock, fx)
    info = client.define_view("ops_status", fx["request"]["body"])
    assert info.generation == 1
    assert info.ready is False
    assert info.checkpoint is None
    assert info.definition == fx["request"]["body"]
    _only_request(httpx_mock, fx)


def view_define_invalid(
    client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]
) -> None:
    respond(httpx_mock, fx)
    with pytest.raises(InvalidViewError) as err:
        client.define_view("ops_status", fx["request"]["body"])
    assert err.value.detail == "ops must contain 1..64 operations"
    assert err.value.status_code == 400
    _only_request(httpx_mock, fx)


def _latest_many_two_pages(client: AllSourceClient, httpx_mock: HTTPXMock) -> None:
    respond(httpx_mock, load("view_entities_page_1"))
    respond(httpx_mock, load("view_entities_page_2"))
    rows = list(client.latest_many("ops_status", limit=2))
    assert [row.key for row in rows] == [
        "probe-a:2026-10-26",
        "probe-a:2026-10-27",
        "probe-z:2026-10-20",
    ]
    assert rows[2].state == {"ok": 12, "failed": 9}


def view_entities_page_1(
    client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]
) -> None:
    _latest_many_two_pages(client, httpx_mock)
    assert_request(httpx_mock.get_requests()[0], fx)


def view_entities_page_2(
    client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]
) -> None:
    _latest_many_two_pages(client, httpx_mock)
    assert_request(httpx_mock.get_requests()[1], fx)


def view_get(client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]) -> None:
    respond(httpx_mock, fx)
    info = client.get_view("ops_status")
    assert info.generation == 2
    assert info.revision == 62
    assert info.checkpoint == FeedCursor(epoch=41, position=9120)
    assert info.ready is True
    _only_request(httpx_mock, fx)


def view_latest(client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]) -> None:
    respond(httpx_mock, fx)
    row = client.latest("ops_status", "probe-a:2026-10-27")
    assert isinstance(row, ViewLatest)
    assert row.state == {"ok": 1430, "failed": 2}
    assert row.version == 57
    assert row.etag == fx["response"]["headers"]["ETag"]
    _only_request(httpx_mock, fx)


def view_latest_not_found(
    client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]
) -> None:
    respond(httpx_mock, fx)
    result = client.latest("ops_status", "probe-q:2026-10-27")
    assert result == ViewNotFound(view="ops_status", key="probe-q:2026-10-27", ready=True)
    _only_request(httpx_mock, fx)


def view_latest_not_modified(
    client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]
) -> None:
    respond(httpx_mock, fx)
    tag = fx["request"]["headers"]["If-None-Match"]
    result = client.latest("ops_status", "probe-a:2026-10-27", if_none_match=tag)
    assert result == NotModified(etag=tag)
    _only_request(httpx_mock, fx)


def views_list(client: AllSourceClient, httpx_mock: HTTPXMock, fx: Dict[str, Any]) -> None:
    respond(httpx_mock, fx)
    views = client.list_views()
    assert [(v.view, v.generation, v.ready) for v in views] == [
        ("ops_status", 2, True),
        ("skill_catalog", 1, False),
    ]
    _only_request(httpx_mock, fx)


DRIVERS: Dict[str, Driver] = {
    "entity_state": entity_state,
    "entity_state_not_modified": entity_state_not_modified,
    "event_feed": event_feed,
    "event_feed_epoch_changed": event_feed_epoch_changed,
    "events_query_with_entity_version": events_query_with_entity_version,
    "rate_limited_retry_after": rate_limited_retry_after,
    "read_model_commit": read_model_commit,
    "read_model_commit_conflict": read_model_commit_conflict,
    "read_model_page": read_model_page,
    "read_model_unchanged": read_model_unchanged,
    "view_changes": view_changes,
    "view_changes_empty_after_wait": view_changes_empty_after_wait,
    "view_changes_reset": view_changes_reset,
    "view_define": view_define,
    "view_define_invalid": view_define_invalid,
    "view_entities_page_1": view_entities_page_1,
    "view_entities_page_2": view_entities_page_2,
    "view_get": view_get,
    "view_latest": view_latest,
    "view_latest_not_found": view_latest_not_found,
    "view_latest_not_modified": view_latest_not_modified,
    "views_list": views_list,
}


def test_fixtures_exist() -> None:
    assert fixture_names(), "no fixtures found in sdks/contract/fixtures"


def test_every_fixture_has_a_driver() -> None:
    assert set(fixture_names()) == set(DRIVERS)


@pytest.mark.parametrize("name", fixture_names())
def test_fixture(name: str, httpx_mock: HTTPXMock) -> None:
    fixture = load(name)
    assert fixture["name"] == name
    with AllSourceClient(api_key="test-key", base_url=BASE_URL) as client:
        DRIVERS[name](client, httpx_mock, fixture)
