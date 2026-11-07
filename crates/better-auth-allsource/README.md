# better-auth-allsource

[![Crates.io](https://img.shields.io/crates/v/better-auth-allsource.svg)](https://crates.io/crates/better-auth-allsource)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Event-sourced authentication persistence for [better-auth-rs](https://crates.io/crates/better-auth-core), powered by [Allsource](https://github.com/all-source-os/all-source).

Instead of storing auth state in a traditional database, every create, update, and delete becomes an immutable event in Allsource's event store. You get full audit history, time-travel debugging, and crash-safe durability — all with sub-millisecond read performance.

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
better-auth-allsource = "0.14"
better-auth-core = "0.8"
```

Wire it up:

```rust
use better_auth_allsource::AllsourceAuthAdapter;
use better_auth_core::BetterAuth;

let adapter = AllsourceAuthAdapter::new(
    "http://localhost:3900",  // Allsource Core (event store)
    "http://localhost:3902",  // Allsource Query Service
    "ask_your_api_key",      // API key
);

let auth = BetterAuth::builder()
    .database(adapter)
    .secret("your-secret-key")
    .build()
    .await?;
```

That's it. All 10 `DatabaseAdapter` sub-traits are implemented — users, sessions, accounts, organizations, members, invitations, verification tokens, two-factor auth, API keys, and passkeys all work out of the box.

### With a registered client key

Against the hosted gateway, the adapter can sign every request with a P-256 key that an AllSource operator registered on your tenant, instead of holding a long-lived API key. `AllsourceAuthAdapter::with_credential_provider` asks its provider for the bearer before every request. The adapter does not depend on the `allsource` SDK, so add it with the `es256-assertion` feature and wrap its `Es256AssertionProvider` in a closure:

```toml
[dependencies]
allsource = { version = "0.29", features = ["es256-assertion"] }
better-auth-allsource = "0.14.15"
```

```rust,ignore
use allsource::{CredentialProvider as _, Es256AssertionProvider};
use better_auth_allsource::AllsourceAuthAdapter;

let provider = Es256AssertionProvider::from_pkcs8_pem(
    &std::fs::read("client-key.pem")?,
    "my-auth-service",            // client id: the assertion's iss
    "my-tenant",                  // tenant: the assertion's sub
    "registered-kid",             // RFC 7638 thumbprint returned at registration
    "https://api.all-source.xyz", // the gateway's ALLSOURCE_ASSERTION_AUDIENCE
)?;

let adapter = AllsourceAuthAdapter::with_credential_provider(
    "https://api.all-source.xyz",
    "https://api.all-source.xyz",
    move || provider.bearer().map_err(|e| e.to_string()),
);
```

The provider mints a 60-second ES256 assertion and reuses it until 15 seconds before it expires, so a cached token is never sent stale. Only the gateway verifies assertions: point both URLs at it, not at Core or the Query Service directly. Registration is operator-only; see [`docs/runbooks/CLIENT_KEY_ROTATION.md`](../../docs/runbooks/CLIENT_KEY_ROTATION.md). A failing provider surfaces as `AllsourceAuthError::Api` with status `0`, before any request is sent.

API keys minted by the old onboarding flow (HS256 keys with no registration record) are refused with `401` and reason `legacy_key_sunset` once the gateway's `ALLSOURCE_LEGACY_KEY_SUNSET` passes. Dashboard-created keys and registered client keys keep working.

## How It Works

Each auth entity maps to an Allsource event stream:

| Entity | Entity ID Pattern | Event Types |
|---|---|---|
| User | `auth-user:{id}` | `UserCreated`, `UserUpdated`, `UserDeleted` |
| Session | `auth-session:{token}` | `SessionCreated`, `SessionDeleted` |
| Account | `auth-account:{id}` | `AccountCreated`, `AccountUpdated`, `AccountDeleted` |
| Verification | `auth-verification:{id}` | `VerificationCreated`, `VerificationDeleted` |
| Organization | `auth-org:{id}` | `OrgCreated`, `OrgUpdated`, `OrgDeleted` |
| Member | `auth-member:{id}` | `MemberCreated`, `MemberUpdated`, `MemberDeleted` |
| Invitation | `auth-invitation:{id}` | `InvitationCreated`, `InvitationUpdated`, `InvitationDeleted` |
| Two-Factor | `auth-2fa:{id}` | `TwoFactorCreated`, `TwoFactorUpdated`, `TwoFactorDeleted` |
| API Key | `auth-apikey:{id}` | `ApiKeyCreated`, `ApiKeyUpdated`, `ApiKeyDeleted` |
| Passkey | `auth-passkey:{id}` | `PasskeyCreated`, `PasskeyUpdated`, `PasskeyDeleted` |

**Event pattern:**
- **Create/Update** — appends a full-state snapshot as the event payload
- **Delete** — appends a `{ "_deleted": true }` marker event
- **Read** — fetches the latest event for the entity and deserializes the payload

This means every auth state change is preserved as an immutable event. You can reconstruct any entity's full history by querying its event stream.

## Features

- **Full `DatabaseAdapter` coverage** — all 10 sub-traits (`UserOps`, `SessionOps`, `AccountOps`, `VerificationOps`, `OrganizationOps`, `MemberOps`, `InvitationOps`, `TwoFactorOps`, `ApiKeyOps`, `PasskeyOps`)
- **Event-sourced audit trail** — every auth mutation is an immutable event with timestamp
- **Durable storage** — Allsource Core persists events via WAL + Parquet with CRC32 checksums
- **Fast reads** — latest-state queries hit Allsource's in-memory DashMap (11.9us p99)
- **Soft deletes** — deletion markers preserve history; entities can be restored
- **Field-based search** — find users by email, accounts by provider, sessions by user ID, etc.
- **TLS support** — `rustls` (default) or `native-tls` feature flags

## TLS Configuration

```toml
# Default: rustls
better-auth-allsource = "0.14"

# Or use native-tls
better-auth-allsource = { version = "0.14", default-features = false, features = ["native-tls"] }
```

## Architecture

```
┌─────────────────────────────────────────────┐
│          your application                   │
│                                             │
│  BetterAuth ──► AllsourceAuthAdapter        │
│                    │                        │
│                    ├── writes ──► Core :3900 │
│                    │             (WAL+Parquet)
│                    └── reads ──► QS   :3902 │
│                                  (queries)  │
└─────────────────────────────────────────────┘
```

Writes go directly to Allsource Core (the event store). Reads go through the Query Service, which handles auth validation and query routing. Both are part of the [Allsource](https://github.com/all-source-os/all-source) platform.

## Storing password hashes

**Password hashes belong on `Account.password`, not on `User.metadata`.**

`better-auth-core` models email/password credentials on the `Account` row (`AccountOps::create_account` takes a `password: Option<String>` field). That's the field `better-auth-core`'s own password-verify flow reads, and it round-trips through serde cleanly.

```rust
use better_auth_core::types::{CreateAccount, CreateUser};

let user = db.create_user(CreateUser::new().with_email(&email)).await?;

db.create_account(CreateAccount {
    account_id: user.id.clone(),
    provider_id: "credential".to_string(),
    user_id: user.id.clone(),
    password: Some(password_hash), // <- canonical home for the hash
    access_token: None,
    refresh_token: None,
    id_token: None,
    access_token_expires_at: None,
    refresh_token_expires_at: None,
    scope: None,
}).await?;
```

`User.metadata` is for application-specific data (theme prefs, feature flags, etc.). Since v0.14.10 the adapter:

- **round-trips `User.metadata` losslessly** through the event payload (previously dropped because of upstream `#[serde(skip)]` — see issue #187), so genuine app metadata survives create/update/read
- **rejects** `create_user` / `update_user` whose `metadata` contains a `password`, `password_hash`, `passwordHash`, `passwd`, `hashed_password`, or `hashedPassword` key — these must move to `Account.password`
- emits a `tracing::warn` whenever `metadata` is set, so any side-channel use stays visible in production logs

If you are migrating off a `metadata.password_hash` pattern, copy the existing hash to `Account.password` first, then clear it from `metadata` on the next `update_user` call.

## Error Handling

All methods return `AuthResult<T>` from `better-auth-core`. The adapter maps HTTP, JSON, and API errors into `AuthError::Database(DatabaseError::Query(...))`:

```rust
use better_auth_allsource::AllsourceAuthError;

// The error type exposes three variants:
// AllsourceAuthError::Http(_)  — network/connection errors
// AllsourceAuthError::Json(_)  — serialization errors
// AllsourceAuthError::Api { status, message } — non-2xx responses from Allsource
```

## Requirements

- [Allsource Core](https://github.com/all-source-os/all-source) running (event store)
- [Allsource Query Service](https://github.com/all-source-os/all-source) running (API gateway)
- A valid API key

## License

MIT
