---
title: "MCP Server"
status: CURRENT
version: 0.10.3
last_updated: 2026-02-15
category: service
port: 4000
technology: Elixir
---

# AllSource MCP Server

> Turn your event store into an AI-queryable knowledge base. 61 tools for natural language event exploration, time-travel analysis, and real-time insights — all via Model Context Protocol.

[![CI](https://github.com/all-source-os/all-source/actions/workflows/ci.yml/badge.svg)](https://github.com/all-source-os/all-source/actions/workflows/ci.yml)
[![Elixir](https://img.shields.io/badge/elixir-1.17%2B-purple.svg)](https://elixir-lang.org/)
[![Tools](https://img.shields.io/badge/MCP%20tools-61-orange.svg)]()
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](../../LICENSE)

**Current Version**: v0.10.3

## What Can It Do?

Connect Claude Desktop (or any MCP client) to your AllSource event store and ask questions like:

- *"What changed for user-123 yesterday?"*
- *"Show me the signup-to-purchase funnel for the last 30 days"*
- *"Which entities have anomalous activity patterns?"*
- *"Forecast event volume for next week"*
- *"Compare user-123 and user-456 side by side"*

The MCP server translates natural language into precise event store queries, returns results in a token-efficient format, and maintains conversation context across multi-turn interactions.

## Quick Start

For the restricted, consent-bound customer review profile, use
[Local customer review connection](CUSTOMER_CONNECTIONS.md). Its OS-owner-checked
private file is separate from the general event-store connection below.

### 1. Start AllSource Services

```bash
# Core Event Store
cd apps/core && cargo run --release

# Control Plane (optional, needed for tenant management)
cd apps/control-plane && go run .
```

### 2. Start MCP Server

```bash
cd apps/mcp-server-elixir
mix deps.get
mix run --no-halt
```

### 3. Connect Claude Desktop

Add to your Claude Desktop configuration (`~/Library/Application Support/Claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "allsource": {
      "command": "mix",
      "args": ["run", "--no-halt"],
      "cwd": "/path/to/all-source/apps/mcp-server-elixir"
    }
  }
}
```

Or with a compiled release:

```json
{
  "mcpServers": {
    "allsource": {
      "command": "/path/to/apps/mcp-server-elixir/_build/dev/rel/mcp_server_elixir/bin/mcp_server_elixir",
      "args": ["start"]
    }
  }
}
```

## 61 Tools Across 11 Categories

Tools follow a guided workflow from discovery to deep analysis. Each phase builds on the previous one.

### Phase 1: Discover — *What data exists? Start here.*
| Tool | Description |
|------|-------------|
| `quick_stats` | Rapid approximate statistics for fast orientation |
| `sample_events` | Stratified sampling for data exploration |
| `get_stats` | Detailed store statistics |
| `get_cluster_status` | Cluster health and node status |
| `list_schemas` | Available event schemas |

### Phase 2: Search — *Find relevant data*
| Tool | Description |
|------|-------------|
| `query_events` | Flexible event queries with filters, time ranges, pagination |
| `semantic_search_events` | Vector similarity search across event payloads |
| `hybrid_search` | Combined vector + keyword search |
| `get_query_advice` | AI-guided query recommendations for your use case |

### Phase 3: Drill Down — *Deep analysis*
| Tool | Description |
|------|-------------|
| `get_snapshot` | Fast current state retrieval |
| `reconstruct_state` | Time-travel state reconstruction at any timestamp |
| `analyze_changes` | Temporal diff between two points in time |
| `event_timeline` | Chronological event timeline for an entity |
| `explain_entity` | Comprehensive entity analysis with context |
| `find_patterns` | Event pattern detection and anomalies |
| `compare_entities` | Side-by-side multi-entity comparison |

### Phase 4: Context — *Multi-turn conversations*
| Tool | Description |
|------|-------------|
| `start_session` | Begin a conversation session with context |
| `refine_query` | Iteratively refine queries based on results |
| `get_session_context` | Retrieve accumulated session context |

### Phase 5: Mutate — *Write operations*
| Tool | Description |
|------|-------------|
| `ingest_event` | Create new events |
| `delete_events` | Soft delete with audit trail (GDPR/CCPA compliant) |
| `archive_events` | Move to cold storage with retention policies |
| `import_events` | Bulk import with validation and deduplication |
| `clone_entity` | Deep copy entity with all events |
| `merge_entities` | Combine event streams from multiple entities |
| `split_entity` | Partition entity event stream by criteria |

### Phase 6: Event Lifecycle — *Always available*
| Tool | Description |
|------|-------------|
| `restore_events` | Restore deleted or archived events |
| `export_events` | Export to JSON, JSONL, CSV, or Parquet |

### Phase 7: Operations — *System inspection & admin*

**Read (always available):**

| Tool | Description |
|------|-------------|
| `storage_stats` | Storage utilization and file details |
| `partition_info` | Partition distribution and balance |
| `wal_status` | Write-Ahead Log health and segment info |
| `backup_list` | Available backups |
| `health_deep` | Deep health check across all subsystems |
| `performance_report` | Performance metrics and bottleneck detection |
| `audit_log` | Audit trail inspection |

**Write (gated by read-only mode):**

| Tool | Description |
|------|-------------|
| `compact_storage` | Trigger storage compaction |
| `backup_create` | Create point-in-time backup |
| `backup_restore` | Restore from backup |

### Phase 8: Tenants — *Multi-tenancy management*
| Tool | Description |
|------|-------------|
| `tenant_create` | Create new tenant with quotas |
| `tenant_update` | Update tenant configuration |
| `tenant_usage` | Usage statistics and billing data |
| `tenant_quotas` | Manage quotas and rate limits |
| `tenant_suspend` | Suspend/reactivate tenant |
| `tenant_export` | Export all tenant data |

### Phase 9: Schema & Validation — *Event type governance*
| Tool | Description |
|------|-------------|
| `register_schema` | Register event schema with versioning (Core API) |
| `validate_schema` | Validate events against schema (Core API) |
| `migrate_schema` | Register new schema version; dry-run diff is client-side |
| `infer_schema` | Fetch events from Core, infer JSON Schema client-side |
| `schema_diff` | Fetch schema versions from Core, compute diff client-side |

> **Architecture note:** `migrate_schema`, `infer_schema`, and `schema_diff` use Core's schema registry and event query APIs for data retrieval, then perform lightweight computation (structural diffing, type inference) client-side in the MCP server. This is intentional — these operations are simple map comparisons and type detection that don't warrant dedicated Core endpoints. `register_schema`, `validate_schema`, and `list_schemas` are fully backed by Core's schema registry API.

### Phase 10: Analytics — *Business intelligence*
| Tool | Description |
|------|-------------|
| `cohort_analysis` | Analyze user cohorts over time |
| `correlation_analysis` | Find correlations between event types |
| `forecast_events` | Forecast future event volumes |
| `segment_analysis` | User segment analysis |
| `path_analysis` | User journey and conversion path analysis |
| `attribution_analysis` | Attribution modeling for conversions |
| `churn_prediction` | Predict churn risk |
| `ltv_calculation` | Calculate customer lifetime value |

### Phase 11: Developer Experience — *Productivity & testing*
| Tool | Description |
|------|-------------|
| `generate_client` | Generate client code for your language |
| `mock_events` | Generate realistic mock events for testing |
| `debug_query` | Debug query performance with explain plans |
| `benchmark_query` | Benchmark query execution |

## Use Case: E-Commerce Order Investigation

This walkthrough shows how an AI assistant uses the MCP server to investigate a customer support issue end-to-end.

**Scenario:** Customer reports they were charged but never received a confirmation email.

### Step 1: Discover — Orient yourself

```
User: "Customer user-4821 says they were charged but got no confirmation. What happened?"

AI calls: quick_stats
→ 142,387 events across 8,291 entities — system is healthy

AI calls: get_cluster_status
→ Single-node mode, healthy, replication: null
```

### Step 2: Search — Find the customer's events

```
AI calls: query_events
  entity_id: "user-4821"
  limit: 50

→ 23 events found:
  - user.created (2026-01-15)
  - cart.item_added (x3)
  - checkout.started
  - payment.initiated
  - payment.succeeded     ← charge went through
  - order.created
  - (no email.confirmation.sent event!)
```

### Step 3: Drill down — Reconstruct state at the time

```
AI calls: event_timeline
  entity_id: "user-4821"
  since: "2026-02-15T00:00:00Z"

→ Timeline shows payment.succeeded at 14:32:07Z
  followed by order.created at 14:32:08Z
  then... nothing. No email event.

AI calls: analyze_changes
  entity_id: "user-4821"
  from: "2026-02-15T14:32:00Z"
  to: "2026-02-15T14:33:00Z"

→ State changed from {order_status: "pending"} to {order_status: "created"}
  but email_sent remained false
```

### Step 4: Find patterns — Is this systemic?

```
AI calls: find_patterns
  event_type: "payment.succeeded"
  since: "2026-02-15T00:00:00Z"

→ Pattern detected: 47 payment.succeeded events today,
  but only 41 email.confirmation.sent events.
  6 orders missing confirmation emails — all between 14:30-14:45Z.

AI calls: query_events
  event_type: "email.service.error"
  since: "2026-02-15T14:00:00Z"

→ Found: email.service.error at 14:29:55Z
  payload: {provider: "sendgrid", error: "rate_limit_exceeded", retry_after: 900}
```

### Step 5: Resolution

```
AI: "Found the issue. The email service (SendGrid) hit a rate limit at 14:29:55Z,
which caused 6 confirmation emails to fail between 14:30-14:45Z. User-4821's
order was created successfully and payment went through — only the confirmation
email was dropped. Recommend: retry the 6 failed emails and check the email
service error handling for rate limit backoff."
```

### What the MCP server provided

- **5 tool calls** to go from vague complaint to root cause
- **No SQL, no dashboard clicking, no log grep** — natural language throughout
- **Time-travel analysis** to reconstruct exact system state at failure time
- **Pattern detection** to identify it's a systemic issue, not a one-off

## Token-Efficient Responses

All tools return responses in **TOON format** by default — approximately **50% fewer tokens** than JSON for tabular data. This directly reduces LLM API costs and speeds up processing.

| Format | Tokens (typical) | Use Case |
|--------|:----------------:|----------|
| TOON (default) | ~500 | Tabular data, lists, summaries |
| JSON | ~1,000 | Complex nested structures |

Override per-request with `format: "json"` or `format: "toon"`.

## Access Control

Tools are gated by configuration to prevent unintended modifications:

| Gate | Tools Affected | Default |
|------|---------------|---------|
| **Read-only mode** | All write operations (ingest, delete, archive, import, clone, merge, split, compact, backup create/restore) | Off |
| **Control Plane** | All tenant management tools | Enabled when Control Plane URL is configured |

## Architecture

```
┌─────────────────────────────────────────┐
│         Claude Desktop / LLM            │
│                                         │
│  "What changed for user-123 yesterday?" │
└────────────────┬────────────────────────┘
                 │ JSON-RPC 2.0 over stdio
                 ▼
┌────────────────────────────────────────────┐
│      MCP Server (Elixir/OTP)              │
│                                            │
│  • JSON-RPC Handler (pattern matching)    │
│  • Tool Router (61 tools)                 │
│  • Session Manager (multi-turn context)   │
│  • TOON Formatter (token optimization)    │
│  • OTP Supervision (fault tolerance)      │
└────────────────┬──────────────────────────┘
                 │ Tesla HTTP Client
      ┌──────────┴──────────┐
      ▼                     ▼
┌──────────────┐   ┌─────────────────┐
│  Core API    │   │  Control Plane  │
│  (Rust)      │   │  (Go)           │
│  :3900       │   │  :3901          │
└──────────────┘   └─────────────────┘
```

## Performance

| Metric | Value |
|--------|-------|
| Tool call latency | <100ms (local network) |
| Query execution | <10ms (indexed queries) |
| Time-travel reconstruction | <50ms (typical entity) |
| Pattern analysis | <500ms (thousands of events) |
| Token reduction | ~50% fewer tokens vs JSON |

## Development

```bash
# Install dependencies
mix deps.get

# Run tests
mix test

# Code formatting
mix format

# Static analysis
mix credo
mix dialyzer

# Build release
MIX_ENV=prod mix release
```

## Configuration

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `ALLSOURCE_CORE_URL` | `http://localhost:3900` | Core event store URL |
| `ALLSOURCE_CONTROL_URL` | `http://localhost:3901` | Control Plane URL (enables tenant tools) |
| `MCP_READ_ONLY` | `false` | Disable all write operations |

## Troubleshooting

### MCP Server Won't Start
```bash
elixir --version           # Check Elixir is installed
curl localhost:3900/health # Check Core is running
curl localhost:3901/health # Check Control Plane is running
```

### Claude Desktop Not Connecting
- Restart Claude Desktop completely
- Check config path is absolute
- Verify MCP logs in `~/Library/Logs/Claude/`

### Tool Calls Failing
- Use ISO timestamps (`YYYY-MM-DDTHH:mm:ssZ`)
- Check entity exists before querying
- Test the underlying API directly with curl

### Tools Vanish Mid-Session / Connection Flaps
stdout is the JSON-RPC channel — anything else written there is protocol garbage
to the client, and a crash in the server process drops the stream outright. Two
causes were fixed in #229: Logger defaulted to `:standard_io` (now
`:standard_error`, set in `config/config.exs`), and an exception while handling a
request took the server GenServer down with it. Request handling is now wrapped
at the seam (`Server.guarded_process/2`, around the whole of `process_request/2`)
so JSON-RPC parsing, argument shaping, tool dispatch and response encoding all
degrade to a `-32603` error instead of a process exit — including
client-controlled shape errors such as `"arguments"` arriving as a JSON string
rather than an object. That matters because the supervisor's default
`max_restarts: 3` in 5s means four fatal requests in five seconds would take the
whole application down, not just the session.

A third cause was found later, on the *quiet* side of the same stream: the
server answered JSON-RPC **notifications**. `JsonRpc.normalize_request/1` always
puts an `:id` key in the request map (nil when the client sent none), so the
"unknown notification — no response needed" clause in `process_request/2` was
unreachable and every notification drew a frame with `"id": null` onto stdout.
MCP clients send `notifications/cancelled` and `notifications/progress` as a
matter of course, so cancelling one tool call was enough: the client gets a
response it cannot correlate, which the MCP SDK reports as "a response for an
unknown message ID". Notifications are now silent end to end (success, tool
error and crash paths alike) — see `Server.respond/2`.

If it recurs, capture the connector's **stderr** — that is where the server
logs — and check that no library you added writes to stdout.

## Resources

- [Model Context Protocol Spec](https://spec.modelcontextprotocol.io/)
- [AllSource Documentation](../../README.md)
- [Core API Reference](../core/README.md)

## License

[MIT](../../LICENSE)

---

<div align="center">

**AllSource MCP Server** — 61 AI-native tools for your event store

Query, analyze, and manage temporal data through natural language

[GitHub](https://github.com/all-source-os/all-source) | [AllSource Core](../core/README.md)

</div>
