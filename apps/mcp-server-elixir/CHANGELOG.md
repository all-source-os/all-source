# Changelog

All notable changes to the MCP Server will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- ES256 client key authentication against the hosted gateway. Set
  `ALLSOURCE_CLIENT_SIGNING_KEY` (inline PEM or path), `ALLSOURCE_CLIENT_ID`,
  `ALLSOURCE_TENANT_ID` and `ALLSOURCE_CLIENT_KID`, and optionally
  `ALLSOURCE_ASSERTION_AUDIENCE` (default `https://api.all-source.xyz`). Each
  request then carries a 60-second assertion, reused until 15 seconds before it
  expires, and the static `CORE_API_KEY` is not sent. The variable names match
  `allsource-mcp`.
- Boot fails on a missing identity variable, a key that is not P-256 PKCS#8, or
  a kid that is not the key's RFC 7638 thumbprint.

### Changed
- HTTP retries and WebSocket reconnects ask for the credential again instead of
  reusing the header captured at start.

### Fixed
- README listed `MCP_READ_ONLY`; the variable read is `ALLSOURCE_READ_ONLY`.

## [0.9.0] - 2026-02-10

### Added

#### Event Management Tools (v2.0 Phase 2 - Partial)

**Lifecycle Operations (8 tools)**
- `delete_events` - Soft delete with complete audit trail (GDPR/CCPA compliant)
- `archive_events` - Move events to cold storage for cost optimization
- `restore_events` - Restore deleted or archived events back to active state
- `export_events` - Export to JSON/JSONL/CSV/Parquet formats with compression
- `import_events` - Bulk import with validation, deduplication, and entity mapping
- `clone_entity` - Deep copy entity with all events (for testing/templates)
- `merge_entities` - Combine event streams from multiple entities
- `split_entity` - Partition entity event stream by criteria

**All Event Management Tools Include:**
- Dry-run preview mode for safe operations
- Complete audit trail for compliance
- Rich tool descriptions with AI agent guidance
- Decision trees and common patterns
- Performance tips and best practices

### Changed
- Tool count increased from 19 to 27 tools
- Updated all tool count assertions in test suite
- Enhanced tool descriptions with dry_run/preview mentions

### Technical Details
- 82 new tests for event management tools
- All 179 protocol tests passing
- Code formatted and Credo compliant
- ~800 LOC added for event management handlers

---

## [0.2.0] - 2026-02-03

### Added

#### AI-Native Enhancements - Complete

**Enhanced Tool Descriptions (US-013)**
- Rich tool descriptions with agent guidance
- Best practices and common patterns embedded in descriptions
- Performance tips for each tool
- Decision trees for tool selection
- Context-aware recommendations

**Query Advice Tool (US-014)**
- `get_query_advice` tool implementation
- Use-case specific recommendations
- Query pattern suggestions based on data shape
- Performance optimization tips
- Index usage recommendations

**Conversation Context Manager (US-015)**
- `ConversationContext` for multi-turn interactions
- Session-based query refinement
- Iterative query composition
- Context preservation across tool calls
- Memory of previous queries and results

**Quick Exploration Tools (US-016)**
- `sample_events` for fast data exploration
- `quick_stats` for rapid statistics
- Stratified sampling options
- Random and time-based sampling modes
- Configurable sample sizes

#### Native Search Integration

**Semantic Search Tools**
- `semantic_search_events` - Vector similarity search
- `hybrid_search` - Combined vector + keyword search
- Configurable similarity thresholds
- Metadata filtering support

#### Real-Time Integration

**Core WebSocket Client**
- `CoreWebSocketClient` for real-time Core events
- Auto-reconnect with exponential backoff
- PubSub integration for event distribution
- Connection health monitoring

**Broadway Pipeline**
- `CoreProducer` for Broadway integration
- `EventPipeline` for stream processing
- Cursor tracking and persistence
- Backpressure handling

**Projection Sync**
- `ProjectionSync` GenServer
- ETS cache for local reads
- Automatic state restoration

### Changed
- Updated all tool descriptions with AI-native guidance
- Enhanced error messages for better agent understanding
- Improved response formatting for LLM consumption

### Technical Details
- 4 new AI-native tools
- 2 new search tools
- Real-time event streaming infrastructure
- ~2500 LOC added

---

## [0.1.0] - 2025-12-01

### Added

#### MCP v2.0 Phase 1 - Complete

**Advanced Query Tools**
- `advanced_query` - Complex queries with aggregations
- `time_series_analysis` - Trend analysis over time

**Analytics Tools**
- `funnel_analysis` - Conversion tracking
- `detect_anomalies` - Real-time anomaly detection

**Projection Tools**
- `create_projection` - Materialized views
- `get_projection_state` - Query projections
- `list_projections` - List all projections

**Pipeline Tools**
- `execute_pipeline` - Event processing
- `replay_events` - Event replay
- `validate_events` - Data validation

**Policy Tools**
- `create_policy` - Policy creation
- `evaluate_policy` - Policy evaluation
- `list_policies` - List policies

### Technical Details
- 13 advanced tools implemented
- ~900 LOC
- Full MCP protocol compliance

---

## Version History

| Version | Date | Status | Highlights |
|---------|------|--------|------------|
| [0.9.0] | 2026-02-10 | Current | Event Management Tools (8 new), 27 total tools |
| [0.2.0] | 2026-02-03 | Stable | AI-Native, Search, Real-Time Integration |
| [0.1.0] | 2025-12-01 | Stable | MCP v2.0 Phase 1, 13 tools |
