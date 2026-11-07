# Changelog

All notable changes to the `allsource-go` SDK (`github.com/all-source-os/allsource-go`).

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows [SemVer](https://semver.org). Releases are tagged `sdk-go-v<VERSION>`.

## [Unreleased]

## [0.24.0] - 2026-11-07

Registered client keys. A tenant client can authenticate to the gateway with a
short-lived ES256 assertion signed by a P-256 key whose public half an
AllSource operator has registered on the tenant, instead of a long-lived API
key. Design: `docs/decisions/2026-11-07-tenant-clients-registered-es256-keys.md`;
registration and rotation: `docs/runbooks/CLIENT_KEY_ROTATION.md`.

### Added

- `CredentialProvider` interface (`Bearer(ctx) (string, error)`) and the
  `CredentialProviderFunc` adapter. The client asks the provider on every
  request attempt, retries included.
- `WithCredentialProvider` option. With it, the client sends only
  `Authorization: Bearer <token>` and no `X-API-Key`; the API key passed to
  `New` is ignored and may be empty.
- `CredentialError` and `ErrNoCredential`. A provider error, or an empty
  token, fails the request before anything is sent, and is not retried.
- `ES256AssertionProvider` and `NewES256AssertionProvider(pem, clientID,
  tenant, kid, audience)`. Mints a 60-second assertion (`iss` client id,
  `sub` tenant, `aud`, `iat`, `exp`, `jti`; header `alg` ES256, `typ` JWT,
  `kid`) and reuses it until 15 seconds before it expires. The key must be a
  PKCS#8 PEM P-256 private key; a bad key or an empty argument is refused at
  construction. Standard library only.
- `JWKThumbprint`: the RFC 7638 thumbprint of a P-256 public key, which is
  the `kid` the gateway expects.
- `DefaultAssertionAudience` (`https://api.all-source.xyz`),
  `AssertionLifetime`, `AssertionRefresh`.
- `APIError.Reason`: the gateway's `reason` field from an auth denial body
  `{"error","reason","message"}`. Previously dropped.
- `APIError.IsLegacyKeySunset()`: a 401 with reason `legacy_key_sunset`. After
  the gateway's `ALLSOURCE_LEGACY_KEY_SUNSET`, a managed onboarding key with no
  registration record gets this; dashboard keys keep working.
- `APIError.IsAssertionDenied()`: reason starts with `asserted_`
  (`asserted_signature`, `asserted_audience`, `asserted_skew`,
  `asserted_expired`, `asserted_unregistered`, `asserted_revoked`,
  `asserted_unavailable`).

### Unchanged

- A client built without a provider sends the static key in both
  `Authorization: Bearer` and `X-API-Key`, as before.
