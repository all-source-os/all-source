# Verify hosted Prime sync with native skills

Use `allstack-plan`, `allstack-review`, and `allstack-qa` in Codex or Claude. The executable acceptance lives in `apps/prime-mcp/tests/hosted_sync.rs`; native skills interpret its result and inspect the actual hosted records. AllSource owns events, Chronis owns delivery, and Prime holds source-linked context.

## Prerequisites

- Use an authorized checkout and keep `.allstack/` ignored.
- Obtain a temporary tenant credential for `https://api.all-source.xyz` with only `events:read` and `events:write`. Treat it as opaque; hosted keys can be JWTs.
- Supply `PRIME_API_KEY` through your secret manager, or set `ALLSTACK_HOSTED_PROOF_KEY_FILE` to a private file containing the credential. Never put its value in command arguments, transcripts, or source control.
- Set `PRIME_SYNC_TO=https://api.all-source.xyz` and `ALLSTACK_HOSTED_PROOF_RECEIPT` to a new JSON path inside an existing private directory. Each writer run requires a new path.
- Acquire the machine's exclusive compiler slot. Build from the intended source with the same feature/profile configuration used for qualification.

## Codex writer

Ask Codex to use `allstack-qa` and run:

```text
cargo test --locked --manifest-path apps/prime-mcp/Cargo.toml \
  --no-default-features --test hosted_sync -- \
  --ignored --exact hosted_prime_sync_write_restart
```

Bound the command below 300 seconds. A prepared test executable can instead run through `allstack check`, which retains separate raw streams and an expected-exit receipt without creating a delivery run.

The writer checks authentication rejection, creates two uniquely named synthetic decisions through the real local MCP binary, and restarts the same local store between writes. It compares hosted event identities and exact properties, requires a later persisted cursor, and checks local recovery without remote replay duplicates. Its private directory retains local proof data and MCP stdout/stderr. The sanitized receipt contains entity IDs, expected properties, remote event IDs, cursor positions, and the writer binary path; it contains no credential.

## Claude independent reader

Give Claude the same source revision and sanitized receipt. Supply its credential separately. Ask it to use `allstack-qa` and run:

```text
cargo test --locked --manifest-path apps/prime-mcp/Cargo.toml \
  --no-default-features --test hosted_sync -- \
  --ignored --exact hosted_prime_sync_independent_readback
```

The reader opens no writer store and starts no local Prime process. It checks both exact entities through hosted Core and both exact nodes through hosted Prime. Inspect the actual command receipt and require one executed test, matching identities/properties, and no timeout. Ordinary CI ignores these two tests; its green result does not establish hosted delivery.

## Finish

Inspect the two named decisions in the hosted Memory page. Revoke the temporary key and verify a subsequent exact-entity query returns HTTP 401. Keep the synthetic decisions in the tenant's audit trail. Remove private credential files and retain source-bound evidence before deleting your own proof worktree or build cache.

Save the outcome under indexed `docs/reports/`. Native `allstack-ship` can adopt the accepted candidate, run the frozen policy against its exact final commit, publish the authorized PR, and read back Chronis completion and Prime context. Publication does not authorize a merge or deployment.

Codex and Claude can run sequentially on one machine. That proves separate coding-host processes; it does not prove a second physical computer, another tenant, hosted-server restart durability, or vector retrieval.
