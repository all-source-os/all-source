"""Credential providers: asked on every attempt, exactly one credential, typed refusals."""

from typing import List

import pytest
from pytest_httpx import HTTPXMock

from allsource_client import (
    LEGACY_KEY_SUNSET,
    AllSourceAsyncClient,
    AllSourceClient,
    AllSourceError,
    LegacyKeySunsetError,
    SyncCheckpoint,
)
from tests.contract import load, respond

BASE_URL = "http://localhost:3902"

SUNSET_BODY = {
    "error": "unauthorized",
    "reason": "legacy_key_sunset",
    "message": "This API key predates registered keys and stopped working on 2026-12-01. "
    "Register a client key or create a dashboard key.",
}


class CountingProvider:
    def __init__(self) -> None:
        self.calls = 0

    def __call__(self) -> str:
        self.calls += 1
        return f"token-{self.calls}"


class TestConstruction:
    def test_api_key_and_provider_together_are_refused(self) -> None:
        with pytest.raises(ValueError, match="not both"):
            AllSourceClient(api_key="key", credential_provider=lambda: "token")
        with pytest.raises(ValueError, match="not both"):
            AllSourceAsyncClient(api_key="key", credential_provider=lambda: "token")

    def test_neither_credential_is_refused(self) -> None:
        with pytest.raises(ValueError, match="required"):
            AllSourceClient(base_url=BASE_URL)
        with pytest.raises(ValueError, match="required"):
            AllSourceAsyncClient(base_url=BASE_URL)

    def test_a_provider_that_is_not_callable_is_refused(self) -> None:
        with pytest.raises(TypeError):
            AllSourceClient(credential_provider="token")  # type: ignore[arg-type]

    def test_the_api_key_stays_positional(self, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(url=f"{BASE_URL}/health", json={"status": "ok"})

        with AllSourceClient("positional-key", BASE_URL) as client:
            client.health()

        request = httpx_mock.get_request()
        assert request is not None
        assert request.headers["Authorization"] == "Bearer positional-key"
        assert "X-API-Key" not in request.headers


class TestSyncProvider:
    def test_the_provider_is_asked_on_every_attempt_including_retries(
        self, httpx_mock: HTTPXMock
    ) -> None:
        provider = CountingProvider()
        client = AllSourceClient(base_url=BASE_URL, credential_provider=provider)
        httpx_mock.add_response(status_code=503, headers={"Retry-After": "1"}, content=b"")
        respond(httpx_mock, load("view_changes"))
        sleeps: List[float] = []
        client._sleep = sleeps.append

        items = list(client.sync("ops_status", checkpoint=61, wait=0, follow=False))

        assert sleeps == [1.0]
        assert items[-1] == SyncCheckpoint(revision=62, generation=2)
        requests = httpx_mock.get_requests()
        assert [r.headers["Authorization"] for r in requests] == [
            "Bearer token-1",
            "Bearer token-2",
        ]
        assert all("X-API-Key" not in r.headers for r in requests)
        assert provider.calls == 2
        client.close()

    def test_the_prime_path_also_asks_the_provider(self, httpx_mock: HTTPXMock) -> None:
        provider = CountingProvider()
        httpx_mock.add_response(
            url=f"{BASE_URL}/api/v1/prime/projections", json={"data": [], "count": 0}
        )

        with AllSourceClient(base_url=BASE_URL, credential_provider=provider) as client:
            client.list_prime_projections()

        request = httpx_mock.get_request()
        assert request is not None
        assert request.headers["Authorization"] == "Bearer token-1"

    def test_an_empty_bearer_is_refused(self) -> None:
        with AllSourceClient(base_url=BASE_URL, credential_provider=lambda: "") as client:
            with pytest.raises(ValueError, match="non-empty"):
                client.health()

    def test_an_async_provider_needs_the_async_client(self) -> None:
        async def provider() -> str:
            return "token"

        with AllSourceClient(base_url=BASE_URL, credential_provider=provider) as client:  # type: ignore[arg-type]
            with pytest.raises(TypeError, match="AllSourceAsyncClient"):
                client.health()


class TestAsyncProvider:
    async def test_an_async_provider_is_awaited_on_every_attempt(
        self, httpx_mock: HTTPXMock
    ) -> None:
        calls: List[int] = []

        async def provider() -> str:
            calls.append(1)
            return f"async-token-{len(calls)}"

        client = AllSourceAsyncClient(base_url=BASE_URL, credential_provider=provider)
        httpx_mock.add_response(status_code=429, headers={"Retry-After": "1"}, content=b"")
        respond(httpx_mock, load("view_changes"))
        sleeps: List[float] = []

        async def record_sleep(delay: float) -> None:
            sleeps.append(delay)

        client._sleep = record_sleep

        items = [
            item async for item in client.sync("ops_status", checkpoint=61, wait=0, follow=False)
        ]

        assert sleeps == [1.0]
        assert items[-1] == SyncCheckpoint(revision=62, generation=2)
        requests = httpx_mock.get_requests()
        assert [r.headers["Authorization"] for r in requests] == [
            "Bearer async-token-1",
            "Bearer async-token-2",
        ]
        assert all("X-API-Key" not in r.headers for r in requests)
        await client.close()

    async def test_a_plain_callable_works_with_the_async_client(
        self, httpx_mock: HTTPXMock
    ) -> None:
        provider = CountingProvider()
        httpx_mock.add_response(url=f"{BASE_URL}/health", json={"status": "ok"})

        async with AllSourceAsyncClient(base_url=BASE_URL, credential_provider=provider) as client:
            await client.health()

        request = httpx_mock.get_request()
        assert request is not None
        assert request.headers["Authorization"] == "Bearer token-1"


class TestLegacyKeySunset:
    def test_sync_client_raises_the_typed_refusal(self, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(status_code=401, json=SUNSET_BODY)

        with AllSourceClient(api_key="old-onboarding-key", base_url=BASE_URL) as client:
            with pytest.raises(LegacyKeySunsetError) as exc_info:
                client.list_views()

        error = exc_info.value
        assert isinstance(error, AllSourceError)
        assert error.reason == LEGACY_KEY_SUNSET
        assert error.code == "unauthorized"
        assert error.status_code == 401
        assert "Register a client key" in str(error)

    async def test_async_client_raises_the_typed_refusal(self, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(status_code=401, json=SUNSET_BODY)

        async with AllSourceAsyncClient(api_key="old-onboarding-key", base_url=BASE_URL) as client:
            with pytest.raises(LegacyKeySunsetError) as exc_info:
                await client.list_views()

        assert exc_info.value.reason == LEGACY_KEY_SUNSET

    def test_the_query_service_nested_refusal_is_typed(self, httpx_mock: HTTPXMock) -> None:
        nested = {"error": {"code": "unauthorized", "reason": "legacy_key_sunset", "message": "x"}}
        httpx_mock.add_response(status_code=401, json=nested)

        with AllSourceClient(api_key="old-onboarding-key", base_url=BASE_URL) as client:
            with pytest.raises(LegacyKeySunsetError) as exc_info:
                client.list_views()

        assert exc_info.value.reason == LEGACY_KEY_SUNSET
        assert exc_info.value.code == "unauthorized"

    def test_other_assertion_denials_keep_their_reason(self, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(
            status_code=401,
            json={
                "error": "unauthorized",
                "reason": "asserted_unregistered",
                "message": "client key is not registered for this tenant",
            },
        )

        with AllSourceClient(credential_provider=lambda: "a.b.c", base_url=BASE_URL) as client:
            with pytest.raises(AllSourceError) as exc_info:
                client.list_views()

        assert not isinstance(exc_info.value, LegacyKeySunsetError)
        assert exc_info.value.reason == "asserted_unregistered"

    def test_a_body_without_a_reason_has_none(self, httpx_mock: HTTPXMock) -> None:
        httpx_mock.add_response(status_code=401, json={"error": "unauthorized"})

        with AllSourceClient(api_key="key", base_url=BASE_URL) as client:
            with pytest.raises(AllSourceError) as exc_info:
                client.list_views()

        assert exc_info.value.reason is None
