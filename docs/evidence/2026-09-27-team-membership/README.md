# Verified-email invitations and current team authority

27 September 2026. Progress on `t-7ce02d`, based on signed ownership commit
`58b7f5d2ba219b80d03b941253a462d32ab692ee`. Access-task acceptance criteria remain
unchecked. This is not a released MCP connection or human approval gate.

## Result

Control Plane team administration now uses current Core membership, not a
platform role in a JWT. Members and invitation receipts share one conditional
Core config revision at `team:<tenant>:members`. Retries re-read both authority
and membership, so an administrator removed during a write cannot restore an
older list. Removing or demoting the final administrator is refused. API keys
and view-as sessions cannot administer membership.

Each invitation has a random 192-bit code, a seven-day lifetime, a specific email
and an admin/member role. Core stores only its SHA-256 identifier, receipt and
location pointer. Acceptance requires provider-verified email and current
inviter administration. Membership admission and receipt consumption commit
together. Concurrent subjects cannot both consume the same receipt. Replays
can resume workspace selection only for the same subject while still a member;
they cannot restore a removed member. Existing members retain their current
role. A fresh explicit invitation can restore membership.

A separate selection pointer makes subsequent sign-ins return to the joined
workspace. The pointer itself grants no authority: login checks current stored
membership. Selection failure returns no session, even if admission committed.
Normal OAuth stamps verified email into its session; GitHub reads verified
addresses from its email endpoint instead of trusting public profile text.
Google reads the v2 `verified_email` boolean. The auth service's `emailVerified`
boolean is preserved; missing or false proof cannot accept an invitation.

The website offers code creation and a signed-in **Join workspace** form. Codes
travel in request bodies; no mail delivery is claimed. Current role controls,
member counts and errors reflect Control Plane responses. The team proxy uses
the browser cookie, rejects caller-supplied Authorization and cross-origin
mutations, allowlists routes, bounds request/response reads and deadlines, and
does not forward unrelated cookies or redirects. Joining sets an HttpOnly
replacement cookie and strips its JWT from browser JSON. Subject, platform role
and session expiry are retained; the full page reload clears prior SWR caches.

## Evidence

- Full Control Plane `go test -race ./...` with the real Core binary: passed.
  Tests include concurrent acceptance, stale administrator writes, restart
  recovery, interrupted selection, removal before retry, verified-email provider
  fixtures, actual auth middleware, body limits, final-admin retention and
  tenant-member count compatibility. `golangci-lint run`: zero issues.
- Full web suite: **182 tests passed in 40 files**. Final focused team suite:
  **17 tests passed**, including a malformed null session payload. Type check and production build passed. Targeted Biome
  check passed with one existing `noExplicitAny` warning in the unrelated
  API-key listing method; no rule was disabled.
- Query Service/Core/compiled MCP regression: **14 tests, zero failures,
  one optional native Claude skip** (13 executed). Actual Core accepts the v2
  team envelope; membership removal still denies a live grant. Strict Credo
  passed; Dialyzer passed using the existing seven filters and reported one
  unnecessary skip. No ignore file was changed.
- Real in-app browser at `http://127.0.0.1:4344/dashboard/team`: synthetic owner
  signed in through the website login proxy, created an invitation, saw its
  expiry/code and accurate no-email message, and dismissed the dialog with
  Escape. Synthetic invitee signed in, entered the code and joined via the
  actual Next proxy and Control Plane handler against real Core. Two members
  appeared, the invitee's admin controls disappeared, and reload retained that
  team. The invitation dialog was visually inspected at the default viewport.
  Final copy pluralization and input-size adjustments followed this journey;
  focused tests and the production build cover those final changes.

The opt-in `TestTeamBrowserFixture` supplies only synthetic identity exchange
and Query Service shell responses; membership handlers, signed-session
middleware, website routes and Core persistence are real. This does not verify
live Google/GitHub login, production billing display or native Claude. The
fixture, Core process and web preview were stopped; no production credentials,
private source records, email or external model call were used.

## Rollout and remaining limits

1. Upgrade Core to the mandatory conditional-config/initialized-tenant API.
2. Upgrade Query Service to read both member arrays and v2 envelopes.
3. Drain old Control Plane team writers, then deploy all new Control Plane
   instances and the web application together. Do not mix legacy unconditional
   list writes with v2 edits. Rollback must retain v2-compatible readers/writers;
   blindly reverting to old Control Plane can lose receipts or team changes.
4. Reissue old `team:invite:<token>` invitations. There is no unsafe v1 fallback.
   Migrate legacy ownership explicitly; do not infer an owner from an email slug.
5. Verify the production topology's leader, durability and rollout behavior
   before customer use. No production deployment occurred in this change.

The 1,000-member, 128-receipt and encoded-record bounds are technical admission
limits, not purchased seats. Expired receipts are pruned on subsequent creation;
lookup records and immutable history are not erased. Grant/retention policy
still needs completion. Unused invitations have no revoke/list UI yet.

Team removal is enforced by this team path and the restricted customer-review
eligibility path. Existing generic data sessions/API keys have separate
revocation behavior; this change does not claim immediate removal from every
legacy data endpoint. Revoke issued API keys separately. Team join is not proof
of browser-only authority for a consequential action. Existing OAuth callback
JWT query transport also remains a separate hardening item. Connection issue/
revoke UI, explicit durable host/field consent, transport ownership or PKCE,
source authority, pending proposals, human receipts and native host proof remain
unfinished. Trial MCP entitlement is not invented or changed here.

Provider references: [GitHub email API](https://docs.github.com/en/enterprise-cloud%40latest/rest/users/emails)
and [Google OAuth v2 userinfo](https://developers.google.com/resources/api-libraries/documentation/oauth2/v2/python/latest/oauth2_v2.userinfo.html).

See [reproduction commands and logs](verification.md), [source hashes](source-sha256.txt)
and [artifact hashes](artifact-sha256.txt).
