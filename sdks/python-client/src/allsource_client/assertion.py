"""ES256 client assertions for a gateway that holds this client's registered public key.

Needs the optional `cryptography` dependency: ``pip install 'allsource-client[assertion]'``.
"""

from __future__ import annotations

import base64
import hashlib
import json
import threading
import time
import uuid
from typing import Any, Callable, Dict, Optional, Tuple, Union

try:
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.asymmetric import ec
    from cryptography.hazmat.primitives.asymmetric.utils import decode_dss_signature
except ImportError as exc:  # pragma: no cover - exercised only without the extra
    raise ImportError(
        "Es256AssertionProvider needs the 'cryptography' package: "
        "pip install 'allsource-client[assertion]'"
    ) from exc

ASSERTION_LIFETIME_SECS = 60
"""Lifetime of each assertion. The gateway refuses anything longer than 60s."""

ASSERTION_REFRESH_SECS = 15
"""A cached assertion is replaced this long before it expires."""

DEFAULT_AUDIENCE = "https://api.all-source.xyz"

_P256_COORDINATE_BYTES = 32


def _b64url(data: bytes) -> str:
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode("ascii")


def _compact_json(value: Dict[str, Any]) -> bytes:
    return json.dumps(value, separators=(",", ":")).encode("utf-8")


def jwk_thumbprint(public_key: ec.EllipticCurvePublicKey) -> str:
    """RFC 7638 thumbprint of a P-256 public key: the `kid` the gateway registers."""
    if not isinstance(public_key.curve, ec.SECP256R1):
        raise ValueError("client signing key must be on the P-256 curve")
    numbers = public_key.public_numbers()
    x = _b64url(numbers.x.to_bytes(_P256_COORDINATE_BYTES, "big"))
    y = _b64url(numbers.y.to_bytes(_P256_COORDINATE_BYTES, "big"))
    canonical = f'{{"crv":"P-256","kty":"EC","x":"{x}","y":"{y}"}}'
    return _b64url(hashlib.sha256(canonical.encode("ascii")).digest())


class Es256AssertionProvider:
    """Mints a 60-second ES256 assertion and reuses it until 15 seconds before it expires.

    `client_id` becomes `iss`, `tenant` becomes `sub`, `kid` names the
    registered key and defaults to the RFC 7638 thumbprint of the key's public
    half, and `audience` must equal the gateway's `ALLSOURCE_ASSERTION_AUDIENCE`.
    Safe to share between threads. Pass it as `credential_provider`.
    """

    def __init__(
        self,
        private_key: ec.EllipticCurvePrivateKey,
        client_id: str,
        tenant: str,
        kid: Optional[str] = None,
        audience: str = DEFAULT_AUDIENCE,
        *,
        clock: Callable[[], float] = time.time,
    ) -> None:
        if not isinstance(private_key, ec.EllipticCurvePrivateKey) or not isinstance(
            private_key.curve, ec.SECP256R1
        ):
            raise ValueError("client signing key must be a P-256 private key")
        resolved_kid = jwk_thumbprint(private_key.public_key()) if kid is None else kid
        if not client_id or not tenant or not resolved_kid or not audience:
            raise ValueError("client_id, tenant, kid and audience are all required")
        self._key = private_key
        self._client_id = client_id
        self._tenant = tenant
        self._kid = resolved_kid
        self._audience = audience
        self._clock = clock
        self._lock = threading.Lock()
        self._cached: Optional[Tuple[str, int]] = None
        self._mint(self._now())

    @classmethod
    def from_pkcs8_pem(
        cls,
        pem: Union[bytes, str],
        client_id: str,
        tenant: str,
        kid: Optional[str] = None,
        audience: str = DEFAULT_AUDIENCE,
        *,
        clock: Callable[[], float] = time.time,
    ) -> Es256AssertionProvider:
        """Build a provider from a PKCS#8 PEM P-256 private key.

        A key that cannot sign ES256 is refused here rather than on the first request.
        """
        data = pem.encode("ascii") if isinstance(pem, str) else pem
        try:
            key = serialization.load_pem_private_key(data, password=None)
        except (ValueError, TypeError) as exc:
            raise ValueError(f"client signing key is not a P-256 PKCS#8 PEM: {exc}") from exc
        if not isinstance(key, ec.EllipticCurvePrivateKey) or not isinstance(
            key.curve, ec.SECP256R1
        ):
            raise ValueError("client signing key is not a P-256 PKCS#8 PEM")
        return cls(key, client_id, tenant, kid, audience, clock=clock)

    @property
    def kid(self) -> str:
        return self._kid

    def __repr__(self) -> str:
        return (
            f"Es256AssertionProvider(client_id={self._client_id!r}, tenant={self._tenant!r}, "
            f"kid={self._kid!r}, audience={self._audience!r})"
        )

    def _now(self) -> int:
        return int(self._clock())

    def _mint(self, iat: int) -> Tuple[str, int]:
        exp = iat + ASSERTION_LIFETIME_SECS
        header = {"typ": "JWT", "alg": "ES256", "kid": self._kid}
        claims = {
            "iss": self._client_id,
            "sub": self._tenant,
            "aud": self._audience,
            "iat": iat,
            "exp": exp,
            "jti": str(uuid.uuid4()),
        }
        signing_input = f"{_b64url(_compact_json(header))}.{_b64url(_compact_json(claims))}"
        der = self._key.sign(signing_input.encode("ascii"), ec.ECDSA(hashes.SHA256()))
        r, s = decode_dss_signature(der)
        signature = r.to_bytes(_P256_COORDINATE_BYTES, "big") + s.to_bytes(
            _P256_COORDINATE_BYTES, "big"
        )
        return f"{signing_input}.{_b64url(signature)}", exp

    def mint_at(self, iat: int) -> str:
        """An uncached assertion issued at `iat` (Unix seconds), for fixtures that pin the clock."""
        return self._mint(iat)[0]

    def bearer(self) -> str:
        """The current assertion, minting a new one within 15 seconds of expiry."""
        now = self._now()
        with self._lock:
            if self._cached is not None:
                token, exp = self._cached
                if now + ASSERTION_REFRESH_SECS < exp:
                    return token
            token, exp = self._mint(now)
            self._cached = (token, exp)
            return token

    def __call__(self) -> str:
        return self.bearer()
