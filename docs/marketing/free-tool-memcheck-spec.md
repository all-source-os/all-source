---
title: "Free-tool spec — memcheck"
status: DRAFT
maps_to: "PRICING_EXPOSURE_PLAN.md §5 (free tool, memcheck recommended) + §6 Week 4 (pick next free tool)"
last_updated: 2026-11-23
---

# DRAFT — `memcheck` Free-Tool Spec

> **DRAFT / SPEC, not an implementation.** This defines a future free-tool funnel
> per plan §5 so it can become its own build prompt later. Nothing here is built,
> deployed, or wired to any service. All AllSource reference numbers trace to
> `siteConfig`/`CLAUDE.md` (fact-check at bottom).

---

## One-paragraph pitch

**memcheck** is a free, no-signup web tool that tells a developer how their
current agent-memory stack stacks up. You pick what you're using today
(mem0 / Letta / Zep / a vector DB / a raw chat log / nothing), describe your
agent's read pattern, and memcheck shows a side-by-side comparison against
AllSource on **cost per recall**. The output is a shareable scorecard ("your stack:
$X per 1M recalls — AllSource: self-host free") with a one-click path to
either self-host or start Indie at £18.99/mo. It's the §5 free-tool funnel: deliver
a genuinely useful answer first, earn the click second.

---

## User flow

1. **Land** on `all-source.xyz/memcheck` — no signup, no card.
2. **Pick current stack** (single select): `mem0` · `Letta` · `Zep` ·
   `Generic vector DB` · `Raw chat-log / context window` · `Nothing yet`.
3. **Describe the workload** (3 sliders / inputs):
   - Recalls per agent turn (1–N)
   - Turns per month (volume)
   - Whether recall is on the hot path (every turn) or occasional.
4. **Compute** → memcheck renders a comparison card:
   - **Monthly recall cost**: your stack's pricing math vs AllSource
     (self-host = $0 infra-only; hosted = Indie/Studio tier + x402 overage at
     $0.0001/call).
5. **CTA**: "Self-host free (Apache-2.0 core)" or "Start Indie — £18.99" + a "Copy scorecard"
   button (shareable PNG / link for the build-in-public loop).

---

## Inputs / outputs

### Inputs

| Field | Type | Notes |
|---|---|---|
| `currentStack` | enum | mem0 / letta / zep / vector-db / chat-log / none |
| `recallsPerTurn` | int | default 1 |
| `turnsPerMonth` | int | drives the volume math |
| `onHotPath` | bool | recall on every turn vs occasional |

### Outputs

| Field | Derived from |
|---|---|
| `yourMonthlyCost` | stack pricing × volume (cited per stack) |
| `allsourceCost` | self-host $0 infra-only; hosted = Indie £18.99 / Studio £78.99 + x402 overage $0.0001/call |
| `scorecardImage` | shareable PNG of the comparison |

**Hard rule for the build:** every comparison number for a *competitor* must
carry a visible source/citation in the UI (their docs / pricing page), and every
AllSource number must read from `siteConfig` at build time — never hardcoded
inline. Posting an unsourced competitor stat is a credibility hit the founder
eats publicly (plan §implementation note).

---

## Minimal build scope (for a future prompt)

- **Surface:** one static Next.js route under `apps/web` (`/memcheck`),
  client-side compute only — no backend, no DB, no auth. (Keeps it a pure funnel,
  no infra to run.)
- **Data:** a single typed `comparisons.ts` table of competitor pricing figures,
  each with a `source` URL; AllSource numbers imported from `siteConfig`.
- **UI:** stack picker + 3 inputs + a result card + "Copy scorecard" (render card
  to PNG via canvas). Reuse `packages/ui`.
- **No data leaves the browser** — privacy is a selling point; nothing is sent,
  stored, or logged.
- **Out of scope (v1):** real benchmarking of the user's actual stack, account
  creation, saving results server-side, the x402 playground (separate §5 tool).

**Definition of done (future prompt):** a developer lands on `/memcheck`, picks
their stack, gets a sourced side-by-side scorecard vs AllSource on cost,
and can copy it to share — with working CTAs to self-host and to Indie £18.99.

---

## Fact-check (AllSource reference numbers → source)

| Claim | Source |
|---|---|
| Self-host free, Apache-2.0 core (`apps/core/LICENSE` says MIT; unresolved) | `siteConfig.pricing[self-host]`, `LICENSE` |
| Indie £18.99 / Studio £78.99 | `siteConfig.pricing[indie]`, `[studio]` |
| x402 overage $0.0001/call | `siteConfig.pricing[*].x402.overage` |
| memcheck as recommended free tool | plan §5 ("free tool" list; memcheck recommended) |

**Competitor figures (mem0/Letta/Zep/vector DB) are NOT asserted here** — the
spec mandates they be sourced+cited at build time, not invented now.
