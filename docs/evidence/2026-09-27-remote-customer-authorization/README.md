# Remote customer authorization service

27 September 2026. Task `t-7ce02d` remains open. This verifies an internal PKCE
service and actual Core persistence, not a public OAuth endpoint, browser consent
journey, remote MCP transport, native Claude call or production release.

## Change

The existing consent registry can now hold pending hosted-Claude grants. Such a
grant cannot authenticate until an independent Core receipt is created with an
absent-only conditional write. Duplicate redemption revokes the associated
credential using the existing independent revocation marker. An old registry
snapshot cannot remove either receipt. Unconditional adapter writes do not accept
the activation namespace. The legacy generic Core administrator remains trusted.

The internal authorization service checks the pre-registered `claude-ai` client,
exact hosted callback, configured HTTPS resource, authorization-code response,
single review scope and canonical S256 challenge. After current eligibility and
explicit consent, it encrypts a five-minute code with purpose-separated
Plug.Crypto keys derived from the existing server secret. The durable grant
contains only its credential hash and consent. Original acceptance time is
preserved. Pending authorizations count toward existing issuance limits.

Exchange checks the verifier, client, redirect, resource and expiry, rechecks
live membership/billing, activates once, then uses the existing live access gate.
The internal result is a credential/binding for the future remote token envelope;
no controller exposes it. A crash after activation requires a fresh connection.
An expired code fails before activation; replay revocation applies to a still-valid
code with its correct verifier. Grant lifetime is one hour; refresh is not added.

Settings render pending receipts as “Awaiting connection” and allow revocation
before redemption. The local creation form still issues only Claude Code grants.
No feature flag or production setting changed.

## Observed verification

| Check | Result |
|---|---|
| Initial PKCE/domain and actual-Core tests | 9 passed |
| Full Query Service regression | 6 doctests, 1,055 tests, 0 failures, 2 skipped, 106 integration exclusions |
| Final focused suite including compiled local MCP → HTTP → Core | 38 tests, 0 failures, 2 intentional skips; 36 executed |
| Warnings-as-errors compile and formatting | Passed |
| Strict Credo | 73 checks, no issues after simplifying request validation |
| Dialyzer | Passed with existing ignore file unchanged; 7 existing warnings filtered, 1 unnecessary filter reported |
| Connection UI tests | 3 passed, including pending-receipt revocation |
| Web TypeScript and targeted Biome | Passed |

The final focused suite includes the six new actual-Core cases, four pure PKCE
tests and two code-envelope tests, plus existing connection/grant/compiled-MCP
regressions. It adds the final malformed-activation/admin-boundary case after the
full suite; no production code changed after the full suite. The skips are the
optional native Claude model test and manual browser fixture. Neither was enabled.

New real-Core cases prove pending denial, code exchange across SIGKILL/restart,
activation and subsequent replay revocation across further restarts, resistance
to restoring the old registry, at-most-one successful concurrent exchange,
wrong-client/resource/redirect/verifier denial, generic JWT/code tampering denial,
missing encryption secret, revoked pending consent, removed membership, canceled
billing, existing connection limits, admin-only activation records and malformed
receipt denial. Envelope tests also cover secret rotation, purpose separation,
expired and oversized codes. RFC 7636's known S256 vector supplies an independent
crypto expectation; redirects include case/port/encoding/query/fragment attacks.

Final focused command, from `apps/query-service` with the existing local Core and
MCP release binaries supplied through their documented environment variables:

```text
env -u ALLSOURCE_CLAUDE_BINARY -u ALLSOURCE_CLAUDE_TRACE MIX_ENV=test \
  ALLSOURCE_CORE_BINARY=<absolute-Core-binary> \
  ALLSOURCE_CUSTOMER_MCP_BINARY=<absolute-MCP-release-binary> \
  gtimeout -k 15 300 mix test --include integration \
  test/query_service_ex/integration/customer_remote_authorization_test.exs \
  test/query_service_ex/integration/customer_connections_test.exs \
  test/query_service_ex/integration/customer_agent_grant_core_test.exs \
  test/query_service_ex/integration/customer_agent_http_test.exs \
  test/query_service_ex/domain/customer_agent/remote_authorization_test.exs \
  test/query_service_ex/infrastructure/adapters/customer_authorization_code_test.exs \
  test/query_service_ex/infrastructure/adapters/customer_agent_grant_store_test.exs
```

## Remaining gates

Discovery metadata, bounded form token endpoint, encrypted access-token binding,
cookie/CSRF-protected browser consent and the existing MCP server's HTTP transport
remain unwired. Public client pre-registration is the selected first path; no
DCR/CIMD capability is claimed. Native host execution remains subject to the
earlier unanswered external-processing approval. No model request, charge,
production grant, deployment or pilot activation occurred.

Every Query Service reader must enforce activation before remote issuance is
enabled. Pending receipts consume a live slot until their one-hour grant expires
or the owner revokes it, although their code expires after five minutes. Immutable
history/tombstone retention, legacy ownership migration, source authority,
proposal persistence/display and human action approval remain open. Sequential
authority reads are not an atomic authorization-plus-disclosure transaction.

Prime design indexing was attempted but rejected because the connected store is
a read-only replica; the versioned design remains in git. Concurrent partnership,
analytics, Prime-package and outreach edits were preserved.

See [design](../../plans/2026-09-27-customer-remote-oauth-design.md) and the source
and artifact hashes alongside this report. Earlier signed CI fixes are separately
recorded under the local connection evidence; they do not establish this change's
remote host or deployment readiness.
