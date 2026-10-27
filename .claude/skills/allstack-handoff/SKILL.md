---
name: allstack-handoff
description: Continue an active Allstack delivery in Codex or Claude using durable source, current ownership, and an immutable worktree instead of copying a transcript.
---

# Continue in another host

Use handoff only when the user requests another host or continuation requires it. Work in the coordinator checkout. Run `allstack current` to discover the active run; if multiple runs are active, ask which job to continue rather than guessing. A missing active run requires explicit enrollment through the delivery guide; never fabricate state from a conversation summary.

Read current run state and resolve any pending operation through its destination-specific recovery path before transferring. Confirm the requested host exists and is authenticated when execution is required. Preserve already granted authority; ask only for an actually missing authorization against a concrete result.

Use `allstack handoff <id> --epoch <current> --to codex|claude-code`, then `allstack resume <id> --epoch <returned>`. Keep these identifiers inside the tool calls. Read the returned workspace and Chronis claimant before acting. The new worktree, accepted plan, selected methods, and outstanding evidence define continuation; prior hidden reasoning is unnecessary.

If invoking another real host CLI, use existing supervised Allstack plan/build operations with normal authentication and permission modes. Native tools used outside those operations have the host's own authority and are not fenced by the epoch. Do not claim global protection.

For independent native planning, review, or QA, ask the receiving host to read its installed skill and current source. Retain actual calls and command outputs with the target revision. Do not report a fresh skill read or executed check from the receiving host's final text alone.

Report which job moved, the receiving host, preserved work, and the next user-relevant action. Recovery must inspect receipts and actual edits; never replay an uncertain build merely because its final response is missing. Retain ownership and process cleanup until the operation is complete.

## Host capability

Claude Code: invoke as /allstack-<name> or request its name naturally. Use native repository tools and its current permission mode. An unavailable or denied tool leaves its check unverified.
