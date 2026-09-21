# AllSource bet

Status: active
Contract version: 2
Shape schema: 1

## Complete mobile delivery

Mobile delivery schema: 1
Decision: 2026-09-21 founder-requested delivery reshape. This is the required
target, not a claim that the apps or purchases have shipped. It supersedes
earlier Android-only, companion-only, browser-only fulfilment, and mobile
not-applicable scope. Existing customer, amount, fee trigger, refund promise,
no-gos, and promotion/kill criteria remain fixed. For mobile payments, trusted
store or service-provider receipts extend the web receipt source; identical
qualification, fulfilment and refund requirements apply.

- Android experience: complete product journey on Android. Onboard and authenticate a tenant, provision hosted access, connect an existing real agent via safe key handoff, inspect ingest/recall/provenance and restart proof, buy or manage Indie, and obtain support. The mobile app controls hosted infrastructure; it does not run the agent or replace API/SDK/MCP clients.
- iOS experience: complete product journey on iOS with the same outcome and domain rules.
- Mobile commerce class: digital
- Android purchase: Google Play Billing; same contracted price and entitlement unit, trusted server verification before unlock.
- iOS purchase: StoreKit; same contracted price and entitlement unit, trusted server verification before unlock.
- Entitlement lifecycle: trusted verification, durable receipt deduplication and recoverable delivery; restore, refund and revocation checks; explicit offline/revalidation policy. Web and mobile entitlement mapping must prevent double charging while keeping local customer data private.
- Mobile fulfilment: Onboard and authenticate a tenant, provision hosted access, connect an existing real agent via safe key handoff, inspect ingest/recall/provenance and restart proof, buy or manage Indie, and obtain support. The mobile app controls hosted infrastructure; it does not run the agent or replace API/SDK/MCP clients.
- App discovery pages: https://www.all-source.xyz/apps, https://www.all-source.xyz/apps/ios, https://www.all-source.xyz/apps/android; required target routes, not claims of current availability.
- Mobile release evidence: `docs/MOBILE_DELIVERY.md` and `docs/LAUNCH_READINESS.md` track Android and iOS independently through signed build, internal installation, physical-device proof, review and verified public store URLs.
- Lesson coverage: 2026-09-21 ten-bet audit; Real onboarding must survive restart and create durable tenants. Paid renewal, not trial signup, is the commercial signal.
- Missing-work queue: repository Chronis epic and dependent Android, iOS, commerce/offer, website, device and release beads listed in `docs/MOBILE_DELIVERY.md`.

Decision and audit: [delivery amendment](decisions/2026-09-21-complete-mobile-delivery.md).

## Customer, trigger, and struggle

- Customer: technical founders and small engineering teams operating a stateful
  AI agent
- Payer: technical founder or engineering lead buying hosted infrastructure
- Decision-maker: person accountable for agent reliability and operating cost
- Trigger: an agent loses useful state after a restart or deployment, or the
  team cannot trace a recalled fact to the event that created it
- Struggle: the team must either operate its own durable memory stack or accept
  opaque, lossy state without replay, point-in-time inspection, or provenance

## Bet

If a technical founder can connect one real agent to hosted AllSource, preserve
its event history across a restart, recall past state, and inspect the source
events, then the founder will keep paying from £18.99 per month because this is
less work and risk than building and operating durable agent memory in-house.

## First market

- Geography: global, sold in English
- Buyer: solo technical founder or small engineering team with one live or
  production-bound agent
- Supported route: hosted AllSource through API, SDK, or MCP-compatible tooling
- First paid plan: Indie; live LemonSqueezy catalog is price source of truth
- Current Indie contract: £18.99 per month, 500K events per month, 14-day
  retention, three streams, and hosted MCP read access
- Queued or excluded: regulated-enterprise procurement, bespoke connectors,
  private-cloud deployment, and broad database-replacement positioning

Self-hosted Apache-2.0 software remains a product-led acquisition route. It does
not count as paid validation.

## Offer and economics

- Outcome: durable, ordered agent events with recall, replay, point-in-time
  inspection, and provenance after restart
- Price: Indie from £18.99 per month; show higher tiers only from live catalog
- Trial: 14 days
- Payment trigger: customer completes hosted checkout and LemonSqueezy records a
  paid subscription
- Renewal trigger: LemonSqueezy records the next paid billing cycle without a
  refund or chargeback
- Cancellation and refunds: published billing terms apply; no business,
  compliance, recall-quality, or latency outcome is guaranteed

## Qualifying customer

A customer qualifies only when all conditions hold:

1. Customer is not founder, employee, contractor, or test account.
2. Agent workload contains customer-controlled, non-demo events.
3. Customer completes ingest, recall or point-in-time query, and provenance or
   replay against that workload.
4. Customer verifies useful state remains available after an agent or client
   restart.
5. Hosted subscription receives first payment and first renewal without refund
   or chargeback.

## Essential circuit

- Acquisition: reproducible technical evidence, self-hosted repository,
  comparison pages, implementation guides, and trusted developer communities
- First value: customer runs a restart proof using one real agent workload and
  sees recalled state with source-event evidence
- Commitment: customer starts hosted trial, creates an API key, and connects a
  non-demo workload
- Payment: customer completes LemonSqueezy checkout for Indie or higher
- Fulfilment: AllSource accepts events and serves recall, replay, point-in-time,
  and provenance operations within purchased limits
- Evidence loop: tenant activation and product events join to trusted
  LemonSqueezy payment, renewal, cancellation, refund, and chargeback state

## Shape

AllSource Core remains the durable event store: WAL with CRC32 and fsync,
Parquet persistence, and in-memory read projections. Hosted services supply
tenant provisioning, authentication, quotas, billing, API access, and MCP
access. Core events remain separate from operational metadata.

First proof uses one agent and one narrow restart-to-recall journey. It does not
require every dashboard tool, every MCP command, a custom migration, or an
enterprise sales motion.

## Approved defaults

- Lead with lost or untraceable agent state, not generic event-store breadth.
- Use Indie as default paid test; recommend higher tiers only when measured
  workload exceeds Indie limits.
- Keep live LemonSqueezy catalog authoritative for displayed and charged prices.
- Offer self-hosting as credible technical proof and a path into hosted service,
  not as promotion-gate evidence.
- Use real customer events for product proof while keeping raw payloads,
  prompts, API keys, and personal data out of analytics.
- Support onboarding with documentation and bounded founder help; custom
  engineering does not count as repeatable product fulfilment.

## Claims and assumptions

| Statement | Class | Evidence or test |
| --- | --- | --- |
| Core persists events through WAL and Parquet and survives restart | Verified fact | Core architecture, recovery tests, and repository documentation |
| Published reference throughput is 469K events/sec | Verified benchmark | Reproducible benchmark article and `siteConfig.stats` |
| Published Core indexed-read latency is 11.9μs p99, not end-to-end memory recall | Verified benchmark | Reproducible benchmark evidence and `siteConfig.stats` |
| Full deployment exposes 73 MCP tools | Verified product fact | MCP tool registry and `siteConfig.stats` |
| Indie is £18.99 per month in current GBP catalog | Founder decision and billing fact | Live LemonSqueezy catalog plus current launch plan |
| One narrow restart and provenance proof creates first value | Recommended default | Smallest complete proof matching buyer trigger |
| Technical founders will retain paid hosting after proving the workflow | Assumption to test | Three qualifying customers retained through first renewal |

Benchmark claims describe measured reference configuration, not every customer
workload. Marketing must link method and limitations near each number.

## No-gos

- Generic “database for everything” or “perfect memory” positioning
- Claims that every alternative is only a vector database or loses data
- Unsourced competitor latency, cost, durability, or feature comparisons
- Claims that self-host installs, GitHub stars, signups, trial starts, demo
  tenants, or founder purchases satisfy the gate
- Bespoke integration work disguised as repeatable onboarding
- Enterprise compliance guarantees, certifications, or SLAs not in force
- Analytics containing event payloads, prompts, API keys, secrets, or raw email
  addresses
- Affiliate programme before paid retention and contribution margin are known

## Gate

Promote after three qualifying non-founder hosted customers each:

- complete the real-workload restart and provenance circuit;
- pay for Indie or higher; and
- remain paid through first renewal without refund or chargeback.

Kill or reshape the hosted-agent-memory offer if either condition occurs:

- five qualified builders complete the real-workload proof and none starts a
  paid subscription; or
- three onboarding attempts require bespoke engineering to reach first value.

## Evidence captured

- Pseudonymous tenant identifier, plan, trial start, acquisition source, and
  partner identifier where applicable
- API-key creation timestamp without key value
- First real event ingest, first successful recall or point-in-time query, first
  provenance view or replay, and restart-proof completion
- Time from signup to first value and failure stage when circuit breaks
- LemonSqueezy subscription, invoice, renewal, refund, cancellation, chargeback,
  and net-revenue state
- Support intervention category and whether custom engineering was required

## Placement check

- [x] Customer, payer, decision-maker, trigger, and struggle are explicit.
- [x] First market and English-language scope are explicit.
- [x] Price source, trial, payment trigger, and renewal evidence are explicit.
- [x] Verified facts, founder decisions, defaults, and assumptions are separated.
- [x] Essential circuit captures product and revenue evidence without payloads.
- [x] Promotion and kill conditions use external behaviour.
- [x] Self-hosting and vanity metrics cannot satisfy the gate.
- [x] No critical placeholder or unresolved product decision remains.
