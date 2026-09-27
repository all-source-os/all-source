# Customer evidence transport — 27 September 2026

Implements the design in `docs/plans/2026-09-27-customer-evidence-bindings.md`.
This is local synthetic transport evidence, not public launch or native Claude
host verification. Existing access/tools tasks remain open.

## Behavior

- Existing MCP profile gains three separately gated tools: prepare a two-run
  comparison, retrieve its current review, and retrieve result status. Pending
  receipts cannot claim approval, execution or delivery.
- Closed input/output schemas reject forged authority, unexpected fields and
  unsupported preparation kinds. Successful JSON text and structured output match.
- Query Service binds these tools to the existing durable, metered application
  services. Product-session `/connections/share` selects exact run pins with
  explicit source consent. Agent credentials cannot call that human endpoint.
- Remote HTTP preflight checks current grant/membership/entitlement without
  consuming or demanding unused query allowance. Domain admission still gates
  every source read. An exact final-unit retry succeeds without a second charge.
- Shared MCP HTTP client reads unpooled HTTP/1 chunks with a 64 KiB response limit,
  30-second request deadline (plus at most one second of cleanup), identity
  encoding, no redirect and no retry. Fixed-length JSON is required before body
  reads: Hackney can otherwise buffer a whole transfer chunk before delivery.
  These bounds cover accepted body bytes, not a claimed process-memory ceiling.
- Default-off switches: Query Service `CUSTOMER_EVIDENCE_ENABLED`, MCP
  `ALLSOURCE_CUSTOMER_EVIDENCE_REVIEW`. Existing review/connection/remote flags
  still apply. Metadata consent is never silently upgraded.

## Proof

`transport.txt`: compiled stdio and remote HTTP release against actual Query
Service HTTP and the frozen Core binary. **10 tests, 0 failures, 2 optional
fixtures skipped; eight executed.** Covers product-only sharing, final-unit
prepare/read retry, SIGKILL recovery, changed intent, source/grant revocation,
wrong tenant, metadata consent, flags, body limits, request-log privacy, exact
pending result semantics, old metadata profile and remote connection lifecycle.
The skipped cases are native Claude Code and opt-in browser consent fixture.

`query-full.txt`: **6 doctests, 1137 tests, 0 failures, 2 skipped, 151 excluded**.
The integration cases above run separately; the default suite excludes them.

`mcp-full.txt`: **654 tests, 0 failures, 5 excluded**, completed in 181 seconds.
`source.sha256` records 23 source/document hashes; `artifacts.sha256` records the
local MCP release archive and frozen Core executable. All source hashes verify.
Scoped formatting, warnings-as-errors compile,
strict Credo and Dialyzer passed for both apps. QS retains its pre-existing seven
filtered Dialyzer findings and one unused filter; MCP has zero findings.

### Bugs found during verification

1. Initial async client left error-response headers in the stdio GenServer mailbox,
   causing a restart after denied access. A focused mailbox assertion reproduced
   it. Cleanup now awaits connection death and drains that sender; unchanged
   assertion passes and compiled denied/reconnect transport passes.
2. Initial evidence schema rejected nullable recorded outcomes. Actual compiled
   review retrieval exposed the mismatch; the domain's existing nullable field
   is now modeled explicitly and the same transport test passes.
3. Source inspection showed Hackney transfer-chunk buffering. Client now refuses
   transfer encoding/missing or oversized content lengths before starting body
   reads; fixtures exercise chunked/compressed/large/malformed/error responses.
4. First complete MCP suite hit its 180-second command deadline after 564 passing
   tests. `mcp-full-timeout.txt` is incomplete, not a passing full-suite result.
   The subsequent 360-second bounded trace run completed in 181 seconds with all
   654 tests passing. Existing pipeline stop cases account for the delay; no
   running build was killed or restarted merely because an observation timed out.

## Limits and remaining work

Product source selection/review UI, remote evidence-consent UI, edits, deletion
controls, consequential human decision authority, generic event/restart/replay
preparation, billing-period reset adoption and actual native-host proof remain
unfinished. MCP cancellation notifications still do not cancel synchronous tool
work; service and HTTP deadlines bound it. No render resource or verified human
review URL is returned, and `read_result` cannot deliver an approved outcome.

`host-evaluation.xml` contains ten planned read-only model questions, not an
executed host evaluation. No customer content was sent to Anthropic, credentials
issued in production, customer flags enabled or deployment performed. Held bets
remain held. Prime design registration was attempted but its current replica is
read-only; no competing writer was stopped.
