# Tenant egress allowances and prepaid overage

Status: implemented locally; commercial values proposed for owner review. Production limits and payment products have not changed.

## Proposed commercial policy

Replace hosted plan retention limits with finite outbound-data allowances. Keep request-rate, event/query, storage and MCP entitlements separate. Removing a plan retention window does not promise unlimited storage or restore deleted records.

| Plan | Included per UTC calendar month | Direct IAD internet-egress cost |
| --- | ---: | ---: |
| Hosted trial/free fallback | 1 GB | $0.02 |
| Indie | 25 GB | $0.50 |
| Studio | 150 GB | $3.00 |
| Scale | 750 GB | $15.00 |
| Enterprise | Explicit finite owner allocation | Allocation × regional unit cost |

| Proposed prepaid pack | Price | Direct IAD egress cost |
| --- | ---: | ---: |
| 100 GB | £10 | $2.00 |
| 1 TB | £75 | $20.00 |

GB means 1,000,000,000 bytes; TB means 1,000 GB. Included allowance resets on the first day of each UTC calendar month, including annually billed subscriptions. Unspent prepaid credit carries across allowance periods without automatic expiry. There is no automatic top-up, postpaid bandwidth debt or skill-authorized spending.

Fly lists North America/Europe internet egress at $0.02/GB; Asia-Pacific/Oceania/South America at $0.04/GB, Africa/India at $0.12/GB. These pack prices target North America/Europe deployment. Reprice before offering higher-cost regions. The public provider unit rate supports these calculations; account-specific financial information is excluded.

Lemon Squeezy lists a base fee of 5% + $0.50, with additional fees possible. The proposed retail buffer also pays for compute, storage, operations, transport overhead and currency exposure. USD network costs and GBP retail prices are deliberately shown separately; these figures are not a complete gross-margin calculation or an exchange-rate claim.

## Enforcement contract

Core owns an append-only, fsync-backed tenant ledger. Included allowance is consumed first, followed by confirmed prepaid credit. Every credential for a tenant shares that balance. Counters live outside mutable subscription metadata. Plan changes preserve consumption; new API keys, tenant deletion/recreation and restarts cannot reset it. Financial ledger history survives tenant deletion, and refunds remain recordable while tenant access is inactive. Unknown tiers require explicit configuration and fail closed.

The gateway reserves up to 16 MiB atomically before upstream work and settles the actual response-body byte count before exposing data. Each server operation receives a fresh random reservation ID; clients cannot replay settlement receipts to obtain another free response. Concurrent replicas use the same canonical Core leader. This is not a split-brain consensus implementation; regional routing must designate one authoritative writer per tenant.

The accounting unit is serialized response-body bytes before transport compression. WebSocket accounting serializes event/reply envelopes before publishing them. HTTP/TLS overhead, refusal responses and internal replication are not identical to this accounting unit; independent infrastructure alerts and ingress/rate controls remain necessary.

A tenant can hold at most four reservations. Each reservation expires after 30 seconds; a late completion cannot authorize delivery. Gateway buffering is limited to 16 simultaneous responses per process, with a 10-second upstream request context. Core HTTP responses also have a 16 MiB cap and 16 simultaneous bounded responses. Oversized responses produce an explicit error without partial data. Library/embedded APIs retain their existing contracts. Core internal WebSocket upgrades remain infrastructure traffic and must remain inaccessible to public clients.

Query Service applies the same canonical ledger to authenticated tenant/API-key HTTP routes and outbound event/projection messages. Unsupported chunked/file/upgrade delivery is rejected before its headers leave the funded HTTP pipeline. Ledger outage fails closed. Billing and authentication remain reachable to recover from exhaustion. x402 auto-pay cannot override an egress refusal.

Exhaustion returns HTTP 402 before calling the upstream data handler. Concurrent capacity returns 429; ledger outages return 503. Error responses are small but consume real network resources. These controls bound funded data delivery; edge protection is still required for high-rate denial-response or anonymous network attacks.

## Prepaid payment lifecycle

The existing billing write permission gates checkout, which selects a server-defined pack, never arbitrary bytes or price. A private durable checkout nonce binds tenant, pack, byte quantity, currency, price and provider variant. Existing orders retain their original binding when the live product mapping changes. Live provider currency, current one-time price, publication state and test mode are checked before checkout. The webhook HMAC is verified over the raw body, whose size is capped.

After a verified webhook, the server fetches the canonical current order and its order item. Store, product variant, GBP currency, unit amount, quantity and total must match. Pending, discounted, test-mode or mismatched orders grant no credit. Credit is keyed by provider store/order identity, atomically with any refund tombstone. Duplicate or reordered paid/refund events cannot mint additional credit or revive a reversed purchase.

Refunds, partial refunds and fraudulent orders conservatively reverse the entire pack. Already-used reversed bytes reduce future prepaid availability; no card charge or automatic debt collection occurs. Included allowance follows its own finite monthly balance. This policy must be disclosed before live sales.

Set LEMON_SQUEEZY_EGRESS_VARIANT_MAP to a JSON map of approved pack IDs (100gb, 1tb) to live one-time variant IDs only after approval. Checkout fails closed when configuration or live verification is missing. No live variant was created, no charge was made and no customer allowance was changed during implementation.

## Retention migration and activation

Subscription webhooks persist retention_days=-1 and the new egress quota. Audit history pagination no longer filters available entries using a plan age limit; UI handles the -1 sentinel without inventing a 90-day fallback. Storage quotas and explicit deletion remain independent.

Before production activation:

1. Approve quantities, UTC calendar-month periods, GBP packs and refund/carryover rules.
2. Inventory actual customer subscription metadata, explicit enterprise allowances, storage quotas and runtime expiry settings. Existing quotas need a controlled metadata migration; code changes alone do not rewrite stored records.
3. Verify every public data route reaches gateway or hosted Query Service enforcement; Core must stay internal. Confirm Query Service uses enterprise build/runtime configuration and a Core credential permitted to access the ledger leader.
4. Configure approved provider variants and order-created/order-refunded webhooks. Preserve their signed raw bodies. Test-provider validation must not grant live credit.
5. Deploy ledger support first, then gateway/Query Service and entitlement/UI changes together. Deploying a caller before compatible Core deliberately fails closed. Keep existing email alerts enabled.
6. Use existing read-only Longhand flows to compare successful reads and production byte rates. Do not create production tenant fixtures. Rolling-day alert totals can remain elevated after instantaneous traffic falls; inspect both measurements.

## Verification

Local proof covers durable included/prepaid exhaustion, concurrent callers, expiry, plan changes, period rollover, duplicate payments, refund replay and WAL recovery. Gateway HTTP integration launches an owned Core server, refuses repeated abusive requests before upstream execution and preserves an independent tenant across restarts. Query Service regressions cover HTTP body settlement, oversized-body refusal, chunked delivery rejection and outbound channel replies. Normal channel and authenticated router-query flows have explicit finite funding fixtures.

The CI workflow .github/workflows/egress-hard-stop.yml builds the tested Core binary, executes ledger tests and requires the real HTTP proof. Missing binary configuration fails the dedicated integration test; it cannot pass by skipping that proof. Full validation results are recorded with the completed change. Production bandwidth improvement is not established by local tests.

## Local validation, 2026-10-23

- Core: 9 ledger tests and 2 response-limit tests passed; strict Clippy passed with warnings denied.
- Control Plane: full Go suite passed; golangci-lint with the egressintegration tag reported 0 issues.
- Real HTTP proof against the final Core binary passed: a 6-byte allowance stopped 20 repeated requests before upstream execution, rotated signed credentials stayed blocked, normal authenticated workflow reads succeeded, duplicate credit/refund replay stayed bounded across two Core restarts.
- Query Service: 1,142 tests and 6 doctests, 0 failures; 2 skipped and 173 integration cases excluded. Credo and formatting passed.
- Web: 12 focused tests passed across 3 files; TypeScript and Biome passed.
- Workflow actionlint and git diff whitespace checks passed. CI has not run for this uncommitted change.
- No production deployment, customer entitlement migration, live provider product creation or payment charge performed. Production bandwidth reduction remains unverified.

Reproduce the dedicated cross-service proof after building Core with multi-tenant support:

```text
ALLSOURCE_CORE_BINARY=<absolute-path-to-tested-core> go test -tags egressintegration -run ^TestEgressRealCore -v -count=1 .
```

Run from apps/control-plane with the repository timeout wrapper (maximum 300 seconds). The test owns and reaps its local Core process and uses private synthetic fixture directories.

## Sources

- [Fly resource pricing](https://docs.fly.io/about/pricing)
- [Lemon Squeezy fees](https://www.lemonsqueezy.com/pricing)
- [Order API](https://docs.lemonsqueezy.com/api/orders/the-order-object)
- [Order-item API](https://docs.lemonsqueezy.com/api/order-items/the-order-item-object)
- [Current price API](https://docs.lemonsqueezy.com/api/prices/the-price-object)
- [Webhook verification](https://docs.lemonsqueezy.com/guides/developer-guide/webhooks)
- [OWASP API4 resource consumption](https://api-security.owasp.org/editions/2023/en/0xa4-unrestricted-resource-consumption/)
