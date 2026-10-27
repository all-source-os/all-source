# Chronis TOON native workflow qualification

## Delivered behavior

TOON string values escape backslashes, pipes and line controls consistently. Non-final action fields also escape colon separators, preserving legacy IDs without changing stored values. An agent identity cannot insert a table column or a second status field. Literal backslash-n and a real newline remain distinct. Ordinary output and silent repeated completion retain their existing behavior.

The [accepted plan](../plans/2026-10-27-chronis-toon-escaping.md), [application request](../reference/chronis-toon-request.md), and [Chronis-specific policy](../../workflows/chronis-native.toml) define acceptance. The existing Prime-only policy remains unchanged.

## Before and after

Baseline application source is `e1416e58eb485bce243f1cc728160f1302289e1c`. Its standalone lockfile needed one existing Core dependency reference corrected from base64 0.22.1 to 0.23.1 before `--locked` compilation. Both baseline and candidate use the identical corrected lockfile, SHA-256 `4e98bebcc65f7992989f7e0d744f16a8184c37a421385591d537a51a3d965d0c`.

The same frozen four-case acceptance file has SHA-256 `46eeaa13966fc153bbc65fb10f36ba9c082474e4dfb0ba0f77cb4e9e77df534a` in both builds. Actual Cargo artifact records show both application packages rebuilt from their respective checkout paths. Builds are separate from test execution.

| Observation | Actual result | Native receipt |
| --- | --- | --- |
| Baseline source build | exit 0; 90,053 ms | `a8db7a79-0c77-41b9-8414-147b377e7512` |
| Baseline frozen acceptance | 1 passed, 3 failed; expected exit 101 | `6f7a5e24-3e8d-4bec-ac42-396f54927d4a` |
| Candidate source build | exit 0; 29,316 ms | `4e589f64-bf9a-420b-8b96-34fcf5dba0ff` |
| Codex candidate acceptance | 4 passed, 0 failed, 0 ignored; exit 0 | `f72c82ed-6e35-4a29-b8de-a2c1f057a87e` |
| Independent Claude acceptance | 4 passed, 0 failed, 0 ignored; exit 0 | `34f692ef-8dd8-4dc6-b278-0a6392df64f6` |

A separate freshly built baseline CLI probe confirmed both title collision and an injected `status:done` line while Chronis JSON reported the actual task as in progress. Its executable SHA-256 is `17102407a20fba247d60bae97519da1d1af025ef23ec650b21055fcb01421abe`.

Candidate `cn` has SHA-256 `130cc8e4982e318a43f6590c08fc5343d1c72196af11fd72d157547958fc2cd9`; the acceptance executable has SHA-256 `ba3b4176c38714c1274e6147d265ba2f655eb6b21858572dba66830e76e56c30`. The test executable's compiled CLI path identifies the original candidate executable; copied and original CLI hashes matched before independent QA, and parent readback confirmed the original hash afterward.

## Actual host work

Authenticated Claude planned from the installed native method and current source in session `49403bff-e4f2-4447-94ef-9045edaccfd1`; 12 observed file-tool calls support that planning claim. Real Codex session `01a17d58-bec1-7a03-a656-ef5d5a3e9836` wrote the formatter/tests. Its first supervised call timed out during inspection; retained diffs were recovered without replaying implementation. A later read-only continuation read the updated native method and final source, and found no remaining actionable source defects (`69a2e9d7-0036-49d0-a778-b4f45d74ff54`).

Independent Claude source review, session `8d45f2b2-7f80-44a3-bcaa-7b6454375f0d`, also required bounded continuation. Actual source inspection identified colon ambiguity in action fields; reading the migration path confirmed arbitrary legacy suffixes. Codex corrected that boundary. Actual CLI dispatch inspection corrected the repeated-completion test expectation. Review recovery receipt is `1368052b-5a76-4af2-a1a0-f86d60b21a03`.

Claude QA session `f9da996b-5cb5-4dbc-a154-4fb612330dfc` explicitly read the updated QA skill and runtime manifest, hashed the test/copied/original CLI executables, executed the frozen acceptance through installed Allstack, and rehashed the test executable. Its outer receipt is `a53fea50-cae2-439f-90b5-47e9193b1a55`. The inner receipt records the four completed tests; the host's prose alone is not the evidence.

The lifecycle case launches separate real `cn` processes for initialization, creation, claim, list/show, stored JSON readback, completion and rejected transitions. Every invocation includes `--toon`. Per-child environment, one operation deadline, kill-then-reap ownership and private temporary workspaces isolate the proof from real project tasks.

## Installed toolkit and evidence

The updated Allstack methods come from [Allstack PR #7](https://github.com/all-source-os/allstack/pull/7). Initial native packaging exercised all 14 host files, created no stores, and preserved a conflicting customization. The installed fixed-dependency macOS artifact has SHA-256 `c29cc5f1d143878e38e726e4a1fc45d6cfb40b0b5e49b674067407fb77724d78`, from [green CI run 40510117757](https://github.com/all-source-os/allstack/actions/runs/40510117757). CI checked out synthetic merge `7efd61db76e2cb980145e7074f637d7cf10affaf`; its Git tree `9742a7b59db3dcdb4a9886c265ba3c428bb9740c` exactly equals the reviewed feature head `0f7f764b492ac9ff9c5b4906201adc516c79b7aa`.

Same-working-directory probing showed Claude absent from the application's inherited shell search path despite its authenticated absolute-path executable working. Explicit host directories in `PATH` resolved discovery. Final application doctor readback reports setup ready, all executables ready, and Chronis readable. No tracked environment file was removed or rewritten.

Private command receipts, separate raw streams and actual host calls remain in ignored proof storage. Required epoch-local evidence must be archived with verified lengths and digests before native shipping can remove accepted worktrees. AllSource owns native delivery history; Chronis and local project Prime own claims and source-linked memory.

## Durable continuation and repository gates

The native run transferred from Codex epoch 1 to Claude epoch 2, then back to Codex epoch 3. Claude read its installed handoff skill and successfully resumed ownership (`97da3708-3521-4a6d-9dd4-6540326ef460`). Codex read its installed handoff skill, resumed with workspace permission, and read the returned plan and report (`f756f3a5-5994-4474-8ece-fce901209a7c`). Actual Chronis readback identified the claimant as `allstack:b67f2a72-0d88-40b5-bf4d-efdc492f4f99:codex:3`. A prior read-only Codex attempt could inspect files but could not write resume state; its successful process exit did not establish a successful handoff.

The full Chronis library, binary and integration suite executed 102 tests with zero failures, ignores or filters (`00ee7008-148f-4001-aa4c-511b7e818b03`). Running from the coordinator matters: Cargo discovers its explicitly excluded Chronis package correctly there. A preliminary command from the nested epoch checkout failed workspace discovery before compilation; it is retained as a failed preflight, not a test result. Native publication gates use the coordinator.

Strict repository checks also require two behavior-preserving Kanban match guards and formatting of an existing claim test. These repair inherited Clippy/format failures on the same branch; no warning suppression or gate reduction is used. Final native verification reruns formatting, all 102 tests and all-target Clippy after preparing the single commit.

## Limits

Qualification covers sequential authenticated hosts on this machine and isolated local task data. It does not prove a second physical host, hosted synchronization on this revision, browser behavior, merge or deployment. Comma-separated dependency IDs retain their existing subformat. Source review and initial timed-out host calls are distinguished from completed runtime checks. Scoped/full gate and publication outcomes are retained separately by the native run and PR checks.

Neither repository currently has applicable protected-branch rules or rulesets; the configured Git hook directory has no commit/push hooks. CI visibility does not establish merge enforcement. This qualification uses explicit native exact-commit verification and destination readback before publication.
