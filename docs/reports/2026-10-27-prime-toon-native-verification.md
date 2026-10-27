# Prime TOON key integrity: native workflow qualification

## Delivered behavior

Prime MCP preserves every original property name when a client reopens retained memory and decodes its TOON response. Non-identifier keys are quoted, and quoted keys use TOON-specific escapes. Ordinary ASCII identifier and dotted keys keep their compact representation.

The [accepted plan](../plans/2026-10-27-prime-toon-key-quoting.md) and [application request](../reference/prime-toon-keys-request.md) define the job. The repair belongs in the presentation encoder. Stored data, JSON output and string-value policy keep their existing behavior. The eight existing encoder tests were moved to a sibling module intact; parent comparison verified their contents and the value-formatting section against baseline bytes.

## Current-source baseline and candidate

Baseline is AllSource `e947beaff71cbf2ce1257a6b08f0f3b4b6a6ad7a`, Prime 0.26.1. Both source builds use Rust 1.95, `--no-default-features`, the unchanged locked dependencies and an owned build directory. Cargo artifact records identify each actual checkout and report the relevant application/test artifacts rebuilt rather than fresh cache hits.

The frozen reference corpus contains 132 cases across 33 keys and four shapes: object fields, nested fields, scalar-array headers and tabular field headers. Corpus SHA-256 is `465d52721c1964c28dbb16b1a95eeb294501d3fb734977a2e4261bfaf42e3809`. Baseline and initial candidate qualification used Rust harness SHA-256 `61af8a54d6201055ea4a74d7be9ee5fda68fc5fb4720769a5399abdb364f541f`. Publication harness SHA-256 is `58ca5f28d0f578c4e9f7c72459fb873c247bdaf7dab95e390eb57908d404b5a8`; only expected-output string assembly differs, with identical assertions and corpus.

| Observation | Actual result | Native receipt |
| --- | --- | --- |
| Rebuilt baseline | exit 0; 106,263 ms | `baseline-build-check.json` in private archive |
| Baseline focused Rust tests | 8 passed, 2 failed; expected exit 101 | `166ca859-8601-4eaf-a2ff-5bdbeaa42982` |
| Baseline reference comparisons | 68 matched, 64 failed of 132 | Same executed harness |
| Baseline real MCP field checks | 17 matched, 16 failed of 33 | Same executed harness |
| Rebuilt candidate | exit 0; 26,582 ms | `76c9fa25-d18c-4956-85e9-ff675291c3ab` |
| Candidate focused Rust tests | 10 passed, 0 failed, 0 ignored | `c9160b92-9bda-4e67-8d58-de5cbfcda17b` |
| Candidate reference/MCP checks | 132/132 and 33/33 matched | Same executed harness |
| Full scoped Prime suite | 152 passed, 0 failed, 5 ignored | `767452df-1610-40b3-874c-31645d8e05fa` |

The two focused tests exercise the actual source encoder and separate real MCP processes. File-backed protocol streams, per-child isolated environment and one operation deadline prevent shared process state or pipe buffering from hiding execution. The child owner kills and reaps on timeout and unwinding.

Strict Rust 1.95 Clippy requires direct string accumulation in the harness, `Duration::from_mins(1)` for seven existing 60-second test constants and `Duration::from_mins(2)` for two existing 120-second hosted test bounds. HTTP and hosted-dispatch tests live in sibling modules under the repository's test-authoring rule. Parent source comparison confirms their executable code is identical after substituting equivalent duration units and ignoring comments/formatting, and production bytes are unchanged.

## Retained memory interoperability

The freshly built baseline created a node with 35 properties through JSON MCP and exited. Later processes reopened that same local store. Baseline JSON preserved all 35 properties; independent TOON decoding returned 34, losing `#policy` as a comment. No property value changed.

The rebuilt candidate reopened the original baseline-created store. Its JSON and independently decoded TOON results match completely: 35 properties, no missing keys and no changed values. This includes distinct precomposed U+00E9 and decomposed U+0065/U+0301 keys. Direct fixture inspection confirmed all 33 corpus keys are distinct.

The dependency-free reference is `@toon-format/toon 4.4.0`, pinned to registry integrity `sha512-A6UcsVoWbNdhdLNhJbe6JpRlocmf/DZGG57fur3bd8oQtsjBaKWo2hHbOU2zTuy+byUmLIOyP1xJeitopCZzpQ==`. Its published JavaScript runs through Bun only for independent interoperability QA; no application dependency was added. [Key encoding](https://github.com/toon-format/spec/blob/main/SPEC.md#73-key-encoding) and [escaping](https://github.com/toon-format/spec/blob/main/SPEC.md#71-escaping) define the tested contract.

Baseline executable SHA-256 is `65ea931ee8c1d78270b460c46218333bd3a8b370a4ed80b26edc4d512062c8a0`. Candidate executable SHA-256 is `bbaf87aa993d80fff4c610987f69ffdb8736933cce365655e7b6fec17a48ffcf`; its focused test executable is `fff4a2927751704c25cccba35762fab91796347ff8d03e2e1283608051a6bdd1`. Immutable copies preserve each runtime identity independently of later target-directory updates.

## Actual Codex and Claude work

Authenticated Claude session `3660e9d5-d0b5-4236-b9c9-8b482c46e30c` read the installed native planning method and source through 14 observed file-tool calls. Its planning revision had identical encoder/policy content to the selected baseline.

Real Codex session `01a17f55-7c5b-7e81-b588-ef524c925216` authored the frozen integration harness before implementing the renderer and moving the eight existing tests. Actual edits, source diffs and frozen hashes establish that work; its prose does not substitute for test execution.

Independent Claude reviewer `ab38518c-e41b-4a54-ac13-d7fab598ee1d` inspected producer/consumer contracts and the candidate. A bounded preliminary review timed out after 20 reads; continuation returned source findings without replaying execution. Candidate source review `9bd82e37-cfa5-40e7-ac3d-6628ede1ade1` found no actionable source defect, with runtime explicitly pending at that point.

Actual independent Claude QA used 12 observed calls (`5804f152-9dc4-4967-9d57-fe075d7d8028`). It read its installed QA method and artifact manifest, hashed both immutable binaries, executed the 132-case encoder test, performed sequential JSON and TOON reads of retained memory, ran the pinned decoder comparison and rehashed both binaries. Corpus receipt `a02fe72d-0679-47a8-8a9b-523ecb2f6cc9` records one executed test with 132 comparisons and nine intentionally filtered sibling tests. Interoperability receipt `4f84cad6-3c90-4623-b0dd-24df441bc00f` confirms complete 35-property equality. Both binary digests remained unchanged.

## Native ownership, installed runtime and publication

Native delivery `d8404323-55a8-4137-8e69-639179e39202` moved Codex epoch 1 → Claude epoch 2 → Codex epoch 3. Both receiving hosts read the installed handoff method and resumed through actual calls. Codex resume receipt is `7bf96399-3d9d-41eb-9dd6-b33cf0612af3`. Chronis readback identifies the active claimant as `allstack:d8404323-55a8-4137-8e69-639179e39202:codex:3`.

The installed Allstack artifact has SHA-256 `c29cc5f1d143878e38e726e4a1fc45d6cfb40b0b5e49b674067407fb77724d78`, verified from [green CI 40518653382](https://github.com/all-source-os/allstack/actions/runs/40518653382). Artifact source tree `2739520c7835934f00ed344465c192f607eb5a28` matches merged Allstack main `12af0eeb2004c91d6d3d5b3000c4a7de90d0dd1c`. All seven bundled methods and all 14 generated application host files matched source. A later shared-global digest change was observed; this run pins the verified artifact in an owned installation prefix. Pinned doctor readback reports setup/executables ready and Chronis readable.

The unchanged Prime publication policy covers formatting, executed binary/integration tests, strict all-target Clippy and whitespace. All four commands must pass against the prepared single commit before native ship. Native evidence and remote PR/branch readback bind publication to that exact commit. AllSource retains delivery history, Chronis owns claim/completion and project Prime records source-linked plan/review/QA/outcome. Raw receipts and streams are archived outside disposable worktrees with verified lengths and digests before shipping cleanup.

## Limits

Qualification covers sequential authenticated hosts on this machine, current-source Prime binaries and isolated local retained memory. It does not prove a second physical machine, hosted synchronization, deployment or general TOON conformance outside key encoding in the tested shapes. Initial installed Prime 0.22.0 observations remain supporting evidence distinct from the rebuilt 0.26.1 baseline.

Five ignored tests require OCR tooling, an embedder model or explicit hosted credentials. They are not counted as passed or as new hosted proof. Compiler contention was handled by waiting for zero active compiler processes; no coordination messages were sent after the user chose waiting. The configured Git hook directory had no commit/push hooks, so publication uses explicit native verification rather than assuming hook enforcement.
