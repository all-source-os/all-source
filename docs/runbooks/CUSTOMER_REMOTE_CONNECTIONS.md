# Customer remote connection configuration

Status: implemented and locally tested; **disabled pending host and rollout gates**.
See [evidence](../evidence/2026-09-27-customer-remote-http/README.md).
This config reuses Query Service, Core and the existing Elixir MCP app. It does
not create another database, general MCP surface or standalone auth service.

## Required settings when rollout is approved

Use one canonical HTTPS web origin without a trailing slash. Example values
below are public configuration, not credentials. Query Service and web must
agree on issuer. Query Service resource must exactly match the public MCP URL.

| Runtime | Settings |
| --- | --- |
| Query Service | `CUSTOMER_REVIEW_ENABLED=true`, `CUSTOMER_CONNECTIONS_ENABLED=true`, `CUSTOMER_REMOTE_ENABLED=true`; `CUSTOMER_OAUTH_ISSUER=https://www.all-source.xyz`; `CUSTOMER_REVIEW_RESOURCE=https://www.all-source.xyz/mcp/customer-review`; existing strong `JWT_SECRET` and existing Core authorization access |
| Web | `CUSTOMER_CONNECTIONS_ENABLED=true`, `CUSTOMER_REMOTE_ENABLED=true`; same `CUSTOMER_OAUTH_ISSUER`; canonical `NEXT_PUBLIC_APP_URL`; existing `QUERY_SERVICE_URL`; `CUSTOMER_REVIEW_HTTP_URL` set to the HTTPS origin serving the existing customer MCP profile |
| Existing MCP app | `ALLSOURCE_CUSTOMER_REVIEW=true`, `ALLSOURCE_CUSTOMER_REVIEW_HTTP=true`; `CUSTOMER_REVIEW_QUERY_URL` set to the Query Service HTTPS origin; `CUSTOMER_REVIEW_METADATA_URL=https://www.all-source.xyz/.well-known/oauth-protected-resource`; explicit HTTP listen port/IP |

MCP listens on `127.0.0.1:3904` by default. A container deployment requires an
explicit appropriate listen IP and private/TLS service routing. Both web and MCP
optionally accept comma-separated `CUSTOMER_REVIEW_ORIGINS`; default empty
rejects requests with any Origin header. Requests without Origin are allowed
subject to bearer authorization. Keep these allowlists identical; add only
verified client origins. Neither HTTP adapter accepts arbitrary upstream URLs
from a request. Cleartext upstream HTTP is accepted only for literal loopback
test hosts. Do not set Core admin credentials on the customer MCP process.

The HTTP profile is exclusive and does not start stdio or the general Core
backend. Local Claude Code continues to use the existing owner-checked private
connection file and stdio profile without the HTTP flag.

## Customer setup

Once enabled, Agent connections shows the public MCP URL and setup instructions.
In Claude's custom connector settings use the MCP URL, client ID `claude-ai` and
an empty client secret. Automatic client registration is not supported. The
customer signs in at the product, sees explicit fields, and grants one-hour
eligibility/proposal-syntax access. There is no refresh token; reconnect after
expiry. Active and pending grants can be revoked in Agent connections, including
after a subscription expires. OAuth consent is never approval of a review.

## Order and rollback

Deploy Core conditional-write support first. Upgrade **every** Query Service
reader to the activation-aware grant version before enabling remote issuance.
The existing team-ownership migration and old Control Plane writer drain remain
prerequisites; do not enable merely because these endpoints exist. Deploy and
verify the existing MCP HTTP profile, then web. Keep flags false until actual
host, ownership, entitlement, retention and latency gates pass. No Fly app or
public route was activated by this change.

To stop issuance, disable web and Query Service remote flags. To invalidate
existing remote use as well, disable the Query Service remote flag and customer
MCP HTTP process; local stdio grants remain governed by their separate setting.
Revoke specific grants through the existing human product UI when needed. Do
not roll back to readers that accept pending remote credentials as active.
Rotating the existing signing secret invalidates pending requests, codes and
remote envelopes, but affects other sessions; use the established rotation
procedure rather than treating it as a routine connector toggle.

## Operational limits

Discovery/token work has an eight-second Query Service bound and a nine-second
web upstream deadline; uncertain token activation requires reconnect. Codes
expire after five minutes, request cookies after ten, access grants after one
hour. Pending grants consume the existing live/rolling-day issuance limits.
Request admission/tenant limits are the existing per-process ETS buckets.
Capture status, route and timing only; never body, Authorization, Cookie,
authorization query, verifier or callback code. Verify proxy/CDN access-log
redaction before public rollout. Hosting-provider traces and real browser
callback behavior have not yet been proven by the local fixtures.
