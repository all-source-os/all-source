# Discipline — the false-positive controls

| # | Control |
|---|---|
| 1 | **Delegation exclusion** applies only to a check that both fires *and* covers the path. Never to an unknown trigger, never to a partially-blind check. |
| 2 | **Counter-refuters get the documented kill for the exact signal** that produced the finding, pre-loaded. |
| 3 | **Reproduce or drop.** A defect carries a command that was run with its output pasted, or a `file:line` quoted this session. An unproven suspicion routes to a human — the correct destination, not a bin. |
| 4 | **A kill needs evidence too.** An unevidenced kill does not kill. Without this symmetry kills are cheaper than findings and the panel under-reports. |
| 5 | **Deltas only, measured twice.** No stored counts. |
| 6 | **Direction lives in the command, not the brief.** A grep matching `[+-]` matches both directions identically; pair the tuples by key and compare. |
| 7 | **Distrust prose, trust config.** Re-grep every cited symbol before a finding rests on it. A stale citation in a rule doc or in these references is doc drift, **never** a code finding. |
| 8 | **A check's own warning is a lead, never a finding.** Verify against the tree; name which check you distrusted. |
| 9 | **rock / sand / water.** Only `rock` is a defect. `sand` is a hand-check carrying a pass criterion. Never a number, never a duration. |
| 10 | **Sanctioned shapes carve out.** See below. |
| 11 | **Environment is not a defect.** An unreachable service, a missing fetched model, a cold build, a fresh checkout. **An unavailable tool is a connection failure, never evidence of absence.** |
| 12 | **A subagent's report is testimony.** Re-read every cited `file:line` at HEAD; a citation that does not say what the finding claims drops it, and **the drop is printed**. Cross-agent agreement gives **no** confidence boost. |
| 13 | **Self-consistency.** prove-it ships no gate script and starts no build on its own. A probe that earns permanence is promoted to a real check and wired into `ci.yml` in the same change. When a hand-check needs a build, run one at a time on the machine. |
| 14 | **The orchestrator's own prose is testimony too.** Phase 5.5 reads it: a quantifier carries the command that establishes it, or is **rewritten to the bounded observation and the rewrite printed**. |

## Sanctioned shapes in this repo — rule 10

Each is a finding only under the named extra condition.

| Shape | Where it is prescribed | A finding only when |
|---|---|---|
| Splitting a large Core module into a `store_*.rs` / `*_tests.rs` sibling | the existing layout (`git ls-files 'apps/core/src/store_*'`) | the extraction changed behaviour, not just address (guard-ratchets G7 kill) |
| `ISOLATION_OK: <reason>` / `CORE_PROJECTION_OK: <reason>` | `tooling/tenant-isolation-check/src/main.rs` header | the reason is missing, vague, or names an unlanded fix |
| Excluding a crate from root `[workspace] members` with its own `Cargo.lock` | root `Cargo.toml` comments, repo `CLAUDE.md` § Isolation | the excluded crate then has no CI job (it joins coverage-ledger § Nowhere) |
| `#[ignore = "<reason>"]` on a test needing a model fetch, a live service or a large synthetic archive | existing ignores, e.g. `apps/core/tests/archive_compatibility.rs` | the reason names an unlanded fix |
| Leaving `prime-vectors` out of the integration feature list | `ci.yml` comment above `Run integration tests` | the diff adds a suite that needs it and claims it is tested |
| Committing to `main` directly, with no PR | repo `CLAUDE.md` § Git Workflow | never a finding; the review then runs on a commit range |
