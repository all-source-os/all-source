"""The ES256 assertion provider mints what the gateway verifies."""

import base64
import json
import threading
from pathlib import Path
from typing import Any, Dict, List

import pytest
from pytest_httpx import HTTPXMock

pytest.importorskip("cryptography")

from cryptography.exceptions import InvalidSignature  # noqa: E402
from cryptography.hazmat.primitives import hashes, serialization  # noqa: E402
from cryptography.hazmat.primitives.asymmetric import ec  # noqa: E402
from cryptography.hazmat.primitives.asymmetric.utils import encode_dss_signature  # noqa: E402

from allsource_client import AllSourceClient, Es256AssertionProvider, jwk_thumbprint  # noqa: E402
from allsource_client.assertion import (  # noqa: E402
    ASSERTION_LIFETIME_SECS,
    ASSERTION_REFRESH_SECS,
)

AUDIENCE = "https://api.all-source.xyz"
BASE_URL = "http://localhost:3902"

# The Rust SDK (sdks/rust/tests/es256_assertion.rs) and the gateway's
# asserted_keys_interop_test.go verify the same two fixtures with the same
# client, tenant, audience and issue time.
FIXTURES = Path(__file__).resolve().parents[2] / "rust" / "tests" / "fixtures"
INTEROP_TEST_KEY = (FIXTURES / "es256_interop_test_key.pem").read_bytes()
INTEROP_TOKEN = (FIXTURES / "es256_interop_token.jwt").read_text().strip()
INTEROP_CLIENT = "longhand-control-plane"
INTEROP_TENANT = "asserted-tenant"
INTEROP_IAT = 1_794_009_600


def _b64url_decode(part: str) -> bytes:
    return base64.urlsafe_b64decode(part + "=" * (-len(part) % 4))


def segment(token: str, index: int) -> Dict[str, Any]:
    data: Dict[str, Any] = json.loads(_b64url_decode(token.split(".")[index]))
    return data


def verify(token: str, public_key: ec.EllipticCurvePublicKey) -> None:
    """Raises `InvalidSignature` unless `token` is a JWS ES256 signature by `public_key`."""
    header, claims, signature = token.split(".")
    raw = _b64url_decode(signature)
    assert len(raw) == 64, "JWS ES256 signatures are raw r||s, not DER"
    der = encode_dss_signature(int.from_bytes(raw[:32], "big"), int.from_bytes(raw[32:], "big"))
    public_key.verify(der, f"{header}.{claims}".encode("ascii"), ec.ECDSA(hashes.SHA256()))


def interop_public_key() -> ec.EllipticCurvePublicKey:
    key = serialization.load_pem_private_key(INTEROP_TEST_KEY, password=None)
    assert isinstance(key, ec.EllipticCurvePrivateKey)
    return key.public_key()


def generated_pem() -> bytes:
    key = ec.generate_private_key(ec.SECP256R1())
    return key.private_bytes(
        serialization.Encoding.PEM,
        serialization.PrivateFormat.PKCS8,
        serialization.NoEncryption(),
    )


def public_of(pem: bytes) -> ec.EllipticCurvePublicKey:
    key = serialization.load_pem_private_key(pem, password=None)
    assert isinstance(key, ec.EllipticCurvePrivateKey)
    return key.public_key()


class TestInterop:
    def test_the_committed_token_is_signed_by_the_fixture_key(self) -> None:
        public = interop_public_key()

        verify(INTEROP_TOKEN, public)
        assert segment(INTEROP_TOKEN, 0)["kid"] == jwk_thumbprint(public)

    def test_the_provider_mints_the_committed_header_and_claims(self) -> None:
        provider = Es256AssertionProvider.from_pkcs8_pem(
            INTEROP_TEST_KEY, INTEROP_CLIENT, INTEROP_TENANT, audience=AUDIENCE
        )
        minted = provider.mint_at(INTEROP_IAT)

        assert segment(minted, 0) == segment(INTEROP_TOKEN, 0)
        claims = [segment(INTEROP_TOKEN, 1), segment(minted, 1)]
        for claim_set in claims:
            jti = claim_set.pop("jti")
            assert isinstance(jti, str) and jti
        assert claims[0] == claims[1]
        verify(minted, interop_public_key())

    def test_an_explicit_kid_is_sent_unchanged(self) -> None:
        public = interop_public_key()
        provider = Es256AssertionProvider.from_pkcs8_pem(
            INTEROP_TEST_KEY.decode("ascii"), INTEROP_CLIENT, INTEROP_TENANT, jwk_thumbprint(public)
        )

        assert provider.kid == segment(INTEROP_TOKEN, 0)["kid"]


class TestAssertion:
    def test_the_assertion_carries_the_registered_identity_and_a_60s_lifetime(self) -> None:
        pem = generated_pem()
        provider = Es256AssertionProvider.from_pkcs8_pem(
            pem, "longhand-control-plane", "onboard-tenant", "kid-1", AUDIENCE
        )

        token = provider.bearer()

        assert segment(token, 0) == {"typ": "JWT", "alg": "ES256", "kid": "kid-1"}
        verify(token, public_of(pem))
        claims = segment(token, 1)
        assert claims["iss"] == "longhand-control-plane"
        assert claims["sub"] == "onboard-tenant"
        assert claims["aud"] == AUDIENCE, "aud is a single string"
        assert claims["exp"] - claims["iat"] == ASSERTION_LIFETIME_SECS == 60
        assert isinstance(claims["jti"], str) and claims["jti"]
        assert "role" not in claims and "scope" not in claims

    def test_the_kid_defaults_to_the_rfc7638_thumbprint(self) -> None:
        pem = generated_pem()
        provider = Es256AssertionProvider.from_pkcs8_pem(pem, "client", "tenant")

        assert provider.kid == jwk_thumbprint(public_of(pem))
        assert segment(provider(), 0)["kid"] == provider.kid

    def test_a_cached_assertion_is_reused_until_15s_before_expiry(self) -> None:
        now = [1_000.0]
        provider = Es256AssertionProvider.from_pkcs8_pem(
            generated_pem(), "client", "tenant", clock=lambda: now[0]
        )

        first = provider.bearer()
        now[0] = 1_000 + ASSERTION_LIFETIME_SECS - ASSERTION_REFRESH_SECS - 1
        assert provider.bearer() == first
        now[0] = 1_000 + ASSERTION_LIFETIME_SECS - ASSERTION_REFRESH_SECS
        second = provider.bearer()

        assert second != first
        assert segment(second, 1)["iat"] == 1_045
        assert segment(second, 1)["jti"] != segment(first, 1)["jti"]

    def test_concurrent_callers_share_one_assertion(self) -> None:
        provider = Es256AssertionProvider.from_pkcs8_pem(generated_pem(), "client", "tenant")
        tokens: List[str] = []
        lock = threading.Lock()

        def call() -> None:
            token = provider.bearer()
            with lock:
                tokens.append(token)

        threads = [threading.Thread(target=call) for _ in range(16)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()

        assert len(tokens) == 16
        assert len(set(tokens)) == 1

    def test_a_signature_from_another_key_does_not_verify(self) -> None:
        token = Es256AssertionProvider.from_pkcs8_pem(generated_pem(), "c", "t").bearer()

        with pytest.raises(InvalidSignature):
            verify(token, public_of(generated_pem()))


class TestRefusedUpFront:
    def test_a_key_that_is_not_a_pkcs8_ec_pem_is_refused(self) -> None:
        with pytest.raises(ValueError, match="P-256"):
            Es256AssertionProvider.from_pkcs8_pem(
                b"-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n",
                "client",
                "tenant",
            )

    def test_a_key_on_another_curve_is_refused(self) -> None:
        pem = ec.generate_private_key(ec.SECP384R1()).private_bytes(
            serialization.Encoding.PEM,
            serialization.PrivateFormat.PKCS8,
            serialization.NoEncryption(),
        )

        with pytest.raises(ValueError, match="P-256"):
            Es256AssertionProvider.from_pkcs8_pem(pem, "client", "tenant", "kid")

    @pytest.mark.parametrize(
        "client_id, tenant, kid, audience",
        [("", "t", "k", AUDIENCE), ("c", "", "k", AUDIENCE), ("c", "t", "", AUDIENCE), ("c", "t", "k", "")],
    )
    def test_missing_identity_fields_are_refused(
        self, client_id: str, tenant: str, kid: str, audience: str
    ) -> None:
        with pytest.raises(ValueError, match="required"):
            Es256AssertionProvider.from_pkcs8_pem(generated_pem(), client_id, tenant, kid, audience)


class TestClientWiring:
    def test_the_client_sends_only_a_verifiable_assertion(self, httpx_mock: HTTPXMock) -> None:
        pem = generated_pem()
        provider = Es256AssertionProvider.from_pkcs8_pem(pem, "longhand-control-plane", "tenant-a")
        httpx_mock.add_response(url=f"{BASE_URL}/health", json={"status": "ok"})

        with AllSourceClient(base_url=BASE_URL, credential_provider=provider) as client:
            client.health()

        request = httpx_mock.get_request()
        assert request is not None
        assert "X-API-Key" not in request.headers
        scheme, token = request.headers["Authorization"].split(" ", 1)
        assert scheme == "Bearer"
        verify(token, public_of(pem))
        assert segment(token, 1)["sub"] == "tenant-a"
