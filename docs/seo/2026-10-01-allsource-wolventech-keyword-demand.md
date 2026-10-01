# AllSource and WolvenTech: measured keyword demand

Collected: 2026-10-01. Research only; no website changes or publishing.

## Decision

Prioritize AllSource's event-sourcing database, practical CQRS, and replay/projection content. Keep agent memory as a separate use-case cluster: it has measurable demand too. For WolvenTech, lead with qualified Rust engineering and software-specific diligence offers, supported by implementation evidence. Broad keyword volume alone overstates the available audience for both sites.

Recommendations below are inferred priorities, not measured conversion potential or ranking forecasts.

## Evidence and scope

- Provider: DataForSEO through authenticated treg, WolvenTech team. Google, English, United States (2840) and United Kingdom (2826); search partners excluded from the bulk requests.
- Bulk measurement: 95 exact input phrases in each country, 190 returned rows. Discovery: eight seeds, depth 2, maximum 25 results each; 114 returned related-keyword rows before deduplication.
- Monthly volume below is the provider's rounded monthly search-volume estimate. Returned bulk histories cover September 2025–August 2026 where available. August 2026 is the latest returned month; these are not September results or real-time counts.
- Most discovery volume records were updated September 12–17, 2026. Some sparse records are older; raw evidence retains each timestamp. Discovery difficulty and intent can use older SERP snapshots than volume.
- Observed cost: $0.28968 across ten calls. Opening balance $1.00; closing balance $0.71032, no holds.
- [Full provider responses and call IDs](./2026-10-01-keyword-demand-evidence.json). [All 190 volume rows with monthly history](./2026-10-01-keyword-demand.csv).
- Method references: [Google Ads volume endpoint](https://docs.dataforseo.com/v3/keywords_data/google_ads/search_volume/live/) and [related-keyword endpoint](https://docs.dataforseo.com/v3/dataforseo_labs/google/related_keywords/live/).

Missing values remain unknown: 11 null-volume phrases per country. Explicit provider zero is retained separately: two US rows, one UK row. A rounded zero or unavailable estimate does not prove nobody searches. Close variants can share metrics; do not sum rows to estimate a unique audience. CPC and competition describe paid advertising, not organic ranking difficulty. No GSC, conversion, backlink, or localized rank-tracking data was pulled in this study.

## AllSource priorities

Volumes are monthly estimates. Priority reflects intent and product fit before volume. Difficulty is omitted from this decision table because it was unavailable or based on old SERP snapshots for several phrases.

| Priority | Keyword | US | UK | Recommended treatment |
|---|---|---:|---:|---|
| 1 | event sourcing | 2,900 | 480 | Strengthen the existing event-sourcing product hub, with a runnable append → projection → restart → replay proof |
| 1 | event sourcing database | 70 | 20 | Product-selection intent; make durability, ordering, concurrency, subscriptions and deployment choices explicit |
| 1 | event store database | 50 | 20 | Use existing definition page for education and product hub for evaluation; distinct intent, no duplicate landing pages |
| 2 | cqrs pattern | 1,300 | 390 | Explain command/read separation with an AllSource projection example and boundaries on when CQRS is unnecessary |
| 2 | event sourcing pattern | 260 | 90 | Improve existing patterns index and links to runnable examples |
| 2 | cqrs event sourcing | 90 | 30 | Explain how the two patterns differ and combine; one guide for phrase variants |
| 2 | event sourcing kafka | 70 | 10 | Publish or strengthen a fair stream-broker versus event-store decision guide; do not imply equivalence |
| 3 | agent memory | 1,300 | 170 | Preserve a dedicated agent-memory entry point with provenance and restart proof |
| 3 | ai agent memory | 210 | 40 | Support the same use-case cluster rather than creating a synonym page |
| 3 | immutable database | 210 | 50 | Explain append-only history, corrections and derived state without promising features not implemented |
| 3 | event sourcing vs event driven | 40 | 20 | Focused comparison with concrete architecture diagrams and storage responsibilities |
| 3 | postgres event sourcing | 20 | 10 | Decision guide: starting in Postgres versus a dedicated event store; evidence-backed trade-offs |
| Support | event sourcing rust | 10 | 10 | Runnable Rust integration guide; small measured audience, strong fit |
| Support | event sourcing snapshots | 10 | 10 | Improve the existing snapshot/checkpoint guide rather than launch a new page |
| Support | ai agent memory architecture | 30 | 10 | Diagram facts, derived recall, corrections and replay; link to the memory proof |
| Conditional | temporal database | 1,300 | 320 | Broader and potentially mismatched intent; verify required temporal semantics before targeting |

Observed adjacent terms include `event sourcing architecture` (50 US / 20 UK), `cqrs architecture` (70 / 30), `memory for ai agents` (50 / 10), and `llm memory` (260 / 40). Treat these as supporting questions until SERP overlap and existing page performance justify separate pages.

### Existing page ownership

These routes were observed in the local web source. This is a content mapping, not proof that every deployed route is healthy or indexed. Attempts to fetch `/event-store`, `/agent-memory` and `/event-sourcing/patterns` through the web reader failed; that alone is not an outage diagnosis. The first two are not the source routes used below.

| Existing route | Primary job |
|---|---|
| `/platform/event-sourcing` | Evaluate AllSource as an event-sourcing database |
| `/what-is-an-event-store` | Understand the database category and trade-offs |
| `/event-sourcing/patterns` | Find practical implementation patterns |
| `/event-sourcing/patterns/projections-read-models` | Read-model and projection implementation |
| `/event-sourcing/patterns/snapshots-checkpoints` | Snapshot/checkpoint decisions |
| `/event-sourcing/patterns/event-replay` | Rebuild state from history |
| `/solutions/agent-memory` | Evaluate the agent-memory use case |
| `/compare/eventstoredb` | Fair competitor comparison for evaluation traffic |

Do not create 95 keyword pages. First map each intent to an existing canonical owner, check live content and GSC query/page overlap, then improve examples, internal links and the path to product proof.

### Trend checks

- `event sourcing`: US monthly estimate 2,900, August 2,900; UK estimate 480, August 390.
- `agent memory`: US estimate 1,300, August 1,900; UK estimate 170, August 320. This broader memory phrase has more demand than `ai agent memory` alone, so comparing only the latter with event sourcing would understate the cluster.
- `ai agent memory`: US estimate 210, August 260; UK estimate 40, August 140. Low UK counts make percentage growth unstable.
- `event sourcing kafka`: US estimate 70, August 320, July 20. Treat the spike cautiously; do not build a forecast from one month.

## WolvenTech priorities

The [current site](https://wolventech.com/) describes a one-person, Rust-only advisory practice offering technical due diligence, fractional architecture, platform delivery and advisory support. Target pages must preserve that scope. A high-volume phrase does not justify claiming a staffing agency or broad CTO service.

| Priority | Keyword | UK | US | Recommended treatment |
|---|---|---:|---:|---|
| 1 | hire rust developers | 20 | 110 | A clear page for hiring one senior Rust consultant; state single-operator delivery and engagement terms |
| 1 | rust development company | 10 | 20 | Supporting phrase on the same service page, not a claim of a large team |
| 1 | software architecture consulting | 10 | 90 | Bounded Rust architecture review with deliverables and a redacted example |
| 1 | software architecture review | 10 | 20 | Same offer cluster; distinguish audit findings from implementation work |
| 1 | code review services | 30 | 70 | Rust codebase review, async/concurrency boundaries, safety, performance and actionable output |
| 2 | technical due diligence | 170 | 170 | Software/Rust qualifier essential; the unqualified figure includes non-software demand |
| 2 | software technical due diligence | 10 | 10 | More precise buyer intent; investor/acquirer page with sample risk register |
| 2 | technical due diligence checklist | 10 | 40 | Software-specific checklist linked to the paid assessment, supported by real examples |
| 2 | technical due diligence report | 20 | 10 | Redacted software report showing evidence and limits; avoid property-survey framing |
| 3 | rust vs go | 320 | 1,900 | Measured workload comparison supporting architecture or migration decisions |
| 3 | rust vs c++ | 260 | 1,600 | Use a workload you can reproduce; document trade-offs rather than generic superiority |
| 3 | rust vs python | 210 | 880 | Supporting acquisition content only if it connects to a real migration case |
| Support | rust vs typescript | 10 | 50 | Strong fit with existing delivery experience, despite lower volume |
| Support | mcp server development | 10 | 50 | Bounded Rust MCP delivery offer with source and working example |
| Support | low latency trading systems | 20 | 40 | Evidence-led specialist page; no unsupported performance guarantees |

`rust development` has 70 UK / 320 US searches, but mixes learning and purchasing intent. `rust developers` has 140 / 390 and mixes recruitment with service buying. Use qualified copy and measure lead quality, not visits alone. Search sampling found dedicated vendor pages for [hiring Rust engineers](https://www.rustral.com/hire-rust-developers/) and [part-time Rust engagements](https://gun.io/hire-rust-developers/), supporting commercial intent without establishing a UK-specific ranking opportunity.

`fractional cto` measures 480 UK / 1,600 US, but the current offer is fractional Rust architecture. Do not relabel that offer solely for volume. `fractional rust architect`, `rust contractor`, `rust consultancy`, `wolventech` and `wolven tech` returned null volume in both countries; retain unknown status rather than invent demand.

## Intent traps excluded from demand claims

1. **Rust Consulting brand collision.** `rust consulting` measures 1,000 US / 10 UK; `rust consulting services` 880 / 10. [Rust Consulting, Inc.](https://www.rustconsulting.com/about/portalid/0) provides legal settlement administration. These figures cannot be attributed wholly to Rust programming buyers. Use explicit programming/software terms and do not claim this is a 1,000-search qualified opportunity.
2. **CQRS acronym collision.** `cqrs` measures 4,400 US / 2,900 UK. [NHS CQRS](https://welcome.cqrs.nhs.uk/) is a separate reporting/payment service. Prefer `cqrs pattern`, `cqrs architecture` and `cqrs event sourcing`; do not count all UK acronym searches as developers.
3. **Technical due diligence ambiguity.** Related results include surveys, RICS, construction and real estate. [RICS guidance](https://www.rics.org/profession-standards/rics-standards-and-guidance/sector-standards/real-estate-standards/technical-due-diligence-of-commercial-property) confirms that competing meaning. The 170 UK estimate is not software-only demand.
4. **Event store ambiguity.** Related discovery returned party-store and local retail phrases alongside databases. Use `event store database` prominently. Generic `event store` (320 US / 90 UK) is not a clean category-demand total, and `eventstore` can be competitor navigation.
5. **Brand attribution.** `allsource` measures 1,600 US / 40 UK, but the API does not attribute searches to this product. Do not use this as proof of existing AllSource awareness. `eventstoredb` (170 / 50) is competitor-brand traffic, not AllSource brand traffic.
6. **Discovery noise.** Rust books, jobs, IDEs, named individuals and chemistry terms are excluded from acquisition priorities. More volume is not better if the query cannot lead to the offered service.

## Three actions per site

### AllSource

1. Strengthen the product hub and category explainer with a reproducible append/rebuild/restart/replay demo and explicit database-selection criteria. Cross-link without duplicating primary intent.
2. Improve the patterns hub and projection guide around `cqrs pattern` and `event sourcing pattern`; add the Kafka and Postgres decision content only after checking existing coverage and live SERPs.
3. Preserve the separate memory cluster with provenance and restart evidence. Measure proof completion → signup → verified activation; query volume is not activation evidence.

### WolvenTech

1. Create or strengthen a qualified Rust consulting landing page that makes hiring one senior engineer straightforward and links to technical proof.
2. Give Rust codebase/architecture review and software due diligence clear service descriptions, sample outputs and distinct calls to action. Keep report/checklist synonyms together until result overlap suggests otherwise.
3. Publish one reproducible workload comparison connected to a delivery case and a bounded engagement. Select the workload on evidence, not whichever language comparison has the largest count.

Validation: record publication dates, inspect rendered content and canonical ownership, then compare GSC query/page performance by country over complete 28-day windows. Track qualified enquiries and completed product proof independently. No traffic forecast, conversion lift, full SERP cluster analysis or ranking guarantee is supported by this dataset.

## Call ledger

| Request | treg call ID | Cost USD |
|---|---|---:|
| US event sourcing related | `825ce5439e9947cdbfea2e2406ab146f` | 0.01500 |
| US event store related | `95e6a02f5ba6410dad971e988cd5f62d` | 0.01368 |
| US CQRS related | `a83cce8eee964f84b00481c4ce9b4e19` | 0.01500 |
| US AI agent memory related | `8e7a94066045447fa5ac127955fe4c67` | 0.01296 |
| UK Rust consulting related | `242de14e141a4a19a363f1308f2f9639` | 0.01212 |
| UK technical due diligence related | `147f63d294144f84ae4a875ad623ef13` | 0.01380 |
| UK Rust development related | `2a18ceda06cc458eb5fb3c7db03df6d9` | 0.01500 |
| UK software architecture consulting related | `666d46e696ee455784d75540230d091b` | 0.01212 |
| US 95-keyword volume batch | `443154cdaeb344a4bb83a2982f45cc6a` | 0.09000 |
| UK 95-keyword volume batch | `126ed6ab139f43debeaeb1e4ea8c3229` | 0.09000 |
| **Total** | | **0.28968** |
