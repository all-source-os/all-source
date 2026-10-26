# Managed API key lifecycle verification

## Deployment status

Both gateways are deployed from reviewed candidate `f1ede26fedc7d3bc1d12c60938e9e317c25393fa` on [PR #363](https://github.com/all-source-os/all-source/pull/363). All candidate CI passed before rollout. Actual Fly machine image references matched the immutable build digests, and both public readiness endpoints returned HTTP 200. Fresh hosted Codex writer and independent Claude reader tests passed. Both the original revoked credential and the fresh proof credential now receive HTTP 401 through Control Plane and Query Service.

The [accepted design](../plans/2026-10-26-api-key-lifecycle-repair.md) rejects unregistered long-lived API JWTs. Human sessions and short service delegation retain their separate authority. Native `ask_` keys keep their existing Core-backed validation boundary; this report concerns gateway-managed JWTs.

## Implemented contract

Control Plane and Query Service require current Core lifecycle authority before accepting a managed JWT. The reader checks the complete retained `apikey:{tenant}` stream, including archived events, with explicit protocol, tenant, entity and version integrity. Missing registration, revoked state, unavailable authority and incomplete history deny authentication. A positive identity cache cannot override revocation.

All seven scopes offered by the dashboard are recognized: `events:read`, `events:write`, `queries:execute`, `projections:read`, `projections:write`, `schemas:read` and `schemas:write`. Existing schema and projection operations require their matching scope; read-only roles cannot gain mutation access from metadata. Control Plane schema governance retains its existing `ManageSchemas` role requirement, which excludes service accounts. Query Service tenant schema registration retains its existing write-capable service-account access, now additionally requiring `schemas:write`. Scope metadata does not erase either route-specific boundary. Signed roles constrain the current metadata scopes. Managed keys cannot reach administrative or key-management operations. Control Plane's separate admin middleware rejects programmatic JWTs. Query Service's Bearer, X-API-Key and JWKS fallback paths share the same managed-key contract. View-as sessions remain read-only. Public ingestion rejects lifecycle namespace forgery at the ingestion owner, preserving Control Plane's view-as refusal and audit order.

Finite expiration is encoded in both the JWT and authoritative metadata. The dashboard's existing “Never expires” selection produces an explicitly marked managed JWT without `exp`, backed by an active record with null expiration. That shape requires registration on every request. Explicit null, zero or malformed JWT expiration is rejected. Human sessions and delegation still require finite expiration. Existing finite JWTs retain their own deadline when legacy metadata lacks expiration; reissue an old year-limited credential to obtain a true non-expiring key.

Create and revoke return success only after Core acknowledges a lifecycle event. Appends use the leader client without automatic retries. Identical registration retries are idempotent; conflicting recreation cannot reactivate a revoked identity. Revocation invalidates the dashboard projection, including tombstones for legacy audit-only identities. New onboarding credentials register a distinct key identity before returning a secret.

## Source and runtime evidence

The owned Core fixture was rebuilt from base `54c26ea308f38b2adf6b68e3609d6f23b8c2e8af`, Core tree `3e9f478ae30bcef382dbabc7104948e8f664f1de`, with `community`, `default`, `multi-tenant` and `server` features. Cargo reported a non-fresh executable artifact. Its SHA-256 is `7e91c879f1c4d4888e508a70caaa1c1d49c6f826822c4b8b8604198c4f3e58a7`. No Core source modification or production Core deployment is required for this repair.

The real Control Plane boundary exercises registration, application event ingestion/readback, reserved-stream refusal, durable revocation and same-token rejection after Core restart. The real Query Service HTTP boundary exercises requested-expiry minting, application ingestion/readback, scope refusal, revocation, restart rejection and an active non-expiring reader after restart. Both fixtures own their loopback ports and private Core data directories, and kill/reap their Core processes.

Additional regressions cover unknown, malformed and expired credentials; partial authority; tenant mismatch; duplicate registration; conflicting reactivation; role/scope restrictions; bounded delegation clock skew; human/view-as compatibility; admin rejection; JWKS fallback; and create/revoke persistence failures. Test-only HTTP fixtures preserve the updated connection returned by body reads so connection reuse cannot masquerade as an authentication failure.

Codex desktop implemented the repair. Authenticated Claude reviewed the source and independently executed the compiled Go boundary tests after reading the native `allstack-qa` skill. The final Claude session was `16dbeb27-c560-4c6e-9c60-c0104b7f43b4`; its execution had no permission denials. The source-only optional-expiry review (`5441424f-7974-4762-addc-bb7e1c74fbd0`) reported no P1/P2 finding. Source review and runtime execution are separate evidence.

| Check | Observed result | Native receipt |
|---|---|---|
| Control Plane complete suite | 1,108 passing test/subtest cases; 20 existing opt-in skips | `d8ac90e8-2905-4da5-b13e-17b5c6fa3ea1` |
| Control Plane lint | zero issues | `52ea524a-72df-446d-bc97-c711b55c0f12` |
| Query Service complete default suite | 1,154 tests and six doctests; zero failures, two skipped, 174 excluded | `a52f5be7-cb01-4bdd-9427-243b224c8788` |
| Query Service selected lifecycle/auth suite, including real Core | 42 tests; zero failures | `99c044c2-4ce7-4dee-a5f4-17bd06aed266` |
| Query Service warnings-as-errors compile | exit zero | `3aa34bc8-1cf5-46ab-a44c-703a05288be0` |
| Query Service formatting and strict Credo | formatting passed; zero lint issues | `e341d29c-9ac1-4fe3-8a1c-6a87225914ed`, `06c5ca64-059c-4974-8ef8-9f5c76582759` |
| Query Service type analysis | passed with nine existing ignored findings; no new suppression | `873ec47a-e10b-43b7-9902-b14611908141` |
| X-API-Key and lifecycle tests after unreachable-branch cleanup | 20 tests; zero failures | `7696b618-5889-468b-81c9-d79daffac046` |
| Independent Claude native QA | six tests ran and passed; zero skips/failures, including real Core revoke/restart | `fa0206fb-04f5-4461-8ea3-54732dcb4af6` |
| Advertised-scope compatibility | Control Plane role/operation regression passed under race detector; full suite passed | `99fa162e-88ac-492e-b37e-3250c1644134`, `9ab65e9f-33d3-4f8a-b2f5-0605eb358def` |
| Query Service with all dashboard scopes | 42 lifecycle/auth tests and the full 1,154-test/six-doctest suite passed | `99c044c2-4ce7-4dee-a5f4-17bd06aed266`, `8be10913-3172-4417-b4f9-073eafe80078` |
| Scope compatibility lint/type checks | Go lint, strict Credo and Dialyzer passed | `91cf2eff-ca60-46a3-8279-42873faef719`, `89efbadc-235d-4c00-928d-350cbbc84398`, `8ef744f4-b1c3-47a3-b741-6b3ca8693165` |

[Candidate lifecycle CI](https://github.com/all-source-os/all-source/actions/runs/40384260100) executed the actual Core revoke/restart test in Control Plane and 13 Query Service lifecycle tests with zero failures. [Candidate quality CI](https://github.com/all-source-os/all-source/actions/runs/40384259974) also passed. The frozen native Prime policy executed 142 passing tests plus formatting, Clippy and diff checks; it remains distinct from Go/Elixir verification and hosted acceptance.

Claude checked both executable digests before executing. Its test artifact SHA-256 was `73b5fc64c898d9a683a07497f338ac74e2e16bf8c2da7c5e8c9012eadfe21d5b`, unchanged across execution. The fixture build receipt is `22a1e5d7-f5e1-4cec-926b-56b0ab26a7e5`. Its six cases cover real Core revocation/restart, delegation clock bounds, readonly role restrictions, registered non-expiring keys, null/zero expiration rejection and non-expiring token/record consistency. Claude did not execute the Elixir suite; Codex executed that separate check.

The existing tenant egress workflow now includes both Control Plane and Query Service lifecycle proofs against its built Core server. The original frozen Allstack policy remains the Prime-only policy; these Go/Elixir checks supplement it. Exact candidate CI outcomes belong to the PR checks, and local checks do not establish hosted revocation. The original ignored hosted tests remain separate from middleware checks.

## Hosted authority and routing

Before repair deployment, the actual hosted strict-history read returned HTTP 200 with exactly two lifecycle events: `apikey.created`, then `apikey.revoked`, versions 1 and 2. Its integrity protocol was `retained-entity-v1`, count and total count were 2, and `has_more` was false. The state owner already held the revoke. Fresh-key validation after deployment read three lifecycle events and confirmed active registration, exact scopes and an expiration equal to the JWT deadline.

Actual Fly configuration places Control Plane and Query Service in `iad`, both using `http://allsource-core.internal:3900`. Control Plane has no configured regional Core URL overrides. The running Core machine has an empty services list; application configuration has no public HTTP service. Its reserved shared IPv4 allocation does not expose a Core data endpoint through Fly Proxy. The tested public ingress remains Control Plane.

## Hosted qualification

| Target | Deployed image digest | Deployment receipt |
|---|---|---|
| `allsource-control-plane` | `sha256:dddf102d6165538bad18383fdbd528491102f1e56cae694efa03bdd04174ae88` | `58dd55c7-177b-46ab-ab0d-a07086c97fda` |
| `allsource-query` | `sha256:cc559053e12c480c0cc807a05c9f79fa472ba278c38c9cb6c004433421bdfb64` | `a52916d6-5d0c-445e-9868-39c34c731c7a` |

The deployed application Git trees are `fde2b0d3f893e667e8ba1faed667ff8aada51582` for Control Plane and `63cc871620b20ae7f38054bd6b31d178ab074eec` for Query Service. Existing live configuration and machine sizing were preserved. Core was not deployed or restarted.

Human dashboard access remained usable. The temporary key had exactly `events:read` and `events:write`; its JWT and authoritative record both expired at `2026-11-25T19:34:58Z`, with a lifetime of 2,592,000 seconds. Authenticated Codex session `01a179b6-c14b-75e3-9407-9f91902fde48` executed the hosted writer test: one passed, zero failures/skips, receipt `1de82527-1428-4cc0-8b30-5130e894c740`. Authenticated Claude session `ce257513-895c-4de2-8ca6-191090e7288d` read the native QA skill and executed independent hosted readback: one passed, zero failures/skips, receipt `2c19612b-b30a-42ba-8c05-f324b8202b30`.

The [hosted sync report](2026-10-26-hosted-prime-sync-verification.md) records exact delivered entities, event IDs and cursor movement. Hosted Memory displayed both new nodes. After dashboard revocation, Control Plane rejected the old key (`5b8f75a5-f68d-49cc-8502-bd164f04a613`) and new key (`9e16baee-5b2e-4b61-994e-c3ec1e377257`) with HTTP 401. Query Service rejected both keys through Bearer and X-API-Key paths, four HTTP 401 observations (`a8479e46-3a95-483e-bdb2-e82131955563`). Both private credential files were removed and the clipboard cleared. Synthetic decisions remain in the audit trail.

![Temporary proof key revoked in the dashboard](images/2026-10-26-hosted-key-revoked.png)

## Limits

The current strict authority reader accepts at most 1,000 lifecycle events per tenant. Larger, malformed or incomplete histories fail closed; check this bound for other tenants before expanding rollout. The active proof-key validation observed three events. The local restart proof covers owned Core processes, not a hosted Core restart or a second physical computer. Hosted non-expiring-key creation, another tenant and vector retrieval were not exercised. Finite-key hosted sync and revocation passed through the actual deployed boundaries; native source-linked memory and Chronis completion record publication separately.

Claude scope review (`3b4004e5-c7d7-4e7b-b2a5-c2a4829d3d1d`, receipt `9b001cb5-9aae-432b-b3ab-f69265d05566`) identified the missing Control Plane `PUT /api/v1/read-models/:name` scope mapping; it now requires `projections:write`, with a readonly-denial regression. Its cross-plane schema-policy observation is an explicit compatibility constraint: Control Plane governance retains `ManageSchemas`; Query Service tenant schema registration retains its write-level service-account boundary. Tests assert both outcomes.
