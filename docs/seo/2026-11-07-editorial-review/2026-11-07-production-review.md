# Production review and owner handoff

Reviewed 2026-11-07 in the in-app browser. The rendered [introductory article](https://www.all-source.xyz/blog/why-event-sourcing-2026) and [CQRS comparison](https://www.all-source.xyz/blog/cqrs-vs-event-sourcing) both loaded. These observations do not establish indexing, canonical selection or traffic. Publication of the corrections remains unverified.

## Ready for owner review

1. **[Kafka comparison](2026-11-06-kafka-comparison.md):** The introductory page still calls Kafka a transport and asserts a sub-millisecond query limitation without a benchmark. The proposed replacement correctly distinguishes streaming, retention and query responsibilities. Apache describes [durable event storage](https://kafka.apache.org/intro/) and [queryable Kafka Streams state stores](https://kafka.apache.org/documentation/streams/developer-guide/interactive-queries.html); Microsoft discusses [event-store and broker differences](https://learn.microsoft.com/en-us/azure/architecture/patterns/event-sourcing). Owner can review the replacement as a factual correction.
2. **[CQRS internal link](2026-11-07-cqrs-link.md):** The introductory article's rendered body has no link to the comparison. The target loads and contains the stated worked subscription example. The proposed link after “Getting Started” is suitable for editorial review. The canonical tag and search index status were not checked.

## Revised recommendations

3. **[Database versioning comparison](2026-11-05-version-control-comparison.md):** Revised replacement addresses the edge-database prompt and acknowledges that retained detail varies by system. Ready for owner review.
4. **[Replay recommendation](2026-11-04-event-replay.md):** Use a short link to the existing replay validation checklist. The target loaded in the browser on 2026-11-07 and supplies practical steps; the introduction and CQRS article already cover replay and projections. Ready for owner review.

## Boundaries

All four recommendations concern the same introductory article. Product/release owner retains implementation and publication decisions. The source-dated `all-source/docs/IMPACT_PLAN.md` records paid growth HOLD and an unchanged BET gate; preserve the free/paid model and unknown revenue. No owner contact was made. No analytics, Search Console, canonical metadata or release state was checked.
