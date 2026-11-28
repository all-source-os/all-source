# Event History checkout acceptance

The referral test asks an engineering lead to connect one AllSource workspace, ask “What changed in checkout?” and open the release and rollback records. Each record must match its original ingest receipt. [Approved design](../plans/2026-11-26-event-history-design.md) · [Reproduction steps](../plans/2026-11-26-event-history-verification.md)

| Surface | Observed result |
| --- | --- |
| Web routes | Nine Event History browser and MCP route tests passed in the combined Vitest run. The signed `allstack` receipt is `dbac1348-7f1a-4451-8501-a9f1a6ffa881`. |
| Core | A signed gateway request created a synthetic tenant, ingested a checkout release and fetched the same event ID and version. Nine history regression cases and twelve related retained-read and membership cases passed. |
| Query Service and MCP | Twelve Query Service cases passed against an isolated Core or its domain validators. Nine MCP listener and compatibility cases passed. |
| Independent MCP client and browser | The reviewer signed in, selected `review-engineering`, and granted read access. The client requested checkout history over HTTPS. Both returned links opened in a fresh signed-in browser and matched the original event IDs and timestamps. [Two source receipts](evidence/2026-11-26-event-history-checkout.jsonl) |
| ChatGPT | The private plugin discovered the read scope, opened the workspace picker and confirmed the connection after the reviewer selected `review-engineering`. The chat composer showed AllSource Event History selected. A native answer and both opened citations remain outstanding. |
| Claude Desktop | The signed-in Tech Leaders workspace disabled **Add custom connector**. The available account settings offered no connector administration. A native answer and both opened citations remain outstanding. |

## Design and implementation

The self-service flow uses a single workspace-selected read grant. The browser requests a separate signed-in check before showing an event. The isolated test environment adds synthetic sign-in, seed records and an ingress restricted to the review routes. The production feature defaults to off. The [implementation review](../plans/2026-11-26-event-history-code-review.md) records the source findings and remaining public OAuth rate limit.
