# Core memory bound, S1: mutator brief

Goal: lower `peak_live_bytes` (objective, minimize) from the `heap_replay` evaluator
without changing any response byte or any eviction decision.

## Scope

- Mutable: `apps/core/src`.
- Protected inside it: `apps/core/src/infrastructure/persistence/archive_output.rs`,
  `wal.rs`, `event_codec.rs`. Do not touch them.
- Make one change per candidate, inside the mutable path, and leave it uncommitted.

## Forbidden

- Dependencies: no edit to any `Cargo.toml`, `Cargo.lock` or dependency manifest.
- Eviction policy: the items digested by `eviction_policy_unchanged`
  (`tooling/core-heap-replay/src/policy.rs`, `ITEMS`). The only permitted change to
  them is an added ledger call, and the frozen digest rejects any other edit.
- Harness: nothing under `tooling/`, `autoresearch.toml`, `program.md`, `docs/`, or any
  `expected-*` or `tests-S1.txt` file.
- Tests: do not delete, rename or ignore a test. The test list is frozen.
- Responses: request paths, status codes and bodies stay byte-identical. The only
  permitted behaviour changes are the three constraint changes in the design.

## One hypothesis per candidate

State the hypothesis in one falsifiable sentence, then make only the change that tests
it. Record index, commit, hypothesis, gates, objective and decision reason for every
candidate, discards included.

## Gates

A candidate is kept only if every hard gate passes. The hard gates are declared in
`autoresearch.toml`: fixture and build, replay repeatability, query, production-hasher,
codec, geo and eviction digests, ledger two-sided within 10%, rows decoded at or under
`expected/ceilings-S1.json`, expected status, eviction policy and its pin, protected
paths, evaluator self-check, and the core test gates.

## Ties

An equal objective is kept only through the runner's complexity tie-break. Never report
such a keep as an improvement.

## Evaluators

Both evaluators run from the candidate worktree with an empty environment. Their
literal arguments (artifact directory, fixture, environment, pins) live in
`autoresearch.toml`. Do not change them to make a candidate pass.
