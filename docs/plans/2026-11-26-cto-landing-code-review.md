🟡 MERGE AFTER CHECK — 1 thing nobody tested

1. Can a signed-in Claude Code session seed the example and recover its owner, decision and source in a fresh session?

# Independent implementation review

PR #443 was open at review. Scope includes the design commit `eb5ee741` and working-tree implementation against merge base `6e2b54a3`. The forge reported base tip `e2e15fb1`. Six prove-it lenses, inline. Router: none.

## Claims and hunk coverage

| Claim | Verdict | Evidence |
| --- | --- | --- |
| A referral can proceed directly from the public homepage to instructions or a worked example. | Holds in the route | `apps/web/src/app/page.tsx` mounts the hero and setup. `hero.tsx:21` and `:24` target named sections. The technical resources precede the optional sales link. |
| The graph gives an owner, a decision and a source. | Holds | The [captured graph](../evidence/2026-11-26-cto-landing/engineering-graph.json) names Payments, records the queue decision and assigns ADR-17 to its creation event. A `jq -e` check joined decision and event IDs and passed. Substituting ADR-99 failed with exit 1. The public JSON matched the captured file byte for byte. |
| Teams can connect a coding agent to a disposable local example and recover its records. | Holds for signed-in Codex prompts. Claude registration and storage verified. | Codex ran both published prompts in separate processes. The writer created three nodes and two edges in 19.86 seconds. The reader found Payments, the queue decision and ADR-17 in 28.59 seconds. A third Prime process checked both edges and the decision creation event. The Claude check exercised registration, connection, storage and cleanup. [Client verification](../evidence/2026-11-26-cto-landing/client-verification.md) |
| Existing entry points still lead to technical information and sales help remains optional. | Holds in the route | The legacy `/#pricing` caller in `solutions/quant-intelligence/page.tsx:429` reaches a section linking to the hosted pricing page. The component preserves docs, event-store definition, patterns and blog links. The original sign-in and signup routes remain in the shared header. |
| Keyboard and mobile interactions work in a rendered homepage. | Holds in Chromium | The built route passed at 1440, 390 and 320 CSS pixels. Enter selected Payments and ADR-17 and updated the caption. Both anchors resolved, cleanup opened and the mobile menu closed on setup. The document fit each viewport. [Desktop capture](../evidence/2026-11-26-cto-landing/home-desktop.webp) · [Mobile capture](../evidence/2026-11-26-cto-landing/home-mobile.webp) |

Hunk mapping covers route composition, hero artwork, layout, navigation, release signup and route tests under the first and fourth claims. The graph and public example files support the second claim. The instructions, research, design reviews, client receipts and index support the third. No unclaimed feature hunk identified in the reviewed path list.

## Verify by hand

**Outstanding launch check — Claude Code.** The disposable Claude session returned `Not logged in` with zero output tokens. The existing Claude profile also reported `loggedIn: false`. With a signed-in Claude profile, run `claude` in a scratch project after registering the example server. Submit the published seed prompt and approve its example writes if asked. Close the session, reopen the same project and submit the published lookup prompt. Check the stored edges and creation event for Payments, the exact queue decision and ADR-17 without supplying record IDs to the reader. Remove the example registration and confirm a second registration survives. Review found no outstanding implementation defect. The [buyer test](2026-11-26-cto-landing-design.md#buyer-test) covers unaided customer activation.

## Attacks tried

| Sink or behavior | Verification and counterexample |
| --- | --- |
| Graph disclosure | `graph-plate.tsx:56` selects a record on activation. The live `figcaption` at line 65 exposes the selected text. The rendered component assertions in `harness-home.test.tsx` compare selected content with the original source and public fixture. |
| User-run local commands | `harness-home.tsx:57` tells readers to approve the example's graph writes if asked. The noninteractive Codex default denied the calls and the writer reported that nothing was saved. The successful run preapproved the five graph tools on the disposable server. Both prompts name `allsource-example`, checked in `harness-home.test.tsx:26`. [Client verification](../evidence/2026-11-26-cto-landing/client-verification.md) |
| Public proof navigation | Both files under `apps/web/public/examples/` matched their design fixtures with `cmp`. The support restart entry identifies the decision “Replacement approved” and source `case-1842`. The GTM card explicitly labels its example. |
| Custom contact | The final help block uses a visitor-activated `mailto:` link. Setup and both example files are available before contact. |
| Existing subscription | The form at `changelog-signup.tsx:119` submits to the existing route. `api/changelog/subscriptions/route.ts:54` validates email, consent and placement before forwarding at line 71. The implementation changes the home heading. |

## Ledger

The web CI job invokes the brand-background test at `.github/workflows/ci.yml:233`. That job covers theme assertions. The homepage interaction suite, pictured layout and model conversations fall outside that command. Protection lookup returned HTTP 404, and the rulesets lookup returned an empty list. The repository's documented direct-to-main workflow runs CI after landing. The tracked hook query returned no hook, and the configured global hooks directory had no pre-push file on this machine. The prove-it ledger lists the other web suites as uncovered by that workflow.

## Considered and dropped

- **Placement and reuse.** The route remains in the public web app, uses its existing shared header, footer and signup, and introduces page-specific graph state. The changed-path comparison contained no backend, SDK, manifest, workflow or policy change.
- **Guard weakening.** Repositioning moved a definition link from the hero to technical resources. The updated check targets its new component. The homepage tests now independently inspect the rendered root, fixture equality and graph selection. Scoped suppression and skipped-test scans found no additions in those components.
- **Evidence and disclosures.** The fresh Codex answer quoted the correct decision, then incorrectly said its text had not been returned. The history result contained the decision text and its ADR-17 source reference. The fixture held no full ADR document. Buyer testing and partner confirmation remain separate decisions.

## Self-audit

Checked source positions for route composition, both anchors, graph selection and caption, the legacy pricing entry point, changelog submission and upstream validation, and the CI command. Compared both served fixtures with their captured originals. Re-read the Chromium assertions and inspected the mobile capture. Inspected completed MCP calls and final messages from the denied Codex writer, the successful writer and the fresh reader. Signed-in Claude prompt completion remains unverified.
