# Legacy registered API-key authority compatibility

## Accepted change

Preserve registered keys whose retained history begins with legacy Core events carrying version `1`. The observed production history contains nine such events followed by ordinal ten carrying version `10`. Repair both gateway readers in the authorized successor to merged PR #363 without rotating existing keys or rewriting retained events.

## Authority contract

Read a fresh, complete `retained-entity-v1` response. Require matching tenant and entity, complete count and total count, `has_more=false`, unique event IDs, valid nondecreasing timestamps, and an entity revision equal to the maximum retained event version, matching Core’s wire contract. Legacy-only histories report revision one; histories with numbered events also require the revision to equal the retained event count. Accept an initial prefix of version `1` events. Once an event carries a version greater than one, require every subsequent version to equal its logical ordinal. Reject gaps, missing events, duplicate IDs, cross-tenant or cross-entity events, timestamp inversions, and a legacy version after the numbered transition. Preserve terminal revocation, expiry, role and scope checks.

## Verification and publication

Codex reproduces the active-key 401 before the repair, applies compatibility tests for both gateways, and exercises positive and negative cases through gateway entry points. Claude independently reviews the source and checks the compatibility evidence. Run scoped gateway suites, lint and formatting, then native Allstack exact-commit verification using the existing frozen Prime policy. Record gateway checks separately from that policy.

Use the authorized `fix/legacy-key-history` branch and one successor PR as one commit. Wait for candidate CI before deploying immutable Control Plane and Query Service images under the existing production authorization. Preserve live topology and configuration. Read back machine image digests and health, then verify the pre-existing application key through its real session endpoint. Retain previous hosted Codex-to-Claude sync and revoked-key evidence with its original deployment provenance; qualify the compatibility deployment separately.

## Acceptance

- Registered legacy key reaches its authorized existing application session without rotation.
- Legacy histories still reject revoked keys and malformed or incomplete authority.
- Both gateway runtime images match the reviewed application source trees.
- Candidate checks execute positive test cases; the existing PR head and remote branch match the verified commit.
- Native delivery completion, Chronis state and source-linked Prime context are read back from their owning stores.
