# AllSource impact plan

Status: active
Owner: founder
Last updated: 2026-09-08
Bet: [`docs/BET.md`](BET.md)
Marketing system: [`docs/MARKETING_STRATEGY_PACK.md`](MARKETING_STRATEGY_PACK.md)

## Outcome

Reach BET gate: three qualifying, non-founder hosted customers complete one
real-workload restart-and-provenance circuit, pay, then renew once without refund
or chargeback.

Traffic, post engagement, GitHub stars, local installs, and trial starts are
diagnostics. None substitutes for gate.

## Current bottleneck

High-intent pages explain durable memory but skip from explanation to pricing,
documentation, or generic installation. Buyer cannot run one bounded proof of
restart recovery plus source history before committing a real workload.

Current experiment changes that handoff only:

`event-sourcing intent -> local restart proof -> tagged hosted connection -> real workload`

## Fixed evaluator

Use same weights for every marketing iteration. Do not change weights after
seeing outcome.

| Signal | Weight | Evidence |
| --- | ---: | --- |
| BET gate progress | 35 | qualifying paid customer and renewal ledger |
| Real restart/provenance proof completion | 20 | witnessed workload circuit and source-history evidence |
| Accuracy, security, and trust | 20 | bounded public claims, no secrets/payloads in analytics, no false proof |
| Net revenue and margin | 15 | live billing and refund/chargeback records |
| Simplicity and reversibility | 10 | code/docs diff, operational burden, rollback cost |

Score experiments 0–5 for each signal. Weighted score is diagnostic; BET gate
still decides continue/kill.

## Baseline — 2026-09-08

- Qualifying hosted customers through gate: not yet evidenced.
- Buyer-near query coverage: pages exist for event-sourced agent memory,
  replay debugging, approach comparison, Mem0, Zep, and Letta.
- Restart-proof route: absent.
- Proof attempt signal: absent.
- Proof-to-hosted attribution: absent.
- GA4 historical comparison: unavailable; setup is current, not historical.
- Site audit: 89 overall; technical 92, content 82, on-page 88, schema 84,
  performance 91, GEO 88, images 98 (`.seo-cache/audit-scores.json`).

## Experiment 01 — proof-first handoff

Hypothesis: builders arriving on restart-safe agent-memory content need a
small, executable proof before trial. A verified local circuit will produce
more qualified hosted connections than a direct pricing/blog CTA, without
weakening trust.

Change:

- publish `/agent-memory-restart-proof`;
- link from `/event-sourcing-for-ai-agents` and `/solutions/agent-memory`;
- show exact write, recall, history, stop, reopen, recall circuit;
- tag hosted handoff `source=restart-proof`;
- record fixed GA4 events for route view, command copy, hosted click, and GitHub
  click; never send command contents, node ids, event payloads, keys, or email.

Local verification performed 2026-09-08 against installed
`allsource-prime`: semantic recall returned the stored decision before and after
process restart; entity history returned the same `prime.node.created` event
with visible `source: runbook-42`. Retrieval score varied slightly after restart
and is deliberately not a marketing claim.

### Decision window

Review after either:

- 100 unique proof-page views, or
- 14 days live,

whichever happens later. Low traffic is distribution failure, not conversion
evidence.

Continue when at least one qualified builder reaches tagged hosted connection
and begins a witnessed real-workload circuit. Improve when proof attempts occur
but hosted handoffs do not. Revert or rewrite when builders cannot reproduce
the local circuit or page causes support load without qualified attempts.

## Buyer-near demand map

No keyword volume is claimed. These are problem-language hypotheses to test in
Search Console, GA4 landing pages, public technical discussions, and direct
builder interviews.

| Intent cluster | Representative searches/prompts | Best owned surface | Proof |
| --- | --- | --- | --- |
| Restart failure | AI agent memory survives restart; agent loses memory after deployment; persistent agent context after crash | restart-proof + event-sourcing pillar | write -> restart -> recall |
| Provenance | trace recalled AI fact to source; agent memory provenance; why did agent remember this | restart-proof + agent-memory solution | entity history with source event |
| Historical state | what did agent know before decision; agent memory time travel; reconstruct agent state at timestamp | event-sourcing pillar + replay guide | history/as-of workflow |
| Correction history | stale AI memory after correction; superseded agent facts; prevent old memory returning | focused correction-history page, only after evidence | update + history + recall |
| Approach choice | event-sourced vs vector agent memory; AI agent memory architecture; RAG memory alternatives | honest approach comparison | bounded trade-off table |
| Vendor comparison | Mem0 alternative with provenance; Zep alternative self-hosted; Letta memory history | existing `/vs/*` pages | sourced capability matrix |
| Framework integration | LangGraph persistent memory provenance; LangChain agent memory restart; LlamaIndex durable agent state; CrewAI shared memory history; AutoGen persistent memory | one verified integration page per framework | runnable adapter/repository |
| MCP/local-first | MCP memory server persistent; local agent memory MCP; Claude memory survives restart | install + Prime docs + restart proof | local MCP/HTTP circuit |

## Sequence

1. Ship and measure proof-first handoff.
2. Observe builder language and failure points; fix proof before adding pages.
3. Improve existing comparison pages with verified proof links and primary
   sources.
4. Publish one framework page only after runnable integration exists.
5. Build correction-history proof when at least three qualified conversations
   identify stale/superseded recall as active pain.
6. Keep `memcheck` draft until it diagnoses a real local stack with sourced
   measurements. Do not ship self-reported competitor scorecards.

## Weekly review

Record:

- proof route views;
- command-copy actions by step;
- proof-to-hosted clicks;
- API keys tagged `source: restart-proof`;
- qualifying real-workload proofs started/completed;
- paid conversions, renewals, refunds, chargebacks, cancellations;
- support minutes per onboarding attempt;
- exact builder objections and failed commands.

Update this file only with observed evidence. Keep [`docs/BET.md`](BET.md)
unchanged unless founder changes customer, trigger, promise, gate, or kill rule.
