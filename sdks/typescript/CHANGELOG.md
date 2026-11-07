# Changelog

All notable changes to `@allsourcedev/client`, the TypeScript SDK.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows [SemVer](https://semver.org).

## [Unreleased]

## [0.25.0] — 2026-11-07

Registered client keys. A tenant client can authenticate to the gateway with a
short-lived ES256 assertion signed by a P-256 key whose public half an operator
registered on the tenant, instead of a long-lived API key. This brings the
TypeScript SDK to parity with the Rust SDK 0.29.0. Design:
`docs/decisions/2026-11-07-tenant-clients-registered-es256-keys.md`;
registration and rotation: `docs/runbooks/CLIENT_KEY_ROTATION.md`.

### Added

- `credentials` option on `AllSourceClient`: a `CredentialProvider`, either a
  function or an object with a `bearer()` method, returning the bearer (sync or
  async). The client asks it on every request attempt, retries included, and
  sends the result as `Authorization: Bearer` only. Pass exactly one of
  `apiKey` or `credentials`.
- `CredentialError`: thrown when a provider throws or returns an empty bearer,
  and when an assertion provider is given an unusable key or identity. It is
  not retried and does not count toward the circuit breaker.
- `Es256AssertionProvider.fromPkcs8Pem(pem, { clientId, tenant, kid, audience, crypto? })`.
  Uses WebCrypto only, with no new dependency. Signs a 60 s
  (`ASSERTION_LIFETIME_SECS`) assertion with `iss` = client id, `sub` = tenant,
  `aud` = audience, the registered `kid` in the header and a fresh `jti`,
  reuses it until 15 s (`ASSERTION_REFRESH_SECS`) before it expires, and shares
  one mint between concurrent callers. It refuses an unusable key or an empty
  field at construction by test-signing once. `mintAt(iat)` mints an uncached
  assertion at a pinned time, for fixtures.
- `AllSourceError.reason`: the refusal's `reason` code (`legacy_key_sunset`,
  `asserted_expired`, `asserted_unregistered`, ...), read from the gateway's
  top-level `reason` or the Query Service's nested `error.reason`.
- `AllSourceError.isLegacyKeySunset()`: true for a `401 legacy_key_sunset`,
  which a deployment returns after its `ALLSOURCE_LEGACY_KEY_SUNSET` for keys
  minted before managed registration (the old onboarding `SignAPIKey` keys).
  Dashboard keys are not affected.
- `engines.node` is `>=18`. The assertion provider needs a global WebCrypto
  (Node 19+, Bun, browsers) or a `crypto` option.

### Changed

- `AllSourceConfig` is now `AllSourceConfigBase` plus exactly one of `apiKey`
  or `credentials`. Code that passes `apiKey` compiles unchanged.
- The constructor error for a missing key is now
  `apiKey or credentials is required`.
