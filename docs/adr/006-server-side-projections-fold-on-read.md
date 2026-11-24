# ADR-006: Server-Side Projections with Fold-on-Read

**Status:** Accepted; partially superseded by [ADR-021](021-views-folded-by-query-service.md) Decision 7 (route retired)
**Date:** 2026-02-17
**Release:** v0.10.5

## Context

Clients querying AllSource needed to fold raw events into projected state client-side. This meant:
1. Transferring all raw events over the network
2. Every client implementing its own fold logic
3. No caching of computed state across clients

## Decision

Add server-side projections to the Query Service with two modes:

### 1. Fold-on-Read (`POST /api/query/projected`, removed from Query Service)
- Snapshot-aware: checks Core for existing snapshots before folding
- When a snapshot exists, uses it as initial accumulator and only folds events after the snapshot timestamp
- Reduces fold cost from N events to a delta
- Lazy snapshot creation: when `events_after_snapshot > threshold`, creates a new snapshot asynchronously

### 2. Continuous Projections (ProjectionServer + DynamicSupervisor)
- PubSub subscription for real-time materialized read models
- ETS-backed state with fold-on-read fallback for cold reads
- Supervised per-projection GenServer processes

### Architecture
- **Projection behaviour**: `init/1`, `handle_event/2`, `get_state/1` callbacks
- **Compile-time registry**: Maps projection names to modules
- **FoldPipeline**: Snapshot → delta fetch → fold → optional snapshot write
- **5 built-in projections**: IndexState, TradeState, PortfolioState, SagaState, EntitySnapshots

## Consequences

### Positive
- Clients receive projected state directly — no fold logic needed
- Snapshot-aware folding reduces computation for long-lived entities
- Real-time materialized views via PubSub subscription
- Projection modules are pluggable via behaviour

### Negative
- Query Service is no longer fully stateless (ETS cache for projection state)
- Projection consistency depends on event ordering (mitigated by timestamp sorting)
- Snapshot writes add async I/O to the read path
