# Restricted evidence transport

Bind the existing durable comparison workflow to the existing Query Service and
exclusive Elixir MCP profile. No additional service, database, credential type,
or execution authority is introduced.

## Contract

An additional default-off evidence flag exposes preparation, review retrieval,
and result retrieval. Preparation accepts exactly two product-selected run pins,
revision zero and an immutable timestamp/UUID retry key. Retrieval requires the
review ID, version and a separate immutable request key. The domain service owns
source authority, current consent, metering, durability and expiry. Context must
not claim preparation for metadata-only consent. Every receipt remains pending,
unapproved and unexecuted; no approval tool is exposed.

The remote transport authenticates its envelope with a private session preflight
that checks current membership, entitlement, grant and read-context scope without
requiring unused quota. Actual evidence operations use canonical admission. This
allows an exact final-unit retry without admitting a new unpaid query. Existing
context and syntax-validation eligibility checks retain their quota policy.

Source selection stays in the product-session boundary with explicit per-source
consent; an agent bearer cannot select its own sources. Adding a transport endpoint
does not establish a complete product selection or human decision interface.

Both MCP clients share bounded HTTP response handling: no redirect or retry,
identity encoding, fixed-length JSON, on-demand chunks, a 64 KiB body cap and a
total deadline. Reject transfer encoding before body reads because Hackney can
buffer an entire transfer chunk before yielding it.
Input and output schemas are checked before a successful tool result. Tool text
and structured content match. Errors never echo upstream bodies or claim that an
uncertain preparation could not have persisted. Retrying preserves the original
key and exact input.

## Verification and rollout

Exercise real Core, Query Service HTTP, compiled stdio and remote HTTP with
synthetic selected runs. Cover final-unit retry, changed keys/intent, foreign
owners, revoked sources/grants, consent, disabled flags, strict arguments,
pending result semantics, response bounds and fixed errors. Native Claude host
evaluation and production activation remain separate unverified gates.

Billing-period reset adoption, product source selection/review, consequential
human authorization, edits and other evidence kinds remain existing tracked
work. Keep customer flags off until those rollout requirements pass. Held bets
stay held. This work does not authorize remote builds, registry pushes or deploys.

Protocol references: [MCP tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)
and [transports](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports).
