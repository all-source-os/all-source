# Event History implementation review

PR #443 extends the web application, Query Service, Core and the remote MCP listener. The acceptance test starts from a disconnected host and ends when a reviewer opens both source records. The review follows the [approved design](2026-11-26-event-history-design.md).

| Finding | Failure that needs to be ruled out | Verification point |
| --- | --- | --- |
| Archive and WAL startup | An unavailable required store produces an authoritative empty result. | Start Core with persistence configured but unavailable and request the checkout timeline. |
| Timestamp precision | A release near a sub-microsecond boundary changes order or disappears after archive persistence. | Compare exact source time, UTC bounds and order before and after restart. |
| Retried admission | A retry after reservation reads a different 24-hour window. | The operation journal must return the first reserved end time for the same request ID. |
| Missing evidence | A partial empty lookup tells an authorized reviewer the event is absent. | An incomplete source read must return unavailable. |
| Product session expiry | A signed-out or expired reviewer sees a missing-event page. | The product-session endpoint must return 401 and the browser must preserve the relative return path. |
| Rate limits | Traffic from unauthenticated callers prevents a connected workspace from reading. | Verified grants and browser identities have separate counters from unauthenticated traffic. |
| Tracing | A subscriber sees event or subject selectors in a Core request span. | Inspect the span recorded for each history route with HTTP tracing enabled. |

Workspace authority is checked again after the metered source read. The MCP and browser pass projected fields through distinct credential paths. Native host acceptance still requires an actual host response and opened citations. [Host review procedure](2026-11-26-event-history-verification.md)

## Follow-up review

Core tracks archive and WAL readiness, preserves nanosecond precision for new records and reports uncertainty for legacy records. The operation journal retains the end time of the first reservation. Missing evidence from an incomplete read returns unavailable. Expired product sessions return to sign-in. Authenticated reads receive separate rate-limit counters, and Core omits history selectors from request spans.

The web proxy shares a public rate-limit bucket for each unauthenticated OAuth action. At a deployment's rate limit, simultaneous connection attempts can receive HTTP 429 before workspace selection. Authenticated timeline and citation requests have separate counters.
