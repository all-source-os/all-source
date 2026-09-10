export type PatternDecision = {
  title: string;
  detail: string;
};

export type PatternFailure = {
  symptom: string;
  fix: string;
};

export type PatternReference = {
  label: string;
  href: string;
};

export type EventSourcingPattern = {
  slug: string;
  shortTitle: string;
  title: string;
  description: string;
  directAnswer: string;
  problem: string[];
  decisions: PatternDecision[];
  implementation: string[];
  exampleTitle: string;
  example: string;
  failureModes: PatternFailure[];
  checklist: string[];
  related: string[];
  keywords: string[];
  references: PatternReference[];
};

const MICROSOFT_EVENT_SOURCING = {
  label: "Microsoft Azure: Event Sourcing pattern",
  href: "https://learn.microsoft.com/en-us/azure/architecture/patterns/event-sourcing",
} as const;

const AWS_EVENT_SOURCING = {
  label: "AWS Prescriptive Guidance: Event sourcing pattern",
  href: "https://docs.aws.amazon.com/prescriptive-guidance/latest/cloud-design-patterns/event-sourcing.html",
} as const;

export const eventSourcingPatterns: readonly EventSourcingPattern[] = [
  {
    slug: "aggregate-streams",
    shortTitle: "Aggregate streams",
    title: "Aggregate Event Streams for Event Sourcing",
    description:
      "Design aggregate event streams with stable identities, explicit boundaries, ordered versions, and practical AllSource event-store conventions.",
    directAnswer:
      "An aggregate stream is the ordered history for one consistency boundary, such as an order, account, or workflow. Give every aggregate a stable entity ID, append only facts owned by that boundary, and derive current state by folding its events in version order.",
    problem: [
      "Streams become hard to reason about when they mirror database tables or UI screens instead of domain ownership. A single customer-wide stream creates needless write contention; one stream per field destroys the ability to enforce invariants. The useful boundary is the smallest unit that must accept or reject a command consistently.",
      "Identity must survive renames and presentation changes. Prefer an opaque domain identifier such as order-01J... over an email address or display label. Put correlation IDs, causation IDs, actor identity, and source-system details in metadata so workflows can be traced without mixing transport concerns into business payloads.",
    ],
    decisions: [
      {
        title: "Choose a consistency boundary",
        detail:
          "Group events that must be validated together. Separate histories that can progress independently, even when a read model later joins them. This keeps optimistic-concurrency conflicts meaningful instead of turning unrelated activity into one hot stream.",
      },
      {
        title: "Name facts, not commands",
        detail:
          "Use past-tense event types such as order.placed and payment.captured. Events describe accepted facts. A rejected command belongs in an operational record only when rejection itself is a domain fact worth retaining.",
      },
      {
        title: "Keep stream order authoritative",
        detail:
          "Treat aggregate version as the ordering signal. Wall-clock timestamps help operators and temporal queries, but clocks can collide or skew and should not replace stream version for state reconstruction.",
      },
    ],
    implementation: [
      "In AllSource Core, entity_id identifies the aggregate stream and each accepted event receives a version. Write structured facts through POST /api/v1/events, then query the entity history in ascending order. Query Service can fold that tenant-scoped history into current-state views without changing Core's durable record.",
      "Start with one event type namespace per bounded context, for example order.*, shipment.*, and payment.*. Register payload schemas before producers multiply. When a process spans streams, keep correlation metadata across events and let a workflow projection show the joined story; do not collapse separate consistency boundaries into one stream for reporting convenience.",
    ],
    exampleTitle: "Order aggregate event sequence",
    example: `order.placed        v1  { total: 14999, currency: "GBP" }
order.line_added    v2  { sku: "CHAIR-01", quantity: 2 }
order.confirmed     v3  { confirmed_by: "customer" }
order.dispatched    v4  { carrier: "DPD", tracking_ref: "..." }`,
    failureModes: [
      {
        symptom: "One tenant or customer stream receives every write and conflicts constantly.",
        fix: "Split by real aggregate identity; retain tenant ID as scope, not aggregate boundary.",
      },
      {
        symptom: "Events say update_order or set_status and cannot explain what happened.",
        fix: "Rename accepted outcomes as stable past-tense domain facts.",
      },
      {
        symptom: "A renamed email or slug creates a second history for the same entity.",
        fix: "Use an immutable entity ID and keep mutable labels inside events or projections.",
      },
    ],
    checklist: [
      "Aggregate owns one enforceable set of invariants.",
      "Entity ID remains stable for lifetime of history.",
      "Event types describe accepted facts in past tense.",
      "Version, correlation, causation, actor, and tenant context are retained.",
      "Cross-aggregate reporting happens in projections, not write streams.",
    ],
    related: ["optimistic-concurrency", "projections-read-models", "multi-tenant-event-streams"],
    keywords: ["aggregate event stream", "event stream design", "event sourcing aggregate"],
    references: [MICROSOFT_EVENT_SOURCING, AWS_EVENT_SOURCING],
  },
  {
    slug: "optimistic-concurrency",
    shortTitle: "Optimistic concurrency",
    title: "Optimistic Concurrency in Event Sourcing",
    description:
      "Prevent lost updates in event-sourced systems with expected stream versions, deterministic retries, conflict handling, and AllSource Core.",
    directAnswer:
      "Optimistic concurrency protects a stream by accepting a write only when its expected version matches the current version. A mismatch means another command changed the aggregate first, so the caller must reload events, re-evaluate the command, and either append a new valid event or report a domain conflict.",
    problem: [
      "Concurrent writers can both read version 12, make decisions from the same state, and attempt version 13. Without a compare-and-append guard, both facts may enter history even when they violate an invariant such as spending the same balance twice or confirming an already-cancelled booking.",
      "A conflict is not a generic transport failure. Blindly retrying the same payload can preserve an invalid decision. Correct handling depends on command semantics: some operations can be recalculated, some are naturally idempotent, and others must return a business-level conflict for a person or upstream workflow to resolve.",
    ],
    decisions: [
      {
        title: "Read with version",
        detail:
          "Load the aggregate's ordered events and retain latest version alongside derived state. Command handling must carry that version into append; dropping it creates a time-of-check/time-of-use gap.",
      },
      {
        title: "Compare at commit",
        detail:
          "Validate expected version inside event-store write path, not in application code before network transit. Only storage layer can make comparison and append one atomic decision for stream.",
      },
      {
        title: "Retry decisions, not writes",
        detail:
          "After mismatch, reload intervening events and run domain decision again. Reusing stale event payload is safe only when operation is explicitly commutative and idempotent.",
      },
    ],
    implementation: [
      "AllSource Core supports expected-version checks in its ingest path. Build aggregate state from tenant-scoped stream, capture current version, then append with that expectation. Core rejects a mismatch before adding event. Record command ID or idempotency key in metadata so network retries can be distinguished from new business commands.",
      "Instrument conflict rate by event type and stream. A sudden rise often signals aggregate boundary that is too broad, a client holding state too long, or duplicate delivery without idempotency. Do not hide persistent conflicts behind infinite retry loops; cap automatic retries, add jitter for transient contention, and surface invariant failures distinctly from version mismatches.",
    ],
    exampleTitle: "Compare-and-append decision",
    example: `read account-42       -> version 18, available = 5000
decide withdraw 2000    -> account.withdrawal_requested
append expected v18     -> accepted as version 19

second append expects 18 -> concurrency error
reload version 19        -> re-evaluate available balance`,
    failureModes: [
      {
        symptom: "Two conflicting commands both become accepted facts.",
        fix: "Require expected version on invariant-sensitive appends.",
      },
      {
        symptom: "Client retries stale event until it eventually appends.",
        fix: "Reload and rerun domain decision after every version mismatch.",
      },
      {
        symptom: "Unrelated commands conflict on a high-traffic global stream.",
        fix: "Narrow aggregate boundary and move cross-stream views into projections.",
      },
    ],
    checklist: [
      "Reads return aggregate version with derived state.",
      "Invariant-sensitive appends include expected version.",
      "Conflict and duplicate-command errors are separate.",
      "Retries rerun decision logic against fresh history.",
      "Conflict metrics identify hot streams and poor boundaries.",
    ],
    related: ["aggregate-streams", "idempotent-consumers", "event-replay"],
    keywords: ["event sourcing optimistic concurrency", "expected version event store"],
    references: [MICROSOFT_EVENT_SOURCING],
  },
  {
    slug: "projections-read-models",
    shortTitle: "Projections and read models",
    title: "Event-Sourced Projections and Read Models",
    description:
      "Build disposable, rebuildable event-sourced projections for fast queries, safe migrations, tenant isolation, and production operations.",
    directAnswer:
      "A projection folds ordered events into a query-specific read model. It is derived state, not a second source of truth: operators must be able to discard it, replay source events, and build a replacement without rewriting event history or stopping existing readers.",
    problem: [
      "Event streams optimize durable history, not every screen or report. Replaying thousands of events per request wastes latency and compute, while embedding every query shape into write model couples new product questions to old command paths. Projections turn one history into several purpose-built views.",
      "Rebuildability changes migration design. Instead of mutating a live table in place, deploy projection v2 beside v1, replay history into it, compare results and lag, then move readers. This works only when reducers are deterministic and external enrichment has already been captured as events or stable metadata.",
    ],
    decisions: [
      {
        title: "One view per access pattern",
        detail:
          "Model fields, indexes, and retention around actual reader. An order summary, operational queue, and finance export can consume same facts while keeping independent schemas and release cycles.",
      },
      {
        title: "Make reducer deterministic",
        detail:
          "Given same ordered events and initial state, reducer must produce same output. Avoid wall-clock reads, random IDs, and network lookups inside fold; record those inputs as facts first.",
      },
      {
        title: "Version projection identity",
        detail:
          "Treat reducer or output-schema change as new projection version. Run old and new views together until rebuild completes and verification passes.",
      },
    ],
    implementation: [
      "AllSource keeps Core as durable event database. Query Service owns tenant-facing projection compute and exposes HTTP, realtime, and analytics reads. For custom Rust read models, ProjectionWorker consumes events through Core's durable-consumer protocol, applies a synchronous reducer, and tracks a server-side WAL cursor.",
      "Name custom workers by projection version, filter event types narrowly, and retain last applied entity version for deduplication. ProjectionWorker checkpoints cursor position, not reduced state, so restart replays events after checkpoint. If cold rebuild is large, persist projection state in its own serving store or use Core entity snapshots as a starting point, while keeping source events authoritative.",
    ],
    exampleTitle: "Side-by-side projection migration",
    example: `orders_summary_v1  cursor 8,240,118  serving readers
orders_summary_v2  cursor 7,992,450  replaying history

verify counts + sampled states
wait until v2 cursor reaches live head
switch readers to v2
retire v1 after rollback window`,
    failureModes: [
      {
        symptom: "Projection cannot rebuild without calling mutable external APIs.",
        fix: "Capture external result as an event before projection consumes it.",
      },
      {
        symptom: "Reducer change silently alters existing live view.",
        fix: "Create versioned projection and replay beside old reader path.",
      },
      {
        symptom: "Per-tenant computed state leaks across boundaries.",
        fix: "Enforce tenant scope before fold and keep tenant-facing projections in Query Service.",
      },
    ],
    checklist: [
      "Every projection has named reader and access pattern.",
      "Reducer is deterministic, total, and idempotent.",
      "Projection version changes with reducer or schema.",
      "Rebuild progress, lag, errors, and last checkpoint are observable.",
      "Cutover and rollback work without changing source events.",
    ],
    related: ["durable-subscriptions", "event-replay", "snapshots-checkpoints"],
    keywords: ["event sourcing projections", "event sourced read model", "CQRS projection"],
    references: [MICROSOFT_EVENT_SOURCING, AWS_EVENT_SOURCING],
  },
  {
    slug: "snapshots-checkpoints",
    shortTitle: "Snapshots and checkpoints",
    title: "Snapshots and Checkpoints in Event Sourcing",
    description:
      "Use event-sourcing snapshots and consumer checkpoints without confusing cached state, delivery progress, and durable source history.",
    directAnswer:
      "A snapshot stores derived aggregate state at a known point in its history; a checkpoint stores how far a consumer processed the event log. Both speed recovery, but neither replaces source events. Snapshot validity depends on reducer compatibility, while checkpoint validity depends on consumer identity and delivery semantics.",
    problem: [
      "Long-lived aggregates can require expensive full replay, and consumers should not rescan old offsets after every restart. Teams often call both optimizations checkpoints, then discover that advancing delivery cursor did not persist materialized state or that an old snapshot cannot be decoded by a new reducer.",
      "Safe recovery needs explicit coordinates. Snapshot should record entity identity, included event count or version, as-of time, state-schema version, and reducer version. Consumer checkpoint should record durable consumer ID and acknowledged log position. Operators need separate reset procedures for each artifact.",
    ],
    decisions: [
      {
        title: "Snapshot by measured replay cost",
        detail:
          "Choose threshold from aggregate size and latency budget, not arbitrary event count. Small streams may never need snapshots; hot long histories may benefit from automatic periodic creation.",
      },
      {
        title: "Checkpoint after durable effect",
        detail:
          "Acknowledge consumer position only after output is committed or safely deduplicated. Early acknowledgement converts process crash into permanent skipped work.",
      },
      {
        title: "Version both artifacts",
        detail:
          "Snapshot state shape and consumer reducer evolve independently. Include versions and make reset or rebuild a routine operation rather than emergency procedure.",
      },
    ],
    implementation: [
      "AllSource Core supports automatic and manual entity snapshots, including as-of coordinates and event counts, then applies later events during reconstruction. Snapshot endpoints let operators create, list, and fetch latest entity snapshots. Source events remain in WAL and Parquet according to retention and compaction policy.",
      "Core's durable-consumer protocol stores WAL cursor positions. The Rust SDK ProjectionWorker acknowledges positions at configured intervals; its reduced state is not automatically stored with cursor. Select checkpoint interval by replay tolerance and ingest overhead. For stateful custom projections, persist view state transactionally with an idempotency marker or rebuild it from acknowledged event range.",
    ],
    exampleTitle: "Two recovery coordinates",
    example: `entity snapshot
  entity_id: account-42
  as_of_version: 12500
  state_schema: 3

consumer checkpoint
  consumer_id: fraud_alerts_v2
  wal_position: 92418830
  reduced_state: stored separately`,
    failureModes: [
      {
        symptom: "Consumer resumes after checkpoint but its local view state is empty.",
        fix: "Persist view state separately or reset cursor and rebuild from source events.",
      },
      {
        symptom: "New reducer loads semantically incompatible old snapshot.",
        fix: "Version snapshot schema and invalidate or migrate incompatible snapshots.",
      },
      {
        symptom: "Checkpoint advances before external side effect completes.",
        fix: "Commit effect first and make repeats idempotent before acknowledging position.",
      },
    ],
    checklist: [
      "Snapshot carries entity version, schema version, and reducer version.",
      "Checkpoint carries stable consumer identity and durable log position.",
      "Acknowledgement happens after durable, idempotent processing.",
      "Reset and full-rebuild paths are tested before production.",
      "Source-event retention outlives required recovery and audit windows.",
    ],
    related: ["event-replay", "durable-subscriptions", "projections-read-models"],
    keywords: ["event sourcing snapshots", "event store checkpoints", "snapshot pattern"],
    references: [MICROSOFT_EVENT_SOURCING, AWS_EVENT_SOURCING],
  },
  {
    slug: "event-replay",
    shortTitle: "Event replay",
    title: "Event Replay for Event-Sourced Systems",
    description:
      "Plan safe event replay for projection rebuilds, incident analysis, backfills, and deterministic testing with AllSource Event Store.",
    directAnswer:
      "Event replay reads immutable events again in their original stream order and applies them to a new or reset consumer. Use replay to rebuild projections, reproduce historical state, test new reducers, or backfill derived outputs—never to re-trigger uncontrolled external side effects.",
    problem: [
      "Replay is powerful because recorded facts become reusable input. It is dangerous when a handler mixes deterministic state reduction with sending email, charging cards, or calling mutable services. Reprocessing old events can duplicate real-world effects unless replay context and idempotency boundaries are explicit.",
      "Production replay also competes with live traffic and may encounter schemas written years ago. A safe plan defines source range, event filters, target consumer version, rate limits, validation sample, catch-up behavior, and cutover. Historical payloads must be decoded through compatible upcasters rather than rewritten in place.",
    ],
    decisions: [
      {
        title: "Separate projection from effect",
        detail:
          "Keep pure state fold independent from outbound effect dispatcher. Replays can rebuild state while effect handlers ignore replay frames or deduplicate by event ID.",
      },
      {
        title: "Replay into new target",
        detail:
          "Build projection or export under versioned name. Avoid destroying current serving view until replacement catches up and passes comparison checks.",
      },
      {
        title: "Bound and observe workload",
        detail:
          "Define start and end coordinates, throughput budget, lag target, error policy, and pause control. Replay is an operation with progress, not opaque background loop.",
      },
    ],
    implementation: [
      "AllSource stores ordered events in Core and exposes query, reconstruction, snapshot, and durable-consumer paths. For a projection rebuild, register new consumer identity, replay from its initial cursor, fold events deterministically, then continue into live WebSocket delivery. Existing view stays available during catch-up.",
      "For incident analysis, query one entity and reconstruct state at timestamp before failure. Compare event IDs, versions, payloads, and metadata with current state; this answers what system knew then without editing history. For large backfills, filter to required event namespaces, throttle processing, and measure cursor lag against Core head before switching readers.",
    ],
    exampleTitle: "Controlled projection replay",
    example: `target: account_balance_v3
source: tenant-scoped Core events
range: position 0 -> captured live head
effects: disabled
validation: totals, entity count, sampled state hashes
cutover: after v3 catches live stream and checks pass`,
    failureModes: [
      {
        symptom: "Historical replay sends duplicate notifications or payments.",
        fix: "Separate pure folds from effects and deduplicate effects by event ID.",
      },
      {
        symptom: "Rebuild overwrites working projection before validation.",
        fix: "Replay into versioned target and use explicit cutover.",
      },
      {
        symptom: "Old payload cannot be read by current code.",
        fix: "Add deterministic schema upcaster; preserve original stored event.",
      },
    ],
    checklist: [
      "Replay range, filters, target, and owner are explicit.",
      "External side effects are disabled or idempotent.",
      "Historical schemas have deterministic read compatibility.",
      "Progress, throughput, lag, failures, pause, and resume are observable.",
      "New output is verified before reader cutover.",
    ],
    related: ["projections-read-models", "idempotent-consumers", "event-schema-evolution"],
    keywords: ["event replay", "event sourcing replay", "rebuild event projection"],
    references: [MICROSOFT_EVENT_SOURCING, AWS_EVENT_SOURCING],
  },
  {
    slug: "idempotent-consumers",
    shortTitle: "Idempotent consumers",
    title: "Idempotent Event Consumers and Handlers",
    description:
      "Design idempotent event consumers for at-least-once delivery, safe retries, ordered processing, and durable AllSource subscriptions.",
    directAnswer:
      "An idempotent event consumer produces the same durable result when it receives an event more than once. Use stable event IDs, entity versions, unique effect keys, and acknowledge only after processing succeeds; at-least-once delivery then becomes recoverable instead of corrupting state.",
    problem: [
      "Networks fail between committing work and acknowledging delivery. Consumer may finish update, lose connection, and receive same event again after restart. Trying to guarantee exactly-once transport across independent systems usually moves ambiguity rather than removes it. Idempotent business effects provide practical guarantee.",
      "Different outputs require different guards. A projection can ignore an entity event whose version is not newer than last applied. A payment or email needs effect ledger keyed by stable event or command ID. An additive metric may need set membership or deterministic upsert instead of increment-on-delivery.",
    ],
    decisions: [
      {
        title: "Choose deduplication key",
        detail:
          "Use immutable event ID for one effect per fact, command ID for one effect per request, or entity version for ordered projection state. Do not derive key from mutable payload fields.",
      },
      {
        title: "Commit before acknowledge",
        detail:
          "Persist output and dedup marker in same transaction where target supports it. Then acknowledge event-store cursor. Crash before ack causes safe repeat; crash after ack leaves committed output.",
      },
      {
        title: "Handle gaps explicitly",
        detail:
          "Per-entity version jump indicates missing or reordered input. Pause that entity, recover gap, then continue rather than accepting silently inconsistent projection state.",
      },
    ],
    implementation: [
      "AllSource durable consumers deliver committed events at least once. Core tracks acknowledged WAL position and replays from stored cursor after reconnect. ProjectionWorker adds per-entity version dedup as safety net, but cross-entity invariants and external effects still require application-level idempotency.",
      "Give each deployed consumer version stable unique name, keep event-type filters narrow, and store processed event ID with outbound result. On reducer error, stop or dead-letter with enough event metadata to diagnose; do not advance cursor past an unhandled fact. Monitor repeat rate, version gaps, processing latency, reconnects, and checkpoint lag.",
    ],
    exampleTitle: "Idempotent effect transaction",
    example: `begin transaction
  if processed_events contains event.id: return success
  upsert invoice_status from event payload
  insert processed_events(event.id, consumer = "billing_v2")
commit transaction
ack durable consumer position`,
    failureModes: [
      {
        symptom: "Counter increments twice after consumer reconnect.",
        fix: "Use event-ID ledger or deterministic aggregate recomputation, not blind increment.",
      },
      {
        symptom: "Cursor advances although target write failed.",
        fix: "Acknowledge only after durable target commit.",
      },
      {
        symptom: "Two consumer versions share identity and move one cursor.",
        fix: "Version consumer IDs and run one owner per identity.",
      },
    ],
    checklist: [
      "Every effect has stable deduplication key.",
      "Output and processed marker commit atomically where possible.",
      "Cursor acknowledgement follows durable processing.",
      "Per-entity gaps and duplicates have explicit policy.",
      "Repeat delivery and checkpoint lag are measured.",
    ],
    related: ["durable-subscriptions", "optimistic-concurrency", "event-replay"],
    keywords: ["idempotent event consumer", "at least once event handler", "event deduplication"],
    references: [MICROSOFT_EVENT_SOURCING],
  },
  {
    slug: "event-schema-evolution",
    shortTitle: "Event schema evolution",
    title: "Event Schema Evolution Without Rewriting History",
    description:
      "Evolve event schemas safely with compatibility rules, versioned contracts, upcasters, migration tests, and AllSource schema governance.",
    directAnswer:
      "Evolve event schemas by preserving stored facts, registering versioned contracts, making compatible additive changes, and translating old payloads at read time when semantics change. Never rewrite production history merely to match newest application model.",
    problem: [
      "Events outlive services that wrote them. A field rename, unit change, split concept, or stricter required value may break projection rebuild years later even when live traffic appears healthy. Database migration that edits old event payloads damages auditability and makes historical evidence depend on mutable scripts.",
      "Schema shape and event meaning are separate. Adding optional field is often structurally backward compatible; changing money from pounds to pence may keep JSON number type while changing semantics. Compatibility review needs examples, ownership, and replay tests—not validation keyword alone.",
    ],
    decisions: [
      {
        title: "Prefer additive evolution",
        detail:
          "Add optional fields with deterministic defaults for old events. Keep old readers working during deployment overlap and avoid reusing a field name for different meaning.",
      },
      {
        title: "Create new event for new meaning",
        detail:
          "When business fact changes materially, publish new event type or major schema version. Document relation to predecessor instead of forcing incompatible payload under familiar name.",
      },
      {
        title: "Upcast at boundary",
        detail:
          "Translate historical representation into current in-memory shape before reducer. Upcaster must be deterministic, side-effect free, version-aware, and tested against captured fixtures.",
      },
    ],
    implementation: [
      "AllSource Core includes schema registry with subjects, versions, validation, and None, Backward, Forward, or Full compatibility modes. Register contract for each governed event type and validate producer payloads before acceptance. Tag schema versions and owners so operators can trace why a change was allowed.",
      "Store original event unchanged. Consumer identifies schema or event version, runs required upcasters, then applies current reducer. Before release, replay representative histories—including oldest retained payloads—into new projection version. If semantic default cannot be derived from event itself, emit compensating enrichment fact rather than querying today's mutable external state during replay.",
    ],
    exampleTitle: "Compatible order event evolution",
    example: `v1 order.placed { "total_pence": 14999 }
v2 order.placed { "total_pence": 14999, "currency": "GBP" }

read v1 -> default currency from recorded tenant contract version
read v2 -> use explicit currency
stored v1 payload remains unchanged`,
    failureModes: [
      {
        symptom: "New required field makes historical replay fail.",
        fix: "Make field optional with deterministic default or introduce new version and upcaster.",
      },
      {
        symptom: "Same event type silently changes business meaning.",
        fix: "Create distinct fact or major contract version and document transition.",
      },
      {
        symptom: "Migration rewrites old payloads and loses original evidence.",
        fix: "Preserve source event; translate on read or emit explicit correction event.",
      },
    ],
    checklist: [
      "Every governed event has owner and registered subject.",
      "Compatibility mode matches deployment and reader requirements.",
      "Semantic changes receive explicit version or event type.",
      "Upcasters are deterministic and covered by historical fixtures.",
      "Full projection replay passes before producer rollout completes.",
    ],
    related: ["event-replay", "projections-read-models", "aggregate-streams"],
    keywords: ["event schema evolution", "event sourcing schema versioning", "event upcasting"],
    references: [MICROSOFT_EVENT_SOURCING],
  },
  {
    slug: "temporal-queries",
    shortTitle: "Temporal queries",
    title: "Temporal Queries and Point-in-Time State",
    description:
      "Answer what changed, what was known, and what state existed at a past time using ordered events, snapshots, and AllSource queries.",
    directAnswer:
      "A temporal query reconstructs entity or system state at a specified stream version or timestamp by selecting only facts accepted by that coordinate and folding them in order. It answers what was recorded then, while current-state query answers what is recorded now.",
    problem: [
      "Conventional current-state rows overwrite prior values. Logs may show requests but not accepted domain state, timestamps may use different clocks, and audit tables often omit reducer logic. Incident response then cannot reliably answer whether a decision was correct given information available at that moment.",
      "Temporal correctness requires named coordinate. Stream version is precise within one aggregate. Global WAL position orders accepted writes across store. Event timestamp supports human time questions but equal or skewed clocks need tie-breaker. Effective business dates may differ from record time and should be explicit payload facts, not substituted for append order.",
    ],
    decisions: [
      {
        title: "Choose temporal axis",
        detail:
          "Use stream version for aggregate reconstruction, durable log position for cross-stream processing boundary, recorded timestamp for operator questions, and domain effective date only when business model needs bitemporal meaning.",
      },
      {
        title: "Fold with historical rules",
        detail:
          "Reducer and upcasters must interpret old facts consistently. Record policy version or decision inputs when reconstructing what system knew matters more than applying today's policy to yesterday's data.",
      },
      {
        title: "Explain result with provenance",
        detail:
          "Return source event IDs, versions, and as-of coordinate with reconstructed state. A historical answer without boundary and evidence is hard to audit and easy to misread.",
      },
    ],
    implementation: [
      "AllSource Core supports point-in-time entity reconstruction and can seed replay from nearest eligible snapshot before applying later events. Query ordered entity events through /api/v1/events/query and use reconstruction endpoints for state questions. Core keeps stored history durable through WAL and Parquet rather than depending on application logs.",
      "Expose as-of coordinate in API response and interface. For a decision audit, store event that records selected policy, input references, and outcome; temporal query can then reconstruct both state and evidence. Prime uses same pattern at application layer to trace recalled facts back to source events and reconstruct what agent memory contained before later corrections.",
    ],
    exampleTitle: "Point-in-time incident question",
    example: `question: what was account-42 state before decision D-918?
boundary: stream version 318
snapshot: version 300
replay: versions 301..318
result: balance 5000, risk_tier "medium"
evidence: source event IDs returned with reconstruction`,
    failureModes: [
      {
        symptom: "Historical query uses current row plus old logs and disagrees with decisions.",
        fix: "Reconstruct from accepted domain events at explicit version or timestamp.",
      },
      {
        symptom: "Equal timestamps produce unstable replay order.",
        fix: "Use stream version or log position as deterministic tie-breaker.",
      },
      {
        symptom: "Today's policy is applied to old facts and called historical truth.",
        fix: "Record policy version and separate reconstruction from re-evaluation.",
      },
    ],
    checklist: [
      "Every temporal query names axis and inclusive boundary.",
      "Replay order has deterministic tie-breaker.",
      "Snapshots never include events after requested coordinate.",
      "Result returns source events and as-of metadata.",
      "Historical reconstruction and present-day re-evaluation are separate operations.",
    ],
    related: ["snapshots-checkpoints", "event-replay", "event-schema-evolution"],
    keywords: ["temporal query event sourcing", "point in time state", "event store time travel"],
    references: [AWS_EVENT_SOURCING, MICROSOFT_EVENT_SOURCING],
  },
  {
    slug: "multi-tenant-event-streams",
    shortTitle: "Multi-tenant streams",
    title: "Multi-Tenant Event Store Stream Design",
    description:
      "Design tenant-scoped event streams with fail-closed authorization, stable aggregate identities, isolated projections, and safe operations.",
    directAnswer:
      "A multi-tenant event store must enforce tenant scope before every read, write, replay, subscription, snapshot, and projection operation. Tenant identity is an authorization boundary carried independently from aggregate identity; matching entity IDs in two tenants must never share history or derived state.",
    problem: [
      "Adding tenant_id field without making it mandatory in query and storage paths creates cross-tenant failure modes. Background rebuilds, admin endpoints, snapshots, cache keys, metrics, and WebSocket filters can bypass controller checks even when ordinary HTTP requests appear isolated.",
      "Embedding tenant only inside entity string is brittle. It invites parsing inconsistencies and makes authorization depend on naming convention. Treat tenant scope as typed request context, validate it at boundary, include it in storage/index keys, and reject missing scope unless endpoint is explicitly system-level with audited privilege.",
    ],
    decisions: [
      {
        title: "Scope before lookup",
        detail:
          "Resolve authenticated tenant and authorization before accessing event IDs, streams, snapshots, or cursors. Filtering after global lookup can leak existence, counts, timing, or payloads.",
      },
      {
        title: "Keep aggregate identity independent",
        detail:
          "Use tenant scope plus stable entity ID as compound boundary. Two tenants may both own order-42 without collision; moving data between tenants becomes explicit migration, not string rename.",
      },
      {
        title: "Partition derived state",
        detail:
          "Tenant-facing projection compute and caches must key by tenant before entity. Administrative global views require separate code path, role, telemetry, and audit trail.",
      },
    ],
    implementation: [
      "AllSource hosted architecture authenticates tenant at control and Query Service layers, while Core remains source of truth for events and operational metadata. Query Service owns per-tenant user-facing projections; Core stores enabled set as opaque tenant metadata and serves tenant-scoped event history. This keeps hot ingest engine from becoming tenant-specific compute layer.",
      "Propagate tenant context to event ingestion, queries, durable-consumer registration, WebSocket delivery, snapshot lookup, and replay. Use fail-closed defaults: missing or invalid tenant context returns error, not global results. Run adversarial tests with same entity and consumer IDs across tenants, including historical reconstruction and reconnect paths.",
    ],
    exampleTitle: "Compound stream boundary",
    example: `tenant acme + entity order-42 -> independent stream
tenant orbit + entity order-42 -> independent stream

query scope: tenant resolved from authenticated context
projection key: (tenant_id, entity_id)
missing tenant: reject
system-wide operation: explicit admin path + audit event`,
    failureModes: [
      {
        symptom: "Entity snapshot cache keys only by entity ID.",
        fix: "Include tenant in key or disable unsafe snapshot fast path for tenant-scoped reads.",
      },
      {
        symptom: "WebSocket reconnect receives events outside tenant filter.",
        fix: "Bind tenant authorization to durable consumer and server-side subscription filter.",
      },
      {
        symptom: "Missing tenant parameter returns global data.",
        fix: "Fail closed; reserve global access for explicit audited administration route.",
      },
    ],
    checklist: [
      "Tenant scope is typed authenticated context, not optional query text.",
      "All indexes, caches, snapshots, consumers, and projections include tenant boundary.",
      "Missing scope fails closed.",
      "Administrative cross-tenant paths are separate and audited.",
      "Isolation tests reuse same IDs across tenants and every access path.",
    ],
    related: ["aggregate-streams", "projections-read-models", "durable-subscriptions"],
    keywords: ["multi tenant event store", "tenant event streams", "event sourcing SaaS"],
    references: [AWS_EVENT_SOURCING],
  },
  {
    slug: "durable-subscriptions",
    shortTitle: "Durable subscriptions",
    title: "Durable Subscriptions and Consumer Checkpoints",
    description:
      "Run durable event-store subscriptions with replayable cursors, acknowledgements, reconnect recovery, lag controls, and AllSource consumers.",
    directAnswer:
      "A durable subscription gives a named consumer a server-tracked position in the event log. After reconnect, event store replays committed events after last acknowledged position, then switches consumer to live delivery. Processing remains at least once, so handlers must be idempotent.",
    problem: [
      "Ephemeral pub/sub drops events while consumer is offline. Client-managed offsets scatter recovery state across services and can advance past failed work. Durable server-side cursor makes restart behavior explicit, but still needs single ownership, acknowledgement policy, filters, lag monitoring, and poison-event handling.",
      "Consumer name is stateful identity, not cosmetic label. Reusing one name for two replicas can race cursor updates; reusing after reducer change can skip history new version needs. Stable name per logical consumer version turns reset, replay, blue-green migration, and operational ownership into manageable choices.",
    ],
    decisions: [
      {
        title: "Name consumer by output contract",
        detail:
          "Include purpose and version, such as search_index_v3. New reducer or destination schema receives new identity so it can replay independently beside current consumer.",
      },
      {
        title: "Acknowledge in batches deliberately",
        detail:
          "Small interval reduces replay after crash but increases checkpoint writes. Large interval improves throughput but expands duplicate window. Tune against processing cost and recovery target.",
      },
      {
        title: "Design lag and poison policy",
        detail:
          "Measure distance from live head, cap retry loops, and preserve failed event evidence. Pause or dead-letter deliberately rather than acknowledge malformed facts silently.",
      },
    ],
    implementation: [
      "AllSource Core durable-consumer protocol registers consumers, delivers replay frames over WebSocket, and accepts acknowledgement positions through consumer endpoint. Cursor metadata is event-sourced through Core system streams, so service caches can rebuild after restart. Reconnection uses stored position rather than process memory.",
      "Rust ProjectionWorker wraps protocol with event filters, checkpoint interval, exponential reconnect backoff, replay-to-live transition, and per-entity version dedup. It is single-instance per consumer name; shard with explicit distinct identities and partition-aware reducers. Watch replay count, checkpoint age, reconnect rate, lag notices, reducer latency, and current position versus Core head.",
    ],
    exampleTitle: "Replay-to-live lifecycle",
    example: `POST /consumers -> register search_index_v3
connect WebSocket ?consumer_id=search_index_v3
receive replay positions 81200..81500
POST /consumers/search_index_v3/ack { position: 81500 }
receive replay_complete
continue with live committed events`,
    failureModes: [
      {
        symptom: "Two workers with same identity move one cursor unpredictably.",
        fix: "Run single owner per name or implement explicit partitioned consumer identities.",
      },
      {
        symptom: "Slow reducer falls behind live broadcast repeatedly.",
        fix: "Measure lag, optimize or shard reducer, and rely on replay after reconnect.",
      },
      {
        symptom: "Bad event retries forever without operator visibility.",
        fix: "Expose failure, retain payload reference, and define pause, fix, replay, or dead-letter policy.",
      },
    ],
    checklist: [
      "Consumer ID names logical output and reducer version.",
      "One active owner or explicit sharding model exists.",
      "Acknowledgement interval matches replay tolerance.",
      "Handlers are idempotent under duplicate delivery.",
      "Lag, reconnect, error, position, and replay metrics are visible.",
    ],
    related: ["idempotent-consumers", "projections-read-models", "snapshots-checkpoints"],
    keywords: ["durable subscription event store", "consumer checkpoint", "event stream consumer"],
    references: [MICROSOFT_EVENT_SOURCING],
  },
] as const;

export const eventSourcingPatternBySlug = new Map(
  eventSourcingPatterns.map((pattern) => [pattern.slug, pattern])
);

export function getEventSourcingPattern(slug: string) {
  return eventSourcingPatternBySlug.get(slug);
}

export function getPatternWordCount(pattern: EventSourcingPattern) {
  const copy = [
    pattern.directAnswer,
    ...pattern.problem,
    ...pattern.decisions.flatMap((decision) => [decision.title, decision.detail]),
    ...pattern.implementation,
    pattern.exampleTitle,
    pattern.example,
    ...pattern.failureModes.flatMap((failure) => [failure.symptom, failure.fix]),
    ...pattern.checklist,
  ].join(" ");

  return copy.trim().split(/\s+/).length;
}
