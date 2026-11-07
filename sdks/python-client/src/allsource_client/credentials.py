"""Per-request credentials.

A static API key is sent unchanged on every request. A credential provider is
asked for a bearer on every request attempt, retries included, so a
short-lived token never outlives the request it was minted for.
"""

from __future__ import annotations

import inspect
from typing import AsyncGenerator, Awaitable, Callable, Generator, Optional, Union

import httpx

CredentialProvider = Callable[[], str]
"""Returns the bearer token for one request attempt, without the `Bearer ` prefix."""

AsyncCredentialProvider = Callable[[], Union[str, Awaitable[str]]]
"""Like `CredentialProvider`; the async client also awaits a coroutine result."""


def _checked(token: object) -> str:
    if not isinstance(token, str) or not token:
        raise ValueError("credential provider must return a non-empty bearer string")
    return token


def require_one_credential(
    api_key: Optional[str],
    credential_provider: Optional[object],
) -> None:
    """Enforce that exactly one of `api_key` and `credential_provider` is given."""
    if api_key is not None and credential_provider is not None:
        raise ValueError("pass api_key or credential_provider, not both")
    if api_key is None and credential_provider is None:
        raise ValueError("api_key or credential_provider is required")
    if credential_provider is not None and not callable(credential_provider):
        raise TypeError("credential_provider must be callable and return a bearer string")


class ProviderAuth(httpx.Auth):
    """Sets `Authorization: Bearer <token>` from the provider on every attempt."""

    def __init__(self, provider: AsyncCredentialProvider) -> None:
        self._provider = provider

    def auth_flow(self, request: httpx.Request) -> Generator[httpx.Request, httpx.Response, None]:
        token = self._provider()
        if inspect.isawaitable(token):
            close = getattr(token, "close", None)
            if callable(close):
                close()
            raise TypeError("an async credential provider needs AllSourceAsyncClient")
        request.headers["Authorization"] = f"Bearer {_checked(token)}"
        yield request

    async def async_auth_flow(
        self, request: httpx.Request
    ) -> AsyncGenerator[httpx.Request, httpx.Response]:
        token = self._provider()
        if inspect.isawaitable(token):
            token = await token
        request.headers["Authorization"] = f"Bearer {_checked(token)}"
        yield request
