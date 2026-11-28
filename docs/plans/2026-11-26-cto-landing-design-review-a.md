---
design: docs/plans/2026-11-26-cto-landing-design.md
design-sha256: 62b8881d05bf8eaf4cf79a73fdc372b88ff8b86a8e66447ac775af843442771b
verdict: clean
---

# Independent design review A

## Engineering harness review · round 2

1. **Closed.** After the three unaided comprehension questions, every participant is asked where they would try the workflow and who would own it. A response earns credit only when it pairs a concrete workflow with an accountable person or team. The pilot advances only if at least two of five draft readers supply both (design lines 57–61).

No new blocking findings.

## Round 3 (superseded)

Reviewed digest `c238ce3f5a0a47624bddd90796f8fb8c5a107efe83ee133034d8be560007eb07`. Verdict was findings.

**Major · The pilot lacks a prompt for its workflow-and-owner threshold.** The three questions at design line 55 measure what the offer does, who builds it and how to start. The rule at line 57 requires two readers to identify a workflow and owner at their own company. A CTO might correctly answer all three questions and never have occasion to name an internal owner. After recording unaided comprehension, ask every participant “Where, if anywhere, would you try this in your company, and who would own that work?” Score only a concrete workflow paired with an accountable person or team. Record “none” as a miss.

The [graph fixture](../evidence/2026-11-26-cto-landing/engineering-graph.json) supports the Checkout example, including both relationships and the supplied ADR-17 reference on the decision creation event. The design identifies named-client setup and partner copy as pending verification.

## Round 2 (superseded)

Reviewed digest `6679ce742df33afe1e29c69ced0f17f14abbbc24eb38252a1fd18bcd47d392c4`. Verdict was clean.

1. The first screen names the memory service as the component that restarts (design line 11). The example identifies a clean Prime restart and explains that the application retains the decision ID for the engineer to retrieve the approval (lines 19–30). This matches the local result in the brief (line 35).
2. Both scored groups now require engineering leaders at companies selling customer-facing AI support agents to businesses. The recruitment rule also requires escalation responsibility and records internal adopters separately (design lines 34–38).

Both earlier blockers are closed. Carry one copy edit into Plan: align the suggested referral opener at design line 40 with the memory-service restart used on the page. The opener is excluded from the scored test.

## Round 1 (superseded)

`design-sha256: superseded` (reviewed digest `69f7c67dd86b382c2c445e754ca6832c1a12fb33d813cf20e56ee39d31ead073`). Verdict was findings.

1. **Blocking. The headline and the proof describe different restarts.** The headline says the agent restarted (design line 9). The local test restarted Prime and fetched history using an existing entity ID (brief line 35). A reader could infer that a restarted agent can locate the approval from an incoming case. Align the first screen and example with the demonstrated lookup, or exercise that agent flow and document how it recovers the identifier.
2. **Blocking. The pilot can advance without testing the selected buyer.** The brief selects companies selling support agents (line 7). The recruitment rule admits B2B companies running support agents (design line 36), including firms using purchased software internally. Five internal operators could satisfy the decision rule while no vendor CTO has seen the page. Qualify the scored group on responsibility for a customer-facing agent product their company sells. Record internal adopters separately if they participate.
