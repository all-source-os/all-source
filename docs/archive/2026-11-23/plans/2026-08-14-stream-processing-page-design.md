# Stream-processing page redesign

## Goal

Explain AllSource Core stream processing accurately enough that a developer can decide whether to use it, understand current limits, and construct a valid pipeline definition.

## Chosen direction: pipeline rail

Use an ordered operator rail as the page signature. Source event, operators, result, and boundary appear in one trace, making operator order and in-process execution visible without animation or client JavaScript.

Alternatives considered:

- A simulated pipeline editor would feel more interactive but imply a hosted editor that does not exist.
- A conventional architecture diagram would be accurate but make the operator model harder to scan.
- The pipeline rail demonstrates the real execution model while remaining static, fast, and indexable.

## Token system

- Abyss `#0E1A2A`: existing dark site background.
- Panel `#122235`: quiet execution surfaces.
- Trace cyan `#12B7E8`: active operator path.
- Signal green `#20C997`: supported result/status.
- Text `#F2F5F7`: primary copy.
- Muted `#9CA3AF`: evidence and boundary notes.
- Display/body: existing site sans stack, with tighter display tracking.
- Utility/data: existing monospace stack for event types, JSON, methods, and status labels.

## Layout

```text
┌──────────────────────────────┬─────────────────────────────┐
│ Thesis + exact scope         │ source_event_types          │
│ Core API / source links      │ → filter → window → reduce  │
│                              │ → in-process result         │
└──────────────────────────────┴─────────────────────────────┘

  execution contract ─ where / when / what survives

  ordered operator ledger ─ supported / limited status

  valid PipelineConfig JSON ─ lifecycle endpoint table

  fit guide ─ Core / Query Service / Kafka-Flink boundary

  direct-answer FAQ ─ source + docs calls to action
```

## Accuracy boundaries

- Core applies matching enabled pipelines during event ingestion.
- Filter, Map, Reduce, Window, and Branch execute in Core today.
- Enrich currently produces placeholder field values; external lookups are not wired.
- `output` is configuration metadata; Core does not automatically persist or publish the computed result.
- Pipeline definitions and state are in-memory and must be registered again after restart.
- Query Service WebSocket feeds broadcast accepted events, not Core pipeline results.
- Hosted gateway does not expose pipeline lifecycle routes; page leads with self-hosted Core rather than hosted trial.
- 469K events/sec remains a Core batch-ingest reference, not pipeline throughput.

## Acceptance criteria

- Page is a server component with no decorative animation dependency.
- Hero explains runtime, available operators, and current result boundary.
- JSON example matches current Rust `PipelineConfig` serde shape.
- Operator and lifecycle sections distinguish shipped, limited, and separate capabilities.
- FAQ and breadcrumb structured data match visible answers.
- Desktop and mobile have no horizontal overflow, broken links, or console/server errors.
