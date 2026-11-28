# Build your own engineering AI harness

Status: proposed homepage design, 26 November 2026. [Research brief and competitive analysis](2026-11-26-cto-landing-brief.md)

## First screen · proposed copy

> For engineering teams using Codex or Claude Code
>
> # Build an AI agent that knows your systems.
>
> Give your coding agent a memory of how your company works. Connect services to their owners and design decisions, so a fresh session can find the answer and show where it came from.
>
> **Build your first harness** · See an example

Place the self-service button first on desktop and mobile. It opens the setup section at `#build-your-first-harness`. The example link opens `#checkout-example`. Put the example alongside the hero on desktop and immediately below it on mobile. Keep setup, example and evidence readable without a form, sign-in or sales conversation.

## See what your team can build

**Who owns checkout, and why does it use a queue?**

> Payments owns checkout. The team chose a queue so requests can wait during payment provider outages. Decision record: ADR-17.

Caption: “Example company data. A fresh local session found the service, followed its links and retrieved the saved decision and source reference.”

**What your engineer adds.** Save Checkout as a service, Payments as its owner, and the queue decision with its source. Link the three records. Together they form a small knowledge graph an agent can query. When checkout changes hands, the team knows which relationship to update. The practical benefit to test is fewer interruptions to the engineer who remembers the original decision.

**Build your first harness.** Start with one service and one question people keep asking. Install AllSource Prime, connect the coding tool your team already uses, and save the relationships. Open a fresh session on the same store and ask your question again. Check the owner and source against the originals. Then replace the example with one of your team's services.

Implementation detail for the setup section: provide two client-specific paths. Link the [Prime install and Claude Code setup](../../apps/prime-mcp/README.md) as the engineering source; on the website link to its [public GitHub path](https://github.com/all-source-os/all-source/tree/main/apps/prime-mcp). Codex supports a local command via `codex mcp add`. Show the installed binary and an explicit absolute data-directory argument. Use one client and one Prime process at a time for this local example. Include a copyable seed prompt specifying all three records, both relationships and the source. Supply the separate retrieval prompt above. Publish each named path after a fresh profile completes registration, writing and lookup without a sales call. A reader must see what succeeded and which step failed.

**Setup card.** With Rust installed, run `cargo install allsource-prime`. Start in a new scratch directory and pick one client. Use the `allsource-example` registration name the prompts refer to, and a new example data directory. Codex registers the server in the user's profile. Claude Code's local registration is scoped to the current project. Both use the absolute path expanded by the shell.

```sh
# Codex
codex mcp add allsource-example -- allsource-prime --mode mcp --data-dir "$PWD/.allsource-example"

# Claude Code (local to the current project)
claude mcp add --scope local allsource-example -- allsource-prime --mode mcp --data-dir "$PWD/.allsource-example"
```

**First session.** “Use the AllSource example tools. Save a service named Checkout, a team named Payments, and this decision with source ADR-17: Queue checkout requests during payment provider outages. Link Checkout to Payments with owned_by and to the decision with governed_by. Show me what was saved.”

**Fresh session.** Close the first client. Open the same project and ask: “Use AllSource to find Checkout. Who owns it, why does it use a queue, and what source is on the decision? Show me the stored evidence.” Compare the answer to the example before trying company data.

After the exercise, remove the example registration with `codex mcp remove allsource-example` or `claude mcp remove --scope local allsource-example` for the selected client. The fresh-profile launch check must verify an unrelated registration survives cleanup.

**Use the same approach elsewhere.** An engineering team can build a service-discovery assistant. Support can check which case authorised a replacement. A GTM team could connect an account decision to its owner and meeting notes. Each team chooses the workflow, supplies source data and connects its tools. Label GTM as an example design. A support history lookup has a local [fixture](../evidence/2026-11-26-cto-landing/restart.json).

**Built with AllSource.** Proposed named-reference copy for partner confirmation: “Longhand builds AI workflows for executives and go-to-market teams. AllSource supports the content management and sync behind those harnesses.” The founder supplied the relationship. The [public Longhand site](https://www.getlonghand.com/) demonstrates shared skills and publishing updates. Use the independently verified local graph example regardless of named-reference availability.

**Want help building yours?** Below setup, examples and technical docs, offer help designing or building a custom engineering harness. Suggested link text: “Talk through a custom build.” Reuse the existing sales contact. Keep the path optional. A consultation, contact form or proposal never gates installation or counts as self-service activation.

## Buyer test

Recruit CTOs or engineering leaders at software companies already using coding agents, with an engineer who owns an internal workflow. The trigger is a repeated systems question whose answer spans a service, a person and a decision. Szymon's subsequent referral opener can be “When someone changes checkout, how does your coding agent find the person and decision behind it?” Use a neutral invitation for the scored test.

Give five comparable first-time readers today's first screen and five the draft, alternating assignment. After five seconds, hide the page. Ask “What can your team do with this?”, “Who builds it?” and “What would you do first?” Record verbatim responses. Score the current page against its own job of reconstructing application state from events. Score the draft when the reader identifies an engineering team building its own agent workflow with saved company context and can point to self-service setup. Record consulting-only interpretations as misses.

After recording unaided responses, ask everyone: “Where, if anywhere, would you try this in your company? Who would own that work?” Score a concrete workflow paired with an accountable person or team. Record “none” and missing owners as misses.

Advance to a larger test if at least 4/5 draft readers explain their job correctly, the draft beats control on comprehension and 2/5 draft readers name a workflow and owner. If visitors expect an agency, revise first-screen copy. If they understand but cannot name a use, revisit the segment. Hold the existing public page for ties or mixed results. These are proposed pilot thresholds.

**Activation.** Count an engineering team once when, without staff assistance, it records one of its own services and a sourced decision, then retrieves both owner and source in a fresh session. Denominator is distinct teams that start setup; record abandonment by step and separate assisted completions. **Business outcome.** Compare median time to a correct owner-and-decision answer and the fraction of matched questions requiring a colleague to intervene. Check answers against the team's source, log misses separately, and report sample counts. No savings claim until measured.

## Semantic ISA and acceptance

The design specifies static copy, two anchors, client setup instructions, existing outbound links and optional email navigation. The website issues no automatic email or writes to customer systems. Manual local setup writes client configuration and synthetic memory to a chosen directory. Existing Treg research calls read public URLs. No automatic connector, analytics pipeline or live department sync is part of this design.

| Boundary | Closed input → output | Acceptance and refusal |
| --- | --- | --- |
| Cognitive · write copy | supported local result or labelled proposal → page text | Admit exact fixture observations and labelled examples. Reject unmeasured savings, unverified integrations and partner endorsements. |
| Memory · seed local fixture | empty disposable directory + three specified nodes + two named edges → local graph | Start with synthetic inputs in an isolated store. Refuse writes into an existing company or user memory store during verification. |
| Execution · setup and navigation | selected client + explicit local path → configured server; selected link → named destination | Test both clients in isolated profiles. Confirm registration, writing, fresh-session lookup and clean process exit. Check both anchors, keyboard navigation, mobile legibility, external links and optional contact destination. A broken client route or sales gate blocks launch. |
| Normative · verify answer | discovered service, two linked nodes and decision history → pass or fail | Require service Checkout, owner Payments, exact recorded decision and ADR-17 on its creation event. Missing edge or changed source fails. A well-worded model answer alone cannot pass. |
| Meta-cognitive · score pilot | cohort, role, response, owner and assistance status → advance, revise or repeat | Exclude unqualified readers, repeat visits and assisted runs from self-service success. Keep denominators and failures. |

User-provided references, marketing pages and model wording retain their source labels. Keep actual customer records out of public examples. On failure, retain the mismatched field and withhold the relevant claim. Named-client and mobile tests are implementation acceptance checks. This document proposes the page and buyer test; publishing needs a separate decision.

Independent design reviews are recorded in the two sibling review files.
