---
name: allsource-customer
description: Prepare AllSource customer work through its verified MCP connector, then hand off to the product for human review and display. Use when the customer asks to prepare a tenant event timeline and show what a replay would affect before i approve anything.
---

# AllSource customer workflow

Read [workflow](references/workflow.md) for product inputs, domain limits and connector binding requirements. Read [human gate contract](references/human-gate.md) before any pending write or handoff.

## Workflow

1. Establish the customer's intended product job and permitted data. Retrieve tenant-authorised event/restart/replay evidence and prepare a read-only investigation view using existing MCP infrastructure.
2. Discover the configured product connector and verify its actual tool schemas. This draft package does not supply a live endpoint or install a connector. When unavailable, prepare an unsubmitted factual summary only and state what is missing.
3. Use authoritative validation/calculation tools and explicit provenance. Submit only a pending proposal, with consent for required host/server processing, revision checks and idempotency. Never treat model output as agreed data.
4. Display the product review resource when supported; otherwise use a verified authenticated product review route where host rules allow it, plus useful text. Source-linked event timeline, query scope, restart proof and replay-plan differences.
5. Hand control to the human. The tenant operator reviews the evidence in the product and explicitly authorises any consequential replay or infrastructure action through its separate human gate. Stop before the gated action; do not click approval/signing controls or call privileged product endpoints on their behalf.
6. Retrieve the authorised versioned status/result only after the product records the decision. Report exact pending/approved/rejected/failed state; never infer completion from a tool call or chat response.

## Limits

No arbitrary queries, cross-tenant payloads, secrets, replay execution, infrastructure change or subscription action from this customer review skill. Existing ingestion SDK functionality is not redefined by this review-surface amendment. The product enforces price, entitlement, identity, human authority and release state. A skill prompt does not enforce security. No arbitrary files, secrets, private portfolio data or unrelated customer records may be read.

## Example trigger

“Prepare a tenant event timeline and show what a replay would affect before I approve anything”
