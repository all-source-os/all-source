# Chronis TOON integrity

## User outcome

An agent creates and claims a task through `cn --toon`, reopens the workspace through later CLI processes, and reads one truthful set of fields. Literal escape sequences remain distinguishable from control characters. An arbitrary agent identity cannot insert table columns, rows, or a second status field. Completing the task produces the real done status, while stored text remains unchanged.

## Evidence and repair site

Baseline source is `e1416e58eb485bce243f1cc728160f1302289e1c`, Chronis 0.9.1. The installed 0.9.1 CLI reproduced identical title output for literal backslash-n and newline, and raw `CN_AGENT_ID` inserted an extra column and a `status:done`-looking line while actual state remained in progress. The formatter on baseline main has both defects. Rebuild that baseline before claiming executable source provenance.

Authenticated Claude session `49403bff-e4f2-4447-94ef-9045edaccfd1` read the native plan method, captured CLI baseline and formatter/callers through 12 actual file-tool calls. Receipt `1f00b611-0ee9-4c99-b662-f6572fbce943` retained the independent plan; no application execution occurred in that planning pass.

The repair belongs in `apps/chronis/src/presentation/toon.rs`. Keep task projections, stored payloads and JSON output unchanged. Do not reject or rewrite legitimate titles and identities at ingestion. Extend the existing escape helper and consistently apply it to string values; typed enum and boolean output remains stable. Add focused encoder and actual subprocess CLI tests in `apps/chronis/tests/toon_escaping.rs`.

## Rendering contract

Escape backslash as `\\`, pipe as `\|`, newline as `\n`, and carriage return as `\r`. Process original characters once, or escape backslash before introducing other escapes. Decode in one left-to-right scan. Chained reverse replacements do not establish round trips.

Non-final colon-delimited action fields additionally escape colon as `\:`. Legacy migration preserves arbitrary ID suffixes, so generated-ID constraints alone cannot protect these fields. Keep the final action value and key/value detail payloads compatible with literal colons.

Use an escape-aware delimiter parser for table assertions; plain `split('|')` treats escaped pipes as field separators. Keep the existing comma-separated dependency representation for generated task IDs. This repair does not define a new dependency-ID subformat.

## Frozen acceptance

- Exercise both rebuilt baseline and candidate using the same contract cases. A successful build invocation or executable hash alone cannot establish source provenance after cache copies; rebuild the affected package explicitly and preserve its output separately.
- Real CLI processes initialize an isolated workspace, create special-character and ordinary tasks, claim with a pipe/newline-bearing identity, reopen, list/show, complete, and reopen again. Every `cn` invocation includes `--toon`.
- Assert nine actual table columns, one real status field, no injected rows, and different encodings for literal escape sequences and actual control characters. Include backslash-pipe, repeated backslashes, carriage return and Unicode.
- Read original data through Chronis JSON output and assert exact original title and claimant. The CLI must preserve persisted strings across process reopen. Do not inspect storage segments around the engine.
- Assert ordinary output remains compatible and a prohibited lifecycle transition returns a nonzero exit without changing real state. Record executed counts, expected exits, separate streams and digests.
- Preserve CLI completion idempotency: repeated `done` succeeds with no success record and leaves state unchanged. The dispatch layer skips already-completed tasks before the repository's stricter completion method runs.
- Use real authenticated Codex to implement and real authenticated Claude to review and execute the candidate. Inspect actual skill reads and commands before reporting host work as completed. Source review cannot substitute for runtime QA.

## Verification and publication

Use scoped Chronis formatting, executed tests and all-targets Clippy through `workflows/chronis-native.toml`, with at most one machine compiler and each command below 300 seconds. The existing Prime-only policy does not cover this change and remains intact. Cheap text checks run before compilation. Do not change dependencies, unrelated service code, production credentials or deployments.

The user authorized `fix/chronis-toon-escaping` and one PR. The coordinator is the isolated application checkout; preserve the tracked Chronis sync export and existing project memory. Native adoption enrolls the implemented candidate and accepted plan without repeating model planning or implementation. Verify the single commit, archive epoch-local receipts/raw streams before shipping removes accepted worktrees, then read back the remote branch and PR head, completed task and project Prime records. Preserve coordinator state before owned cleanup. Merge and deployment require separate authority.
