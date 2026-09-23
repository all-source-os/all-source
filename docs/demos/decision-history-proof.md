# Decision-history restart proof

An offline event-sourcing example, not a live Jev demo.

```console
RUSTC_WRAPPER= cargo run -p allsource-core --no-default-features --features embedded --example decision_history_proof
```

Run from the repository root. The parent creates temporary storage, starts one
process to append four records, then starts a new process to read and replay them:

1. Synthetic ticket: invoice paid, account still locked.
2. Fixture classification: Billing, with source and question/policy versions.
3. Synthetic human correction: Access, referring to the earlier decision.
4. Fixture reevaluation: Billing under a new question version. It does not erase
   the human correction.

Expected final line:

```text
PASS: restart preserves Billing history and Access correction; no model called.
```

Both processes assert the exact stored records. Historical reconstruction after
record 2 returns Billing; after records 3 and 4 it returns Access. Replay takes
only stored records; it has no model client. Unit tests reject broken references,
duplicate/conflicting sequence numbers, invalid order and missing provenance.

This proves graceful separate-process restart and an application-owned reducer,
not crash durability, production classification quality, tenant isolation or a
hosted projection endpoint. Fixture identity is `offline-fixture-not-jev` with
model `no-model-called`. No key is read and no external request is made.

Follow-on work and evaluation gates:
[`2026-09-23-jev-decision-intelligence-design.md`](../plans/2026-09-23-jev-decision-intelligence-design.md).
