---
title: "AllSource MCP Server Enhancement Plan"
status: SUPERSEDED
last_updated: 2026-03-01
category: reference
---

# AllSource MCP Server Enhancement Plan

> **Note (2026-03-01):** This proposal is **superseded**. It was written on 2025-10-24 when the MCP server was a TypeScript project in `packages/mcp-server` and referenced a "Clojure Query DSL" and "Clojure analytics engine" that no longer exist. The MCP server is now an Elixir application at `apps/mcp-server-elixir/` (port 4000). A separate Rust MCP Docker binary exists at port 3904 (archived in the wallet project). The tool categories and enhancement ideas may still be useful as inspiration, but the implementation approach, code examples, and architecture references are outdated.

**Date**: 2025-10-24
**Status**: Superseded (see note above)
**Goal**: Expose all new AllSource capabilities through the MCP interface

---

## 🎯 Current State vs. Enhanced State

### Current MCP Tools (11 basic tools)
1. ✅ query_events - Basic event querying
2. ✅ reconstruct_state - State reconstruction
3. ✅ get_snapshot - Fast snapshot retrieval
4. ✅ analyze_changes - Temporal diff
5. ✅ find_patterns - Simple pattern detection
6. ✅ compare_entities - Entity comparison
7. ✅ event_timeline - Event chronology
8. ✅ explain_entity - Entity explanation
9. ✅ ingest_event - Event ingestion
10. ✅ get_stats - Statistics
11. ✅ get_cluster_status - Cluster health

**Total**: 11 tools, ~620 LOC

### Proposed Enhanced MCP Tools (40+ tools)

#### **Category 1: Enhanced Query & Analytics** (15 tools)
1. ✨ `advanced_query` - Leverage Clojure Query DSL
2. ✨ `time_series_analysis` - Time-series aggregations
3. ✨ `funnel_analysis` - Conversion funnel tracking
4. ✨ `cohort_analysis` - Cohort retention analysis
5. ✨ `trend_analysis` - Trend detection with forecasting
6. ✨ `detect_anomalies` - Anomaly detection (Z-score, IQR, MAD)
7. ✨ `compute_aggregation` - Custom aggregations (sum, avg, percentiles)
8. ✨ `event_correlation` - Find correlated events
9. ✨ `impact_analysis` - Analyze event impact on state
10. ✨ `event_distribution` - Event distribution statistics
11. ✨ `session_analysis` - User session analysis
12. ✨ `retention_metrics` - Calculate retention metrics
13. ✨ `activity_patterns` - Detect activity patterns
14. ✨ `user_journey` - Trace user journeys
15. ✨ `data_quality_report` - Data quality metrics

#### **Category 2: Projection Management** (8 tools)
16. ✨ `create_projection` - Define new projection
17. ✨ `start_projection` - Start projection execution
18. ✨ `stop_projection` - Stop projection
19. ✨ `reload_projection` - Hot-reload projection
20. ✨ `get_projection_state` - Query projection state
21. ✨ `get_projection_status` - Get projection health
22. ✨ `list_projections` - List all projections
23. ✨ `snapshot_projection` - Create projection snapshot

#### **Category 3: Pipeline Management** (8 tools)
24. ✨ `create_pipeline` - Define event processing pipeline
25. ✨ `execute_pipeline` - Run pipeline on events
26. ✨ `pipeline_metrics` - Get pipeline performance metrics
27. ✨ `list_pipelines` - List all pipelines
28. ✨ `validate_pipeline` - Validate pipeline definition
29. ✨ `test_pipeline` - Test pipeline with sample data
30. ✨ `optimize_pipeline` - Get optimization suggestions
31. ✨ `pipeline_health` - Pipeline execution health

#### **Category 4: Integration Tools** (7 tools)
32. ✨ `replay_events` - Replay events to rebuild state
33. ✨ `validate_events` - Validate events against schema
34. ✨ `migrate_schema` - Migrate events to new schema
35. ✨ `rollback_migration` - Rollback schema migration
36. ✨ `export_events` - Export events to file
37. ✨ `import_events` - Import events from file
38. ✨ `audit_trail` - Generate audit trail report

#### **Category 5: Policy & Governance** (6 tools)
39. ✨ `create_policy` - Create access control policy
40. ✨ `evaluate_policy` - Test policy evaluation
41. ✨ `list_policies` - List all policies
42. ✨ `update_policy` - Update existing policy
43. ✨ `policy_audit` - Audit policy usage
44. ✨ `compliance_report` - Generate compliance report

#### **Category 6: Tenant Management** (5 tools)
45. ✨ `create_tenant` - Create new tenant
46. ✨ `list_tenants` - List all tenants
47. ✨ `tenant_usage` - Get tenant usage metrics
48. ✨ `tenant_isolation` - Verify tenant isolation
49. ✨ `cross_tenant_query` - Query across tenants (admin)

#### **Category 7: Monitoring & Observability** (6 tools)
50. ✨ `get_metrics` - Get comprehensive metrics
51. ✨ `get_latency_stats` - Query latency statistics
52. ✨ `get_throughput` - Ingestion throughput metrics
53. ✨ `alert_history` - Get alert history
54. ✨ `performance_report` - System performance report
55. ✨ `resource_usage` - Resource usage analytics

**Total**: 55 tools (44 new + 11 existing)

---

## 📋 Implementation Priority

### Phase 1: Core Analytics (High Priority)
**Timeline**: 1-2 weeks

Tools to implement:
1. `advanced_query` - Most impactful for users
2. `time_series_analysis` - High-value analytics
3. `funnel_analysis` - Business-critical
4. `detect_anomalies` - Operational value
5. `compute_aggregation` - General-purpose

**Rationale**: These leverage our Clojure analytics engine and provide immediate value.

### Phase 2: Projection Management (High Priority)
**Timeline**: 1-2 weeks

Tools to implement:
6. `create_projection`
7. `start_projection`
8. `get_projection_state`
9. `get_projection_status`
10. `list_projections`

**Rationale**: Projections are core to the architecture and frequently used.

### Phase 3: Pipeline & Integration (Medium Priority)
**Timeline**: 2-3 weeks

Tools to implement:
11. `create_pipeline`
12. `execute_pipeline`
13. `replay_events`
14. `validate_events`
15. `migrate_schema`

**Rationale**: Power users need these for advanced workflows.

### Phase 4: Governance & Multi-Tenancy (Medium Priority)
**Timeline**: 2-3 weeks

Tools to implement:
16. `create_policy`
17. `evaluate_policy`
18. `create_tenant`
19. `tenant_usage`
20. `compliance_report`

**Rationale**: Essential for enterprise deployments.

### Phase 5: Enhanced Monitoring (Lower Priority)
**Timeline**: 1-2 weeks

Tools to implement:
21. `get_metrics`
22. `performance_report`
23. `resource_usage`

**Rationale**: Ops teams benefit, but less critical than core features.

---

## 🏗️ Technical Architecture

### Enhanced MCP Server Structure

```typescript
// New service clients
class ClojureQueryClient {
  // Interfaces with Clojure query-service
  async executeQuery(dsl: QueryDSL): Promise<QueryResult>
  async createProjection(def: ProjectionDef): Promise<ProjectionResult>
  async executePipeline(pipeline: PipelineDef): Promise<PipelineResult>
  async computeAnalytics(config: AnalyticsConfig): Promise<AnalyticsResult>
}

class PolicyEngineClient {
  // Interfaces with Go control-plane
  async createPolicy(policy: Policy): Promise<PolicyResult>
  async evaluatePolicy(request: PolicyRequest): Promise<Decision>
  async listPolicies(): Promise<Policy[]>
}

class TenantManagerClient {
  // Interfaces with Go control-plane
  async createTenant(config: TenantConfig): Promise<Tenant>
  async listTenants(): Promise<Tenant[]>
  async getTenantUsage(tenantId: string): Promise<UsageMetrics>
}

// Enhanced MCP Server
class EnhancedAllSourceMCPServer {
  private clojureClient: ClojureQueryClient;
  private policyClient: PolicyEngineClient;
  private tenantClient: TenantManagerClient;

  // ... existing Rust core client
}
```

### Configuration

```typescript
// Environment variables
const CLOJURE_QUERY_URL = process.env.ALLSOURCE_CLOJURE_URL || 'http://localhost:7888';
const RUST_CORE_URL = process.env.ALLSOURCE_CORE_URL || 'http://localhost:3900';
const GO_CONTROL_URL = process.env.ALLSOURCE_CONTROL_URL || 'http://localhost:8081';
```

---

## 💡 Example: Enhanced Tool Implementations

### 1. Advanced Query with Clojure DSL

```typescript
{
  name: 'advanced_query',
  description: 'Execute advanced queries using the Clojure Query DSL with aggregations, time-series, and complex filters.',
  inputSchema: {
    type: 'object',
    properties: {
      query_dsl: {
        type: 'object',
        description: 'Clojure-style query map',
        properties: {
          select: { type: 'array', items: { type: 'string' } },
          from: { type: 'string' },
          where: { type: 'array' },
          aggregations: { type: 'array' },
          groupBy: { type: 'array' },
          orderBy: { type: 'array' },
          limit: { type: 'number' }
        }
      }
    },
    required: ['query_dsl']
  }
}

// Example usage:
{
  "query_dsl": {
    "select": ["entity_id", "event_type"],
    "from": "events",
    "where": ["and",
      ["=", "event_type", "order.placed"],
      [">", "timestamp", "days-ago(7)"]
    ],
    "aggregations": [
      {"function": "sum", "field": ["payload", "amount"], "alias": "total_revenue"},
      {"function": "avg", "field": ["payload", "amount"], "alias": "avg_order"}
    ],
    "groupBy": ["entity_id"],
    "limit": 100
  }
}
```

### 2. Time Series Analysis

```typescript
{
  name: 'time_series_analysis',
  description: 'Analyze events over time with configurable intervals and aggregations.',
  inputSchema: {
    type: 'object',
    properties: {
      event_type: { type: 'string' },
      interval: {
        type: 'string',
        enum: ['second', 'minute', 'hour', 'day', 'week', 'month']
      },
      aggregations: {
        type: 'array',
        items: {
          type: 'object',
          properties: {
            function: {
              type: 'string',
              enum: ['count', 'sum', 'avg', 'min', 'max', 'p50', 'p95', 'p99']
            },
            field: { type: 'string' },
            alias: { type: 'string' }
          }
        }
      },
      start_time: { type: 'string' },
      end_time: { type: 'string' },
      fill_missing: {
        type: 'string',
        enum: ['zero', 'null', 'forward_fill']
      }
    },
    required: ['interval', 'aggregations']
  }
}

// Example usage:
{
  "event_type": "order.placed",
  "interval": "hour",
  "aggregations": [
    {"function": "count", "alias": "order_count"},
    {"function": "sum", "field": "payload.amount", "alias": "revenue"},
    {"function": "avg", "field": "payload.amount", "alias": "avg_order_value"}
  ],
  "start_time": "2025-10-17T00:00:00Z",
  "end_time": "2025-10-24T00:00:00Z",
  "fill_missing": "zero"
}
```

### 3. Funnel Analysis

```typescript
{
  name: 'funnel_analysis',
  description: 'Analyze conversion funnels with multiple steps and time windows.',
  inputSchema: {
    type: 'object',
    properties: {
      funnel_name: { type: 'string' },
      steps: {
        type: 'array',
        items: {
          type: 'object',
          properties: {
            name: { type: 'string' },
            event_type: { type: 'string' },
            order: { type: 'number' }
          }
        }
      },
      time_window_ms: {
        type: 'number',
        description: 'Maximum time between steps in milliseconds'
      },
      since: { type: 'string' }
    },
    required: ['funnel_name', 'steps']
  }
}

// Example usage:
{
  "funnel_name": "signup_funnel",
  "steps": [
    {"name": "visit", "event_type": "page.view", "order": 1},
    {"name": "signup", "event_type": "user.signup", "order": 2},
    {"name": "activation", "event_type": "user.activated", "order": 3}
  ],
  "time_window_ms": 86400000,  // 24 hours
  "since": "2025-10-01T00:00:00Z"
}
```

### 4. Create Projection

```typescript
{
  name: 'create_projection',
  description: 'Create a new projection that maintains a queryable read model from event streams.',
  inputSchema: {
    type: 'object',
    properties: {
      name: { type: 'string' },
      version: { type: 'number' },
      initial_state: { type: 'object' },
      projection_logic: {
        type: 'string',
        description: 'Clojure function definition as string'
      },
      auto_start: { type: 'boolean' }
    },
    required: ['name', 'version', 'initial_state', 'projection_logic']
  }
}

// Example usage:
{
  "name": "user_statistics",
  "version": 1,
  "initial_state": {
    "count": 0,
    "total_orders": 0,
    "total_revenue": 0.0
  },
  "projection_logic": "(fn [state event] (case (:event-type event) \"user.created\" (update state :count inc) \"order.placed\" (-> state (update :total-orders inc) (update :total-revenue + (get-in event [:payload :amount]))) state))",
  "auto_start": true
}
```

### 5. Anomaly Detection

```typescript
{
  name: 'detect_anomalies',
  description: 'Detect anomalies in time-series data using statistical methods.',
  inputSchema: {
    type: 'object',
    properties: {
      metric_name: { type: 'string' },
      metric_field: { type: 'string' },
      algorithm: {
        type: 'string',
        enum: ['zscore', 'iqr', 'mad'],
        description: 'Z-score (standard), IQR (robust), or MAD (very robust)'
      },
      sensitivity: {
        type: 'number',
        description: '1-10, higher = more sensitive (default: 3)'
      },
      baseline_window: {
        type: 'number',
        description: 'Number of data points for baseline (default: 30)'
      },
      since: { type: 'string' }
    },
    required: ['metric_name', 'metric_field']
  }
}

// Example usage:
{
  "metric_name": "request_latency",
  "metric_field": "payload.latency_ms",
  "algorithm": "zscore",
  "sensitivity": 3,
  "baseline_window": 30,
  "since": "2025-10-23T00:00:00Z"
}
```

---

## 🔒 Security Considerations

### 1. API Key Authentication
Add support for API key authentication when calling Clojure and Go services:

```typescript
const CLOJURE_API_KEY = process.env.ALLSOURCE_CLOJURE_API_KEY;
const CONTROL_API_KEY = process.env.ALLSOURCE_CONTROL_API_KEY;

// Add headers to all requests
headers: {
  'Authorization': `Bearer ${CLOJURE_API_KEY}`,
  'X-Tenant-ID': tenantId
}
```

### 2. Rate Limiting
Implement rate limiting for expensive operations:

```typescript
private rateLimiter = new Map<string, number>();

private async checkRateLimit(toolName: string, limit: number = 10) {
  const key = `${toolName}-${Date.now() / 60000 | 0}`;
  const count = this.rateLimiter.get(key) || 0;
  if (count >= limit) {
    throw new Error(`Rate limit exceeded for ${toolName}`);
  }
  this.rateLimiter.set(key, count + 1);
}
```

### 3. Input Validation
Use Zod schemas for strict validation of all inputs.

### 4. Tenant Isolation
Ensure all operations are scoped to the user's tenant:

```typescript
private async validateTenantAccess(tenantId: string, resourceId: string) {
  // Validate user has access to this tenant/resource
}
```

---

## 📊 Benefits of Enhanced MCP

### For AI Agents
1. **More Powerful**: 55 tools vs. 11 tools (5x increase)
2. **Advanced Analytics**: Time-series, funnels, anomalies, trends
3. **State Management**: Projections for fast queries
4. **Data Pipeline**: Event processing workflows
5. **Governance**: Policy-based access control

### For Developers
1. **Less Code**: MCP handles complexity
2. **Natural Language**: AI translates NL to API calls
3. **Comprehensive**: All AllSource features accessible
4. **Production-Ready**: Monitoring, auditing, compliance

### For Operations
1. **Observability**: Detailed metrics and health checks
2. **Troubleshooting**: Event replay, validation, migration
3. **Governance**: Policy enforcement and auditing
4. **Multi-Tenancy**: Tenant isolation and management

---

## 🚀 Migration Path

### Step 1: Add Clojure Client (Week 1)
```bash
# Install dependencies
cd packages/mcp-server
bun add axios zod

# Create ClojureQueryClient class
# Implement basic query, projection, analytics tools
```

### Step 2: Add Policy Client (Week 1)
```bash
# Create PolicyEngineClient class
# Implement policy CRUD and evaluation
```

### Step 3: Add Advanced Tools (Weeks 2-3)
```bash
# Implement 20 high-priority tools
# Focus on analytics, projections, pipelines
```

### Step 4: Testing & Documentation (Week 4)
```bash
# Write integration tests
# Update MCP documentation
# Create example notebooks
```

---

## 📝 Next Steps

1. **Review & Approve**: Team reviews this plan
2. **Prioritize**: Confirm Phase 1 priorities
3. **Implement**: Start with Phase 1 (core analytics)
4. **Test**: Integration testing with Claude Desktop
5. **Document**: Create user guide with examples
6. **Deploy**: Roll out to production

---

**Status**: ✅ Enhancement plan ready for review
**Impact**: 5x increase in MCP capabilities
**Effort**: 8-10 weeks for full implementation
**Priority**: High - unlocks all new AllSource features for AI agents
