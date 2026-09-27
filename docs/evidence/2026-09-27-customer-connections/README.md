# Customer connection consent and management

27 September 2026. Progress on `t-7ce02d`; the task and customer release remain open.

## Behavior verified

- Query Service requires a signed, verified-email product session for connection
  settings. It derives actor/workspace/resource, rejects agent/API/demo/view-as
  credentials, and checks current stored membership. New issuance also checks
  current billing before and after persistence. Billing expiry does not prevent
  the owner from listing or revoking.
- Explicit `review-metadata-v1` consent and an opaque credential hash commit in
  one conditional Core tenant registry. Selected host, operations, disclosure
  fields and acceptance time persist; raw secrets do not. Missing/stale consent
  and pre-consent v1 credentials fail closed. The local form cannot issue remote
  Claude credentials; that path requires its future PKCE flow.
- Core revisions enforce 16 live connections and 64 issuances per rolling day
  across concurrent callers. Revocation cannot refill the day allowance. The
  same limits survive an actual Core SIGKILL/restart. These are technical abuse
  bounds, not plan prices or seats.
- Revocation markers remain independent of the registry. Stale tenant metadata
  and registry replacement cannot restore access. Core failures deny use.
- The website provides explicit consent, a one-time masked credential, private
  connection receipts and revocation. Its dedicated proxy requires a browser
  cookie and exact origin, rejects supplied Authorization/query values, bounds
  streams/deadlines, forbids redirects and sets no-store. Generic proxies cannot
  bypass the dedicated route. Management JSON is capped at 4 KiB before parsing.

## Browser observation

Built Next UI at `http://127.0.0.1:4344` used the opt-in fixture on port 4345.
Identity-provider exchange and dashboard identity responses were synthetic.
Next cookie handling, connection proxy, Query Service JWT checks, membership,
billing and Core persistence were real. No external model or production data
was involved.

The browser logged out the previous test session, signed in as
`connections@example.test`, and opened `/dashboard/settings/connections`.
Create remained disabled until explicit consent. Creation displayed a masked
one-time credential and an active receipt. Hiding the credential and reloading
retained only the receipt. Revocation displayed a persisted revoked state and
removed its revoke action. Screenshot inspection confirmed readable 16px body
text, clear headings and no exposed plaintext secret. This record describes the
observed session; no screenshot file is included.

Fixture completed successfully and cleaned its isolated Core storage/process;
Next preview stopped and its temporary browser tab closed.

## Limits and rollout

**Not deployed or enabled in production.** `CUSTOMER_CONNECTIONS_ENABLED`
defaults off in Query Service and web. Web availability is evaluated at request
time. Existing restricted tool flags also remain opt-in. No production grant,
private source disclosure, native Claude invocation or charge occurred.

Core's conditional-write endpoint must deploy first. New Query Service rejects
unconsented v1 credentials; reconnect is required. Do not roll back to a reader
that accepts v1 as consented. Preserve the prior team rollout requirement: v2
readers first, then drain old Control Plane writers before upgrading them.

The selected client name is not host attestation. Local OS-owner binding,
connector configuration, remote PKCE/exact redirects, authoritative sources,
proposal persistence/display, separate human action approval, production
consistency and actual-host outcomes remain required. Sequential membership and
billing reads are not atomic with issuance or later disclosure. Tenant-wide
technical caps do not provide administrator management of other users' grants.

Current-view pruning is not erasure: Core's immutable history and independent
tombstones still need a retention/deletion policy. Tool admission remains per
process; distributed source-query metering and end-to-end streaming allocation
caps are not proven. No commercial MCP entitlement was invented for trials.

See [verification](verification.md), [source hashes](source-sha256.txt), and
[artifact hashes](artifact-sha256.txt). Earlier evidence remains historical;
this registry supersedes its unconsented v1 grant layout.
