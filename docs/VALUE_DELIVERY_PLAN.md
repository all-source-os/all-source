# AllSource value delivery plan

Status: active internal accountability plan; customer value not yet reconciled
Owner: founder
Reviewed: 2026-09-21
Tracker: `t-d1796b`; production proof work remains `t-c594ba`

This document does not change prices, contracts, statutory rights, or public
guarantees. [BET.md](BET.md) retains the existing paid validation gate. Its older
agent-first positioning conflicts with the founder's later event-store-first
direction; that conflict is explicit, not silently resolved here. Core is an
event store database; agent memory is one supported use case.

## Customer promise and qualification

**Existing recorded contract, not a fresh price verification:** BET.md records
Indie at £18.99/month. The accepted live LemonSqueezy checkout, purchased plan,
billing interval, currency, tax treatment, and entitlements determine each
customer's actual amount and delivery obligation. `t-5968ae` owns price-copy
reconciliation. Trial limits must not be presented as paid-plan allowances.

The paying technical founder or engineering lead receives hosted event storage
and access to purchased ingest/query/history capabilities within explicit plan
limits. For the existing agent-memory validation cohort, complete delivery
includes a real-workload restart, recall or point-in-time query, and a replay or
source-provenance check. It does not guarantee perfect recall, regulatory
compliance, benchmark latency, unlimited storage, or bespoke integrations.

Before commitment, record supported workload, required retention, event/query
volume, purchased capabilities, and decision-maker authority. Unsupported needs
receive explicit limitations or the self-host route, not an unverified promise.
Exclude founder, employee, contractor, demo, seeded, and QA workloads from
commercial validation.

## Value standard and debt

**Recommended internal default:** evidenced value must exceed the amount paid
for the same delivery period. Target three times that amount only where honest,
documented monetary evidence exists. Neither threshold is a public ROI promise.
Time saved, convenience, confidence, and risk reduction remain descriptive unless
the customer provides a supportable cost basis and attributable avoided cost.

A paid period missing delivery or value proof is value debt, not a pass. Record
the failure, accountable owner, next action, and evidence needed to resolve it.
An unknown payment population means debt is unknown, not zero. Trial and QA
success prove readiness only; zero paid amount cannot produce an ROI multiple.

## Complete delivery

1. Verify settled payment from LemonSqueezy, including amount/currency/period and
   refunds or reversals. Checkout clicks and browser success pages are insufficient.
2. Reconcile subscription to the correct tenant without exposing customer data.
3. Confirm entitlements match the purchased plan and promised retention.
4. Customer-controlled non-demo events are accepted and queryable in that tenant.
5. Capture the required restart/history/provenance circuit against that workload;
   one ingested event alone is not completion of this circuit.
6. Record limitations and support intervention, distinguishing repeatable product
   delivery from custom engineering.
7. Complete value proof or open explicit debt. Confirm first paid renewal without
   refund/chargeback separately before counting toward BET's promotion gate.

## Eligible evidence

| Class | Monetary value eligible? | Required basis |
| --- | --- | --- |
| Realised | Yes | Attributable actual cost avoided or money received, with dated records |
| Avoidable | Yes | Explicit remaining cost the customer can demonstrably avoid |
| Contracted | Yes | Enforceable documented benefit attributable to this delivery |
| Descriptive/unknown | No | Record benefit and uncertainty without invented value |

Do not count a customer's entire revenue, hypothetical outage losses, arbitrary
developer hourly rates, or duplicate savings. Compare value and payment in the
same currency and period; record any conversion source and valuation assumptions.

## Reconciliation sources

| Question | Authority | Not sufficient |
| --- | --- | --- |
| Acquisition and browser attempts | PostHog 244095, `bet=allsource`, QA excluded | Raw shared-project totals |
| Account creation | Tenant-scoped durable auth events | `signup_started`, tenant count, or browser analytics alone |
| Product activation | Non-seeded/non-QA tenant product events | Demo or a signup acceptance |
| Paid fulfilment | Product evidence plus customer-confirmed workflow result | Internal smoke run or single write |
| Payment/renewal/reversal | Trusted LemonSqueezy records and verified webhook state | Checkout redirects or screenshots alone |
| Monetary value | Customer-authorised source evidence | Praise, downloads, impressions, or benchmark claims |

Use pseudonymous references and event IDs, never raw payloads, prompts, keys,
emails, invoices, or private documents in public git/PostHog. Keep source material
in authorised systems. Reconcile each paid period; duplicate webhooks must not
duplicate payments or value, and later reversals must reopen the decision.

## Proposed value-proof record

This is an internal data contract, not a claim that collection is implemented:

```text
schema_version, bet, customer_ref, tenant_ref, traffic_role
payment_ref, subscription_ref, billing_period, currency, amount_paid
qualification_status, purchased_plan, promised_outcome
delivery_status, delivery_event_refs, customer_confirmation_ref
support_intervention, bespoke_work_required
value_basis, value_components, evidenced_value, valuation_assumptions
value_multiple, floor_status, target_status
refund_or_reversal_state, debt_reason, owner, next_action
recorded_at, evidence_refs
```

Allowed floor/target states: `pass`, `fail`, `unknown`, `not_applicable`.
Missing evidence remains `unknown`; never infer delivery from payment.

## Current evidence and blockers

- **Verified:** [signup repair and reconciliation](evidence/2026-09-21-signup-repair.md)
  proves synthetic signup/login, trial limits, event write/read and isolation.
- **Verified scoped snapshot:** Sept 20 00:00–Sept 21 15:03 UTC has zero non-QA
  email signups and zero activations from that cohort. OAuth/historical cohorts
  are excluded; this does not establish zero existing customers or debt.
- **Unknown:** current paid-customer inventory, first renewals, refunds,
  chargebacks, full real-workload delivery, monetary value, and contribution margin.
- `t-c594ba`: real restart/provenance proof and revenue instrumentation.
- `t-7cea76`: legal seller identity and approved governing terms.
- `t-5968ae`: public price copy versus live GBP catalog.

## Work order and spending gate

Resolve missing paid delivery, missing value proof, reversals/complaints, and
production blockers before growth experiments. Continue bounded organic technical
proof without calling it commercial validation.

**Paid acquisition remains HOLD** until an organic source produces qualified
activation or purchase, paid obligations are reconciled, and no unresolved value
debt or critical legal/production blocker remains. Creating this plan does not
lift that gate. Existing promotion/kill thresholds remain unchanged.
