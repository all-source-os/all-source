# Changelog

All notable changes to the `allsource-client` Python SDK.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows [SemVer](https://semver.org).

## [Unreleased]

## [0.25.0] - 2026-11-07

Registered client keys. A tenant client can authenticate to the hosted gateway
with a short-lived ES256 assertion signed by a P-256 key whose public half an
operator registered on the tenant, instead of a long-lived API key. Design:
`docs/decisions/2026-11-07-tenant-clients-registered-es256-keys.md`;
registration and rotation: `docs/runbooks/CLIENT_KEY_ROTATION.md`.

### Added

- `credential_provider` keyword argument on `AllSourceClient` and
  `AllSourceAsyncClient`: a callable returning the bearer token. The client
  calls it on every request attempt, retries included, and sends the result as
  `Authorization: Bearer <token>`. The async client also awaits a coroutine
  result. Exported type aliases: `CredentialProvider`, `AsyncCredentialProvider`.
- `Es256AssertionProvider` behind the new `assertion` extra
  (`pip install 'allsource-client[assertion]'`, which adds `cryptography`).
  `from_pkcs8_pem(pem, client_id, tenant, kid=None, audience="https://api.all-source.xyz")`
  signs a 60 s assertion with `iss` = client id, `sub` = tenant, `aud` =
  audience, the registered `kid` in the header and a fresh `jti`, and reuses it
  until 15 s before it expires. `kid` defaults to the key's RFC 7638
  thumbprint. The provider is safe to share between threads. A key that is not
  P-256, or an empty client id, tenant, kid or audience, is refused at
  construction.
- `jwk_thumbprint(public_key)`: the RFC 7638 thumbprint the gateway uses as `kid`.
- `AllSourceError.reason`: the gateway's refusal reason when the error body
  carries one (for example `asserted_unregistered` or `asserted_expired`).
- `LegacyKeySunsetError` and `LEGACY_KEY_SUNSET`. Raised on
  `401 legacy_key_sunset`, which the gateway returns after
  `ALLSOURCE_LEGACY_KEY_SUNSET` for a managed key minted by the old onboarding
  flow that has no registration record. Dashboard keys are not affected.

### Changed

- `api_key` is now optional. Pass exactly one of `api_key` and
  `credential_provider`; passing both or neither raises `ValueError`. Existing
  calls that pass `api_key` behave as before.
