"""Loads the shared wire fixtures in sdks/contract/fixtures."""

import json
from pathlib import Path
from typing import Any, Dict, List

import httpx
from pytest_httpx import HTTPXMock

FIXTURES_DIR = Path(__file__).resolve().parents[2] / "contract" / "fixtures"


def fixture_names() -> List[str]:
    return sorted(path.stem for path in FIXTURES_DIR.glob("*.json"))


def load(name: str) -> Dict[str, Any]:
    data: Dict[str, Any] = json.loads((FIXTURES_DIR / f"{name}.json").read_text())
    return data


def respond(httpx_mock: HTTPXMock, fixture: Dict[str, Any]) -> None:
    """Register the fixture's response for the next request."""
    response = fixture["response"]
    if response["body"] is None:
        httpx_mock.add_response(
            status_code=response["status"], headers=response["headers"], content=b""
        )
    else:
        httpx_mock.add_response(
            status_code=response["status"], headers=response["headers"], json=response["body"]
        )


def assert_request(request: httpx.Request, fixture: Dict[str, Any]) -> None:
    """The SDK sent exactly the fixture's method, path, query, headers and body."""
    expected = fixture["request"]
    assert request.method == expected["method"]
    assert request.url.raw_path.split(b"?")[0].decode() == expected["path"]
    assert dict(request.url.params) == expected["query"]
    for name, value in expected["headers"].items():
        assert request.headers.get(name) == value
    if "If-None-Match" not in expected["headers"]:
        assert "If-None-Match" not in request.headers
    if expected["body"] is None:
        assert request.content == b""
    else:
        assert json.loads(request.content) == expected["body"]
