# Prime sync interval verification

Status: CURRENT

## Claim and accepted scope

Prime must reject a zero sync interval before opening its store. Positive configurations must still serve actual MCP tools and deliver events through the existing worker. The [accepted plan](../plans/2026-10-25-prime-sync-interval-validation.md) uses native Allstack skills with authenticated Claude planning, Codex implementation, and independent Claude review and QA.

The repair belongs to the CLI argument declaration in `apps/prime-mcp/src/main.rs`. It retains `u64`, the default 1000 milliseconds, positive values, and CLI precedence over `PRIME_SYNC_INTERVAL_MS`; the parser adds a minimum of 1. Sync and email worker mechanics are unchanged.

## Before: fresh current-main failure

[Baseline argument declaration](https://github.com/all-source-os/all-source/blob/897623e8ad7a86a0bfba1c842d73e3a3685f89b6/apps/prime-mcp/src/main.rs#L127) accepted zero. A newly built baseline Prime 0.26.1, SHA-256 `a6a42ae82fea213ca1415e3363adbca4271b46597022fc136f764f2a59a26543`, reproduced the failure through genuine MCP `initialize` and `prime_stats` requests:

```text
--mode mcp --sync-interval-ms 0 --sync-to http://127.0.0.1:9
process exit: 0
prime_stats: sync.enabled = true
sync.rs:99: `period` must be non-zero.
email_ingester.rs:341: `period` must be non-zero.
```

The store was disposable, with a synthetic API key and loopback destination. No hosted tenant was contacted. Cargo reported a newly compiled application artifact from baseline `897623e8ad7a86a0bfba1c842d73e3a3685f89b6`; executable bytes were retained separately from the candidate build.

The same 19 frozen acceptance cases executed against that source: 14 passed and 5 failed. Failures cover explicit zero, environment zero, explicit zero over positive environment, zero in one-shot modes, and the documented minimum in help. The positive synced session already passed on the baseline; it is a preservation check rather than evidence of the zero repair by itself.

## Frozen acceptance and commands

The freshly rebuilt candidate executable has SHA-256 `5c8973ade235b68e4efa516888b1cf99017c4cfbd6291d108d55dd2917158aa9`. All 19 identical frozen acceptance cases passed. The full prebuilt Prime binary suite also executed: 123 passed, 2 explicitly ignored. Formatting passed.

Claude independently executed both acceptance harnesses through native `/allstack-qa`: 18 parser/MCP cases and one actual synced session passed. Its separate direct binary call verified the same SHA-256, exit 2 for zero, empty stdout, parser error on stderr, and unchanged executable identity. The coordinator separately read back the receipt and confirmed the disposable data directory did not exist. The zero error occurs before store creation, rather than merely suppressing a worker panic.

| Test file | Cases | SHA-256 |
| --- | ---: | --- |
| `apps/prime-mcp/tests/sync_interval.rs` | 18 | `50771e4280b17a56d8ec0993d752ca7cde8373087cfdf240b088b2ad92e29498` |
| `apps/prime-mcp/tests/sync_delivery.rs` | 1 | `da37bd3a91a4512923a1b2d55dc072c94f83fce96a0a260221cd48fc5422b34d` |

Tests execute the built application, isolate environment and store state per child, and kill then reap on timeout. The delivery case creates a node through MCP, searches it back, checks its event payload and identity at a loopback HTTP receiver, observes email polling, and confirms the persisted acknowledgement cursor before clean shutdown. It does not infer delivery from `sync.enabled`.

```console
cargo test --locked --manifest-path apps/prime-mcp/Cargo.toml \
  --no-default-features --test sync_interval --test sync_delivery
cargo test --locked --manifest-path apps/prime-mcp/Cargo.toml \
  --no-default-features --bins --tests
cargo clippy --locked --manifest-path apps/prime-mcp/Cargo.toml \
  --no-default-features --all-targets -- -D warnings
cargo fmt --manifest-path apps/prime-mcp/Cargo.toml --check
```

The existing Prime CI job now includes `--tests`; its former `--bins` selection excluded external process tests. The test step has an explicit bounded timeout. Local qualification uses separate baseline and candidate build directories and waits for an exclusive compiler slot.

## Native workflow and retained state

Claude plan session `65266a08-99ef-464c-98e0-5f7440350d1d` expanded native `/allstack-plan`. Codex thread `01a17549-24a8-7252-9c98-8d49a1aebbfe` read the project plan through native recall and implemented the parser, README, and initial public-process tests. Claude's independent source review identified the missing positive synced-MCP test; that coverage was added and reviewed before runtime qualification.

Installed Allstack 0.2.0 initialized the isolated project coordinator, preserving the tracked 127,146-byte `.chronis/sync/events.jsonl` export and project Prime context. The export SHA-256 remained `5b7825d06aff59801f9cc8cc24a526ff2d8e440e5fa378fba9f8eb11f719bc40`. Source-denied `init --check` returned ready with no changed generated files. Project verification policy is [versioned with the workflow](../../workflows/verification.toml).

Native enrollment is AllSource run `b0142d4c-92d2-4e82-b3b2-ac6db49c8995`, epoch `1`, with Chronis task `t-a5105c` claimed by `allstack:b0142d4c-92d2-4e82-b3b2-ac6db49c8995:claude-code:1`. The project Prime plan write completed, and the coordinator retained the original task export unchanged.

## Publication and limits

All-targets Clippy passed with warnings denied. The versioned policy repeats Prime formatting, executed binary and process tests, lint, and source checks against the prepared single commit. Allstack shipping must read back that exact commit at the remote branch and PR head before marking Chronis complete and indexing the Prime outcome. Publication identity and delivery-store results are retained in the run history and PR.

Raw process receipts and host transcripts remain ignored in the founder-mode coordinator's `.allstack/proof/allsource-native/` directory. Classic main-branch protection was absent and repository/inherited ruleset queries returned no rulesets; CI badges alone therefore do not enforce the verified result. Allstack's verification and shipping checks bind the candidate before publication.

The HTTP receiver is a local test boundary. Hosted Core delivery, dashboard visibility, embedding models, release installation of Prime, merge, deployment, and browser behavior are outside this qualification. The existing global Prime executable and live stores are untouched.

The two ignored Prime tests require an embedding model and the Tesseract CLI. Neither contributes a passing case to the reported counts.
