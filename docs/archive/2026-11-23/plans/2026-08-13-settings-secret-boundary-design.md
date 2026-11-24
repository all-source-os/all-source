# Settings secret boundary

## Problem

The authenticated Settings profile renders a long-lived API key and repeats it in a
configuration sample. The session endpoint also returns that key to browser JavaScript,
and the persisted auth store writes it to `localStorage`. The session JWT carries the
same credential, so the WebSocket token bridge can expose it even after the Settings UI
is changed.

## Decision

- Keep Settings for identity, security, notifications, and workspace policy.
- Keep API key creation, rotation, revocation, and connection guidance on API Keys.
- Reveal a raw API key only once, immediately after explicit creation or rotation.
- Never include a long-lived API key in a browser session response, browser storage, or
  human session JWT.
- Keep workspace identity visible only where needed for developer setup; it is an
  identifier, not an authentication credential.

## Data flow

1. Sign-in creates a human session JWT containing identity, tenant, role, and expiry.
2. `/api/auth/session` returns only user and tenant data.
3. Zustand persists only user, tenant, and authenticated state. A storage migration
   drops the legacy `coreApiKey` field from existing browsers.
4. Users create a dedicated scoped credential from API Keys.
5. Raw credential appears once in the creation dialog, then only its prefix and metadata
   remain available for management.

## Acceptance

- Settings contains no tenant credential, raw key, or copyable key configuration.
- Session response excludes `core_api_key`, even when an old JWT contains that claim.
- Auth persistence excludes and migrates away `coreApiKey`.
- New human session JWTs contain no long-lived API key.
- API Keys explains one-time reveal and offers Chronis connection guidance without a
  stored secret.
