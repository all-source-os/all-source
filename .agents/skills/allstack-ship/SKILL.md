---
name: allstack-ship
description: Verify and publish an authorized application change through Allstack's existing exact-commit delivery and destination reconciliation, reusing its single PR.
---

# Ship the accepted change

Read repository policy and existing branch/PR state first. Work on the authorized live feature branch and reuse its one PR. A ship request does not authorize merge, deployment, a new branch, or unrelated changes. Carry earlier explicit commit/publication authority forward; request only a missing permission after preparing a concrete reviewable result.

## Establish the candidate

Use `allstack current` behind the interface. For an active delivery, inspect its accepted plan, current worktree, pending actions, ownership, and policy. Resolve uncertainty by reading the actual destination before retrying. For completed native work without a managed run, use `allstack adopt` with its indexed request, accepted plan, and explicit policy to enroll the existing candidate without rerunning model planning or implementation. Stage intended new files explicitly before adoption. Authority flags require the user's actual authorization.

Review the candidate's behavior and applicable risk with `allstack-review`, then exercise the real job with `allstack-qa`. These are judgment and runtime checks, not substitutes for repository gates. Keep the accepted scope, source-linked decisions, and verification requirements intact.

For native adoption, inspect the coordinator checkout that holds the candidate and its exact commit. An epoch worktree may still hold the earlier base; do not use its HEAD as evidence of the native candidate. Keep the coordinator at its original path until publication and store readback finish, then archive its path-bound receipts before cleanup. Preserve pre-existing Chronis tasks and required project Prime context.

Existing guarded configuration files can remain in the tracked base: snapshots bind their Git identities without copying their contents into Allstack artifacts. A changed, added, or deleted guarded file blocks enrollment. Preserve the user's configuration; resolve the actual change rather than removing unrelated tracked files to satisfy adoption.

## Verify and publish

Use the returned run identity and epoch internally. `allstack verify` prepares the owned single commit and runs the frozen policy against that exact SHA. Confirm positive test execution and unchanged candidate/environment. Any edit or amended commit requires fresh affected verification; do not ship stale receipts. A pending host call must be reconciled, never bypassed.

Use `allstack ship` to publish the exact verified commit and read back the branch and PR head. Confirm Chronis task completion and the Prime source-linked outcome. If publication is uncertain, read the destination before repeating a side effect. Never bypass hooks, push trunk, weaken gates, or rewrite another author's history.

Report the application behavior delivered, PR, exact commit, actual executed checks, and material limits. Required documentation stays indexed under repo docs. Keep private process receipts out of source. The user should not need to manage run IDs or epochs to ask for shipping.

## Host capability

Codex: invoke as $allstack-<name> or request its name naturally. Use native repository tools and its current sandbox. A nested host command requires the same applicable authority as a direct call.
