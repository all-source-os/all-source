# Prime TOON control-value integrity

Status: accepted. The user authorized `fix/prime-toon-control-values` and one AllSource PR.

## User job and observed failure

A Prime user stores document text containing control characters, closes the process, and reopens the same memory through an MCP client requesting TOON. The client must decode the response and recover the original text exactly.

At merged AllSource `fc908dd3227c64049820a95ef1ad66432610e6f7`, JSON preserves a five-property node containing `body = "page one,\fpage two"` and `edit = "alpha\b,beta"` (the displayed escapes represent actual control characters). TOON output contains JSON-only `\f`/`\b` escapes. The pinned `@toon-format/toon@4.4.0` consumer rejects it with `Line 5: Invalid escape sequence: \f`.

The observed executable has SHA-256 `1b7f1b8ce499de7552528499a192ea8c3fbaf96c9bd59428e9261f484f8edcb3`, built for PR #369 feature commit `0adf7d265eba0650775a012cb519f89b08447c93`. Its source tree equals the merged baseline tree, `0f8ecdc1ec89d9495912da8df9e9d494d732c343`. This equivalence establishes the diagnostic baseline; final candidate provenance must be established from its actual build.

## Repair owner and compatibility

The generic output encoder owns this invariant: every C0 character in a quoted TOON string uses a supported TOON escape, and decoding preserves the original Unicode scalar sequence. The persisted JSON and upstream text are already correct.

Inspect `apps/prime-mcp/src/toon.rs`. Replace JSON-only escaping for affected string values with the existing TOON escape contract; share a private quoting helper with keys if that is the smallest coherent implementation. Ensure C0-containing values are quoted. Do not strip controls, normalize Unicode, alter the store, add a connector sanitizer or change unrelated ordinary-string policy. Existing compact entity-ID behavior remains covered.

[TOON escaping](https://github.com/toon-format/spec/blob/main/SPEC.md#71-escaping) defines backslash, quote, LF, CR and tab escapes; other C0 controls use lowercase `\uXXXX`. [Value quoting](https://github.com/toon-format/spec/blob/main/SPEC.md#72-quoting-rules-for-string-values) includes C0 controls. This repair qualifies control-value behavior, not every TOON shape or quoting rule.

## Files and acceptance

- Encoder: `apps/prime-mcp/src/toon.rs`.
- Rust regression coverage: sibling/unit or `apps/prime-mcp/tests/toon_values.rs`, with a frozen reference fixture under `tests/fixtures/`.
- Indexed report under `docs/reports/`; keep the existing 132-case key fixture and key tests active.
- Exercise all U+0000–U+001F controls across the supported object/list/inline-array/tabular value positions, plus literal backslashes, Unicode and safe-string compatibility. Freeze reference expectations before implementation; report actual executed counts.
- Baseline must fail the defect-catching cases. Candidate must match the same frozen cases without weakening assertions.
- The private reference corpus is frozen before implementation: 375 cases, SHA-256 `94117f98fd9a753e2cd977cb236d777cc54dc31fe03d0c3b47cf04093f38a165`. It covers 32 C0 controls in comma-free and comma-containing strings, five positions, and 11 compatibility strings. Every reference output independently decodes back to its input.
- A bare control can be accepted by a decoder despite violating the encoder's quoting contract. Check exact frozen encoding as well as decoded equality; do not report all noncanonical cases as decoder rejections. Quoted `\f`/`\b` are the demonstrated invalid-escape failure.
- Separate real processes must write JSON, close, reopen JSON, close, reopen TOON and decode independently. Compare the complete original property map and whole normalized result, not only selected fields. Reuse the baseline-produced store for candidate compatibility.
- Check unknown/invalid cases only where the changed encoder contract applies; do not broaden this repair into a new serialization platform.

## Native flow and gates

Authenticated Claude reads the installed native planning method and relevant source. Authenticated Codex implements the accepted design. Independent Claude reads review/QA methods and performs actual runtime checks against immutable candidate/final binaries. Record calls and denied/failed observations separately from completed checks.

Keep AllSource delivery history, Chronis ownership and source-linked Prime plan/review/QA/outcome underneath. Transfer native ownership between Codex and Claude using actual resume calls and read back each claimant. Pin the chosen Allstack executable and generated methods across hosts.

Preserve the current `workflows/verification.toml`: formatting; executed scoped Prime binary/integration tests; strict all-target Clippy; source whitespace. Existing optional hosted/model/OCR ignores remain explicit limits. No build runs beside another local build; each command stays within 300 seconds.

Publish one exact-verified commit on the authorized live branch and one PR. Read back the remote branch/PR head and CI. Archive raw streams and necessary runtime dependencies before cleanup. No merge, credential creation, hosted sync or deployment is implied.

## Independent planning evidence

Authenticated Claude session `2328098f-b22b-49c8-a2c3-ee4dd6e5829f` read its installed native plan method and current producer/tests/policy. Sixteen observed tool calls yielded fourteen successful tool results; two attempted Bash calls were denied because this planning session exposed only Read/Grep/Glob. Neither denied call executed. Its 136,305 ms completed host call returned an executable plan, preserving the control-value scope and existing key/entity-ID compatibility. Actual receipts remain private. Implementation, candidate execution and publication are still pending.
# Criterion compilation prerequisite

Publication must also repair the inherited Criterion dependency-phase timeout on this PR. CI run 40678542842 stopped with exit 124 before producing a benchmark executable. Use a dependency-only recipe with empty local Core library and harness sources, verify identical registry compiler fingerprints, then compile the real application and benchmark under the existing separate 285-second limits. Keep benchmark cases, optimization settings and comparison thresholds unchanged. This is required delivery work on the existing authorized branch and PR.
