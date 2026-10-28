# Prime control-value integrity: native qualification

## Delivered behavior

Prime TOON responses preserve C0 string values through retained-memory reopen. Quoted values use TOON escapes, and every C0-containing value is quoted. Stored JSON, Unicode scalar sequences, existing key encoding and compact entity-ID behavior retain their prior contracts.

The [accepted plan](../plans/2026-10-28-prime-toon-control-values.md) and [delivery request](../reference/prime-toon-values-request.md) define this change. The generic encoder in `apps/prime-mcp/src/toon.rs` owns the repair. Its existing key escape implementation now serves keys and values through one private helper; the value path no longer emits JSON-only short escapes.

## Observed baseline and frozen regression

Merged baseline is `fc908dd3227c64049820a95ef1ad66432610e6f7`, Prime 0.26.1. A retained five-property diagnostic node contains document text with form-feed and backspace. JSON preserves the original map; independent TOON decoding rejects `\f` and `\b`. The diagnostic executable came from the already qualified PR #369 feature commit, whose complete source tree equals the merged baseline tree `0f8ecdc1ec89d9495912da8df9e9d494d732c343`.

The new reference corpus was frozen before encoder implementation: 375 cases, SHA-256 `94117f98fd9a753e2cd977cb236d777cc54dc31fe03d0c3b47cf04093f38a165`. It covers all 32 C0 controls in comma-free and comma-containing strings, five positions, and eleven compatibility strings. All reference outputs independently decode back to their inputs. Frozen Rust harness SHA-256 is `32ecf8d62c1b73e4376ea3993c8c73076f74669c15259f204a174a06792bd75d`.

The dependency-free reference is `@toon-format/toon@4.4.0`, registry integrity `sha512-A6UcsVoWbNdhdLNhJbe6JpRlocmf/DZGG57fur3bd8oQtsjBaKWo2hHbOU2zTuy+byUmLIOyP1xJeitopCZzpQ==`. Published JavaScript is used only for independent corpus generation and decoding; application code and permanent tests remain Rust. [TOON escaping](https://github.com/toon-format/spec/blob/main/SPEC.md#71-escaping) and [value quoting](https://github.com/toon-format/spec/blob/main/SPEC.md#72-quoting-rules-for-string-values) define the C0 contract.

The actual encoder is included by the integration test. Reference string comparisons detect required quoting even when a decoder accepts a noncanonical bare control. Decoder rejection was demonstrated separately for the unsupported quoted short escapes.

## Current-source execution

Both builds use Rust 1.95, unchanged locked dependencies, `--no-default-features` and an exclusively owned build directory. Cargo artifact records identify each actual checkout and show the relevant application/test executables rebuilt. The same frozen harness and corpus were used for the unfixed baseline and repaired candidate.

| Executed observation | Result | Receipt |
| --- | --- | --- |
| Fresh source baseline build | exit 0; 86,777 ms | `7d03891b-8d0e-47ac-8ac0-26a83030cb2b` |
| Baseline focused suite | 8 passed, 2 failed; expected exit 101 | `06dffa5f-6a26-4244-9931-835cfe253e55` |
| Baseline reference corpus | 220 matched, 155 failed of 375 | Same executed suite |
| Baseline real MCP value encodings | 44 matched, 31 failed of 75; all 78 JSON properties persisted | Same executed suite |
| Fresh candidate build | exit 0; 21,979 ms | `4467ff54-d81f-49c5-a88b-9683bbfbb427` |
| Candidate focused suite | 11 passed, 0 failed, 0 ignored | `a6d9f30d-9419-4680-8b0a-50d7f6f183b1` |
| Candidate reference/MCP encodings | 375/375 and 75/75 matched; complete 78-property response preserved | Same executed suite |
| Full scoped Prime suite | 165 passed, 0 failed, 5 ignored | `4b80223c-6adf-4ef6-b203-36d97c30bd24` |
| Strict all-target Clippy | exit 0; warnings denied | `2bb6be0a-42a1-4819-a364-eccf66b0c464` |

The original eight encoder tests remain intact. A sibling case adds leading/trailing form-feed, isolated backspace and literal backslash-f distinctions. Existing 132-case key coverage remains active. File-backed MCP streams, a 90-second operation deadline and 20-second child deadlines avoid pipe blocking; the operation owner kills and reaps each child on timeout or unwinding.

Baseline CLI SHA-256 is `d48215f31e1ee6e0610df807a5e96a682292a505aeadac5b4fc33e04be6d569c`. Candidate CLI SHA-256 is `efb3b308f51d442bb55f129958e4370bc08032bfd2e010bf0708c3590fd827ad`; its focused test executable is `9ff5a5862f37518f56fb9f8a80e2987671d5038016ecb83510186034df66961b`. Immutable copies preserve these observations independently of later target-directory changes.

## Actual hosts and retained-memory QA

Authenticated Claude planning session `2328098f-b22b-49c8-a2c3-ee4dd6e5829f` read its installed native method and source through fourteen successful tool results. Two attempted Bash calls were unavailable and did not execute.

Real Codex session `01a181fb-eb38-78f1-8a84-e6a975bf5672` authored the regression harness before encoder repair, then implemented the shared quoter and edge case. Actual calls, source diffs and frozen hashes establish those edits. It ran no builds in its authoring turns; the orchestrator executed the recorded checks.

Independent Claude session `2391c324-1923-453b-94ab-b69a14ee560e` resumed the actual native handoff, read installed review/QA methods and current source, and found no actionable source defect. Source-review receipt `80c18e35-7abb-4231-8d60-8e1d63dc6dfd` records twelve calls. An unsupported Chronis directory argument failed before the successful coordinator-local claimant read.

Its actual runtime QA used thirteen calls, receipt `9e6f7f05-5f17-462c-828b-719e28fd4777`. Claude hashed both immutable binaries, executed the 375-case encoder test, performed strictly sequential JSON and TOON reads of the original five-property baseline-produced store, ran the pinned decoder, and rehashed the binaries. Actual tool-result ordering confirms JSON completed before TOON launched. All five properties and the complete normalized result match; only the known TOON format marker is removed for comparison. No decode errors or changed binary hashes remained.

Native delivery `66712413-6a19-4587-a359-387994e20530` moved Codex epoch 1 → Claude epoch 2 → Codex epoch 3. Both receiving hosts executed resume and used returned workspaces. Chronis claimant readbacks match each host/epoch. Receiving Codex receipt `45713840-fa36-4531-bf04-e8ca658f36f9` records five actual calls and successful continuation.

## Toolkit identity and publication boundary

The run pins rebuilt Allstack SHA-256 `c06e006476f9aa10c4bffd447ac2d1c3af5963ac76bb27910ee32841be5b9af9`. Its Rust 1.95 build records fresh application artifacts from the owned toolkit checkout, and all seven bundled methods plus fourteen installed host variants match reviewed source. The generated guides carry the same runtime/evidence lessons. Toolkit source builds from merged `12af0eeb2004c91d6d3d5b3000c4a7de90d0dd1c` with the reviewed native-method and guide changes; version equality is not used as provenance.

Publication requires the unchanged four-gate policy against the prepared single commit: formatting, executed Prime binary/integration tests, strict all-target Clippy and whitespace. Native evidence and remote head readback bind that exact commit. AllSource retains delivery history, Chronis owns claim/completion and project Prime stores source-linked plan/review/QA/outcome. Required raw streams, runtime dependencies and project context are preserved before disposable worktree cleanup.

## Limits

Qualification covers sequential authenticated Codex/Claude hosts on this machine and isolated local memory. C0 value encoding and existing key compatibility are exercised; broad TOON conformance, another physical machine, hosted synchronization and deployment remain outside this proof.

Five existing ignores require OCR tooling, an embedding model or hosted credentials. They are excluded from passing counts. Two early 45-second wrapper checks exhausted their budget before application launch; neither established a memory write or application failure, and their failed receipts remain retained.

Copied Rust integration executables embed their compile-time application path. Independent immutable-artifact QA runs the pure encoder filter and invokes the retained CLI directly; it does not present the copied full harness as standalone after cache cleanup.

CI prerequisites found while publishing this work are recorded in [Native delivery CI prerequisites](../plans/2026-10-28-native-ci-prerequisites.md).
