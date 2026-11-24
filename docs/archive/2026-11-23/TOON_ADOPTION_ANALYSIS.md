---
title: "TOON Adoption Analysis"
status: CURRENT
last_updated: 2026-02-02
category: reference
---

# TOON Adoption Analysis

## Overview

[TOON (Token-Oriented Object Notation)](https://github.com/toon-format/toon) is a compact format designed for LLM prompts that uses approximately **50% fewer tokens** than JSON. This analysis identifies opportunities to adopt TOON across the AllSource stack to reduce token costs for AI agents.

## Why TOON?

- **Token Efficiency**: ~50% fewer tokens than JSON for tabular data
- **LLM Optimized**: Designed specifically for AI model consumption
- **Self-Documenting**: Clear structure with length markers `[N]` and field headers `{field1,field2}`
- **Elixir Support**: Community implementation available (`toon_ex`)

## Adoption Opportunities

### 🎯 Priority 1: MCP Server Responses (Highest Impact)

**Current State**: MCP server sends JSON-formatted responses to Claude Desktop
**Impact**: **Very High** - This is the primary LLM interaction point

**Locations**:
- `apps/mcp-server-elixir/lib/mcp_server_elixir/protocol/mcp_tools.ex`
  - All tool handlers return JSON via `Jason.encode!(data, pretty: true)`
  - ~11 tools sending responses

**Example Transformation**:

**Current (JSON)**:
```json
{
  "events": [
    {"id": "evt-1", "event_type": "user.created", "timestamp": "2025-01-15T10:30:00Z"},
    {"id": "evt-2", "event_type": "user.updated", "timestamp": "2025-01-15T11:00:00Z"}
  ],
  "count": 2
}
```

**TOON Alternative**:
```toon
events[2]{id,event_type,timestamp}:
  evt-1,user.created,2025-01-15T10:30:00Z
  evt-2,user.updated,2025-01-15T11:00:00Z
count: 2
```

**Token Savings**: ~50% reduction for event lists and tabular data

**Implementation Plan**:
1. Add `toon_ex` dependency to `mix.exs`
2. Create `McpServerElixir.Protocol.ToonEncoder` module
3. Update tool handlers to format responses as TOON when appropriate
4. Keep JSON as fallback for non-tabular data

---

### 🎯 Priority 2: Event Query Responses

**Current State**: Rust Core API returns JSON-formatted event lists
**Impact**: **High** - Event lists are often uniform arrays of objects

**Locations**:
- `apps/core/src/api.rs` - `query_events()` returns `Json<QueryEventsResponse>`
- `apps/core/src/application/dto/event_dto.rs` - EventDto serialization

**Opportunity**:
- Add optional `?format=toon` query parameter to API endpoints
- Convert `EventDto` vectors to TOON format
- Maintain JSON as default for API compatibility

**Implementation**:
```rust
// Add toon_format crate
// Implement EventDto::to_toon() method
// Add format parameter to QueryEventsRequest
```

---

### 🎯 Priority 3: State Reconstruction Responses

**Current State**: `reconstruct_state()` returns JSON
**Impact**: **Medium** - State objects are often non-uniform, but could benefit from TOON for nested arrays

**Locations**:
- `apps/core/src/api.rs` - `get_entity_state()` returns `Json<serde_json::Value>`
- `apps/core/src/store.rs` - State reconstruction logic

**Opportunity**:
- When state contains arrays of uniform objects, convert to TOON
- Keep JSON for complex nested structures
- Add format detection logic

---

### 🎯 Priority 4: Query Service API Responses

**Current State**: Elixir Query Service returns JSON via Phoenix
**Impact**: **Medium** - API compatibility important, but could offer TOON as alternative

**Locations**:
- `apps/query-service/lib/query_service_ex_web/controllers/*.ex`
  - All controllers use `json(conn, data)`

**Opportunity**:
- Add `Accept: application/toon` header support
- Create `toon(conn, data)` Phoenix helper
- Maintain JSON as default

---

### 🎯 Priority 5: Event Payloads (Storage)

**Current State**: Event payloads stored as `serde_json::Value`
**Impact**: **Low** - Storage format, but could save space

**Locations**:
- `apps/core/src/domain/entities/event.rs` - `payload: serde_json::Value`

**Consideration**:
- TOON is optimized for LLM consumption, not storage
- Internal storage should remain JSON for compatibility
- Convert to TOON only when sending to LLMs

---

## Implementation Strategy

### Phase 1: MCP Server (Immediate Impact)

**Goal**: Convert MCP tool responses to TOON format

**Steps**:
1. Add `toon_ex` to `apps/mcp-server-elixir/mix.exs`
2. Create encoder module:
   ```elixir
   defmodule McpServerElixir.Protocol.ToonEncoder do
     def encode(data), do: ToonEx.encode(data)
     
     def format_response(data) do
       case detect_tabular_structure(data) do
         {:tabular, toon_data} -> ToonEx.encode(toon_data)
         {:mixed, _} -> Jason.encode!(data, pretty: true)  # Fallback to JSON
       end
     end
   end
   ```
3. Update tool handlers to use TOON for tabular responses
4. Keep JSON for complex nested structures

**Files to Modify**:
- `apps/mcp-server-elixir/mix.exs` - Add dependency
- `apps/mcp-server-elixir/lib/mcp_server_elixir/protocol/mcp_tools.ex` - Update handlers

**Estimated Savings**: 40-50% token reduction for event lists

---

### Phase 2: Rust Core API (Optional Format)

**Goal**: Add TOON as optional response format

**Steps**:
1. Add `toon_format` crate to `Cargo.toml`
2. Implement `ToToon` trait for DTOs
3. Add `format` query parameter support
4. Update API handlers to accept `?format=toon`

**Files to Modify**:
- `apps/core/Cargo.toml` - Add dependency
- `apps/core/src/api.rs` - Add format parameter handling
- `apps/core/src/application/dto/event_dto.rs` - Implement ToToon trait

**Challenges**:
- Need Rust TOON implementation (may need to implement or wait for official)
- Maintain backward compatibility

---

### Phase 3: Query Service (Header-Based)

**Goal**: Support TOON via Accept header

**Steps**:
1. Add `toon_ex` to `mix.exs`
2. Create Phoenix plug for content negotiation
3. Add `toon(conn, data)` helper
4. Update controllers to support both formats

---

## Format Detection Logic

TOON works best for:
- ✅ Uniform arrays of objects (same fields, primitive values)
- ✅ Event lists
- ✅ Timelines
- ✅ Comparison tables

JSON is better for:
- ❌ Non-uniform data structures
- ❌ Deeply nested objects
- ❌ Objects with varying field sets
- ❌ Mixed types in arrays

**Detection Algorithm**:
```elixir
def detect_tabular_structure(data) do
  case data do
    %{"events" => events} when is_list(events) ->
      if uniform_event_list?(events), do: {:tabular, data}, else: {:mixed, data}
    
    %{"items" => items} when is_list(items) ->
      if uniform_items?(items), do: {:tabular, data}, else: {:mixed, data}
    
    _ -> {:mixed, data}
  end
end

defp uniform_event_list?(events) when length(events) > 0 do
  first_keys = events |> List.first() |> Map.keys() |> Enum.sort()
  
  Enum.all?(events, fn event ->
    event |> Map.keys() |> Enum.sort() == first_keys
  end)
end
```

---

## Token Savings Estimates

### MCP Server Tool Responses

| Tool | Current (JSON tokens) | TOON (estimated tokens) | Savings |
|------|---------------------|------------------------|---------|
| `query_events` (10 events) | ~200 | ~100 | 50% |
| `event_timeline` (20 events) | ~400 | ~200 | 50% |
| `compare_entities` (5 entities) | ~150 | ~75 | 50% |
| `find_patterns` | ~100 | ~60 | 40% |
| `reconstruct_state` | ~80 | ~80 | 0% (non-tabular) |

**Average Savings**: ~40-45% for tabular data

### API Responses

| Endpoint | Impact | Savings |
|----------|--------|---------|
| `/api/v1/events/query` | High | 40-50% |
| `/api/v1/entities/:id/state` | Low | 0-10% |
| `/api/v1/stats` | Medium | 20-30% |

---

## Dependencies Required

### Elixir
```elixir
# apps/mcp-server-elixir/mix.exs
{:toon_ex, "~> 0.1"}  # Community implementation
```

### Rust
```toml
# apps/core/Cargo.toml
# Note: Official Rust implementation in development
# May need to implement or wait for official release
```

### Go
```go
// apps/control-plane/go.mod
// May need to implement or find community library
```

---

## Migration Considerations

### Backward Compatibility
- ✅ Keep JSON as default format
- ✅ Add TOON as opt-in feature
- ✅ APIs maintain JSON responses unless explicitly requested

### Agent Adaptation
- TOON is self-documenting - LLMs adapt quickly
- Can provide examples in prompts
- Format is deterministic

### Testing
- Add tests for TOON encoding/decoding
- Verify token counts match expectations
- Test format detection logic

---

## Recommended Implementation Order

1. **Phase 1: MCP Server** (Week 1)
   - Highest impact, lowest risk
   - Single service to modify
   - Immediate token savings

2. **Phase 2: Rust Core API** (Week 2-3)
   - Optional format support
   - Maintain backward compatibility
   - Wait for official Rust implementation or implement

3. **Phase 3: Query Service** (Week 4)
   - Header-based negotiation
   - Lower priority

---

## Expected Outcomes

### Token Reduction
- **MCP Server**: 40-50% reduction for event-heavy responses
- **Average**: 30-40% reduction across all LLM interactions
- **Cost Savings**: Significant reduction in API costs for Claude/OpenAI

### Benefits
- ✅ Lower token costs
- ✅ Faster LLM processing (fewer tokens to process)
- ✅ Better context window utilization
- ✅ More efficient prompt engineering

### Risks
- ⚠️ New format requires testing
- ⚠️ Need to ensure LLM compatibility
- ⚠️ Maintenance of format detection logic

---

## Next Steps

1. **Research**: Verify `toon_ex` Elixir implementation quality
2. **Prototype**: Implement TOON encoding in MCP server for one tool
3. **Benchmark**: Measure actual token savings vs estimates
4. **Decide**: Proceed with full implementation based on results

---

## References

- [TOON Format Specification](https://github.com/toon-format/toon)
- [TOON Elixir Implementation](https://hex.pm/packages/toon_ex) (community)
- [TOON Benchmarks](https://github.com/toon-format/toon/tree/main/benchmarks)

