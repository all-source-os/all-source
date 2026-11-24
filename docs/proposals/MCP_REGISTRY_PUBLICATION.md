# MCP Registry Publication Plan

## Useful MCP Servers for AllSource

### Must-Have (use directly)
| Server | Value |
|--------|-------|
| [GitHub MCP](https://github.com/github/github-mcp-server) | Manage PRs, issues, releases for all-source-os repos |
| [Git MCP](https://github.com/modelcontextprotocol/servers/tree/main/src/git) | Local repo operations |

### Study for API Design Patterns
| Server | Why |
|--------|-----|
| [Confluent Kafka](https://github.com/confluentinc/mcp-confluent) | Closest architectural analogy — event streaming MCP |
| [ClickHouse](https://github.com/ClickHouse/mcp-clickhouse) | Time-series/event query MCP patterns |
| [Elasticsearch](https://github.com/elastic/mcp-server-elasticsearch) | Search/query API patterns |
| [Axiom](https://github.com/axiomhq/mcp-server-axiom) | Natural language to event queries |
| [Apollo GraphQL](https://github.com/apollographql/apollo-mcp-server/) | API gateway MCP (similar to Query Service) |

### Database / Analytics
| Server | Why |
|--------|-----|
| [Apache Pinot](https://github.com/startreedata/mcp-pinot) | Real-time OLAP — analogous to Parquet-based analytics |
| [Apache IoTDB](https://github.com/apache/iotdb-mcp-server) | Time-series database MCP — relevant event store patterns |
| [Fireproof](https://github.com/fireproof-storage/mcp-database-server) | Immutable ledger database with sync — close to event store semantics |
| [DataHub](https://github.com/acryldata/mcp-server-datahub) | Data lineage/metadata — useful for schema registry integration |

### Monitoring / Observability
| Server | Why |
|--------|-----|
| [Axiom](https://github.com/axiomhq/mcp-server-axiom) | Query logs/traces via natural language |
| [Dynatrace](https://github.com/dynatrace-oss/dynatrace-mcp) | Platform monitoring MCP |

### CI/CD
| Server | Why |
|--------|-----|
| [GitHub](https://github.com/github/github-mcp-server) | Official GitHub MCP — manage repos, PRs, issues |
| [CircleCI](https://github.com/CircleCI-Public/mcp-server-circleci) | Fix build failures via AI |

---

## AllSource MCP Server — Current State

The Elixir MCP server (`apps/mcp-server-elixir/`) is production-ready:

- **47 to 73 tools**, depending on configuration. `list_tools/1` (`apps/mcp-server-elixir/lib/mcp_server_elixir/protocol/mcp_tools.ex`) gates 8 tools on `read_only`, 18 on `control_plane_enabled` and `system_admin`, and removes backend-specific tools via `unsupported_tools/0`. Range before backend removal: 47 (`read_only` on, control plane off) to 73 (all flags on)
- **TOON encoding** — about 50% fewer tokens than JSON for tabular data
- **OTP supervision** — fault tolerance and real-time WebSocket streaming
- **704 ExUnit test blocks** under `apps/mcp-server-elixir/test`; Credo (`~> 1.7`) and Dialyzer (`dialyxir ~> 1.4`) in the dev and test dependencies
- **Docker image** with multi-stage build, Alpine 3.23 (`ALPINE_VERSION`), non-root user, healthcheck
- No MCP Resources defined (tools only — resources optional)

---

## Publication Plan

### 1. Namespace Decision

| Option | Namespace | Setup |
|--------|-----------|-------|
| A (recommended) | `io.github.all-source-os/chronos-mcp` | GitHub OAuth — zero setup |
| B (branded) | `io.allsource/chronos-mcp` | DNS TXT record on `all-source.xyz` |

### 2. Dockerfile Label

Add to `apps/mcp-server-elixir/Dockerfile`:

```dockerfile
LABEL io.modelcontextprotocol.server.name="io.github.all-source-os/chronos-mcp"
```

### 3. Create `server.json`

Place at `apps/mcp-server-elixir/server.json`:

```json
{
  "$schema": "https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json",
  "name": "io.github.all-source-os/chronos-mcp",
  "title": "AllSource Chronos",
  "description": "AI-native event store MCP server with 47 to 73 tools depending on configuration: event queries, time-travel state reconstruction, pattern detection, schema governance, tenant management, and TOON token-optimized responses.",
  "repository": {
    "url": "https://github.com/all-source-os/all-source",
    "source": "github"
  },
  "version": "0.30.0",
  "packages": [
    {
      "registryType": "oci",
      "identifier": "ghcr.io/all-source-os/allsource-mcp-server:0.30.0",
      "transport": { "type": "stdio" },
      "environmentVariables": [
        {
          "name": "ALLSOURCE_CORE_URL",
          "description": "Core event store URL (e.g., http://localhost:3900)",
          "isRequired": true,
          "isSecret": false
        },
        {
          "name": "ALLSOURCE_CONTROL_URL",
          "description": "Control plane URL (e.g., http://localhost:3901)",
          "isRequired": false,
          "isSecret": false
        },
        {
          "name": "ALLSOURCE_CORE_API_KEY",
          "description": "API key for authentication",
          "isRequired": false,
          "isSecret": true
        }
      ]
    }
  ]
}
```

### 4. Publish Steps

```bash
# Install publisher CLI
brew install mcp-publisher

# Authenticate via GitHub (org: all-source-os)
mcp-publisher login github

# Validate metadata
mcp-publisher validate

# Publish to registry
mcp-publisher publish

# Verify
curl "https://registry.modelcontextprotocol.io/v0.1/servers?search=io.github.all-source-os/chronos-mcp"
```

### 5. CI Automation (GitHub Actions)

Add to `.github/workflows/release.yml`:

```yaml
  publish-mcp-registry:
    name: Publish MCP Server to Registry
    needs: [build-mcp]
    runs-on: ubuntu-latest
    permissions:
      id-token: write  # OIDC auth — no secrets needed
    steps:
      - uses: actions/checkout@v4

      - name: Install mcp-publisher
        run: |
          curl -L "https://github.com/modelcontextprotocol/registry/releases/latest/download/mcp-publisher_linux_amd64.tar.gz" | tar xz mcp-publisher
          sudo mv mcp-publisher /usr/local/bin/

      - name: Publish to MCP Registry
        working-directory: apps/mcp-server-elixir
        run: mcp-publisher publish
```

### 6. Optional: Add MCP Resources

Consider adding resources for LLM browsing:
- Event schemas (browseable by LLMs)
- Cluster health status
- Tenant configuration

---

## MCP Ecosystem References

- **Registry**: https://registry.modelcontextprotocol.io
- **Registry repo**: https://github.com/modelcontextprotocol/registry (6.4K stars)
- **Spec**: https://github.com/modelcontextprotocol/modelcontextprotocol (7.2K stars)
- **Rust SDK**: https://github.com/modelcontextprotocol/rust-sdk (3K stars)
- **Go SDK**: https://github.com/modelcontextprotocol/go-sdk (3.8K stars)
- **Inspector**: https://github.com/modelcontextprotocol/inspector (8.7K stars) — visual testing tool
- **Conformance tests**: https://github.com/modelcontextprotocol/conformance
- **Server collection**: https://github.com/modelcontextprotocol/servers (78.6K stars, deprecated in favor of registry)

### Registry Package Types Supported
- **npm** — TypeScript/JavaScript servers
- **PyPI** — Python servers
- **NuGet** — C#/.NET servers
- **OCI** — Docker (ghcr.io, Docker Hub, Google AR, Azure CR)
- **MCPB** — One-click desktop install via GitHub/GitLab releases

### No Manual Review
Publishing is automated — authenticate namespace ownership, pass schema validation, and it goes live immediately.

---

## API Design Pattern Analysis

Deep analysis of 5 production MCP servers to inform AllSource's design.

### 1. Confluent Kafka MCP (47 tools)

**Architecture**: Domain-organized handler files (`topics.go`, `subjects.go`, `connectors.go`, `flink.go`, `environments.go`). Each handler implements `HandleCall(name, args)` dispatch. Static factory in `tools.go` registers all 47 tools.

**Key patterns**:
- **Lazy client init**: Clients created on first tool call, not at startup. Avoids blocking startup when services are unreachable.
- **Conditional tool enablement**: Environment variables gate tool categories (e.g., Flink tools only register if `FLINK_ENV` is set). Keeps tool surface contextual.
- **Cross-tool references**: Tool descriptions include "Use `list_topics` first to get topic names" — guiding LLMs through multi-step workflows.
- **Multi-transport**: Supports stdio, HTTP, and SSE from the same binary via CLI flags.
- **Naming**: Verb-noun (`list_topics`, `create_connector`, `get_schema`). No namespacing prefix.

**Takeaway for AllSource**: Our 47 to 73 tools could benefit from conditional enablement (e.g., tenant management tools only when control plane is configured) and cross-tool workflow hints in descriptions.

### 2. ClickHouse MCP (3-4 tools)

**Architecture**: Minimal tool surface — `run_select_query`, `list_databases`, `list_tables`, `get_table_schema`. Philosophy: "SQL is the API."

**Key patterns**:
- **Read-only enforcement**: Only SELECT queries allowed. The tool itself rejects mutations.
- **Columnar result format**: Returns data as column arrays, not row objects — 40-60% fewer tokens for tabular data.
- **Prompt engineering over tool constraints**: Heavy system prompts teach LLMs how to write ClickHouse SQL, rather than building many specialized tools.
- **Cursor-based pagination**: Results have TTL-cached cursors for large datasets.

**Takeaway for AllSource**: Our TOON encoding already achieves similar token savings. Consider whether some of our 47 to 73 tools could be consolidated into a "query" tool with rich prompts, like ClickHouse does. Read-only mode flag is valuable.

### 3. Elasticsearch MCP (5 tools, Rust)

**Architecture**: Built on `rmcp` Rust SDK. 5 tools: `search`, `get_mappings`, `list_indices`, `get_shards`, `get_cluster_health`.

**Key patterns**:
- **Raw DSL passthrough**: The `search` tool accepts raw Elasticsearch query DSL — no abstraction layer. LLMs write native queries.
- **Multi-content-block responses**: Returns both a text summary block AND a JSON data block per response. LLMs get human-readable context plus machine-parseable data.
- **Read-only by design**: No write tools. Explicitly scoped to observability/exploration.
- **Custom tool templates**: Descriptions include JSON template examples of valid query DSL structures.

**Takeaway for AllSource**: Multi-content responses (summary + data) is a strong pattern — our TOON responses could pair with a brief natural language summary. Including query DSL examples in tool descriptions is better than documentation links.

### 4. Axiom MCP (6 tools, Go)

**Architecture**: 6 tools: `queryApl`, `listDatasets`, `getDatasetInfo`, `createAnnotation`, `getAnnotation`, `listAnnotations`. Built with Go MCP SDK.

**Key patterns**:
- **Massive tool descriptions as prompt engineering**: The `queryApl` tool description is 200+ lines — a full APL (Axiom Processing Language) tutorial. This teaches the LLM the query language inline.
- **Schema-first workflow**: LLMs call `getDatasetInfo` to discover field names/types before writing queries. The description explicitly says "call this before queryApl."
- **Rate limiting per tool category**: Query tools rate-limited differently from annotation tools.
- **Raw JSON output**: No token optimization — returns full JSON. They prioritize simplicity over efficiency.

**Takeaway for AllSource**: Our tool descriptions should be richer — include mini-tutorials for complex tools like `query_events` or `search_events`. The schema-first pattern (discover → query) maps well to our `list_event_types` → `query_events` flow.

### 5. Apollo GraphQL MCP (4 built-in + dynamic)

**Architecture**: 4 core tools: `introspect_schema`, `search_schema`, `validate_operation`, `execute_operation`. Dynamic tools generated from persisted queries in Apollo's registry.

**Key patterns**:
- **Schema minification**: Strips comments, descriptions, and whitespace from GraphQL schemas before sending to LLM — reduces tokens by 60-80%.
- **Guided exploration pattern**: Tools designed as a funnel: introspect → search → validate → execute. Each step narrows scope.
- **Operation allow-listing**: In production, only pre-registered queries can execute. The MCP server enforces this governance.
- **OAuth 2.1 with JWKS**: Full OAuth flow for authentication, not just API keys.

**Takeaway for AllSource**: The guided exploration funnel is excellent UX for LLMs. Our tools could follow: `list_event_types` → `search_events` → `query_events` → `get_event`. Allow-listing operations maps to our policy engine in the control plane.

---

### Synthesis: Patterns to Adopt

| Pattern | Source | AllSource Application |
|---------|--------|-----------------------|
| **Conditional tool enablement** | Confluent | Gate control-plane tools on `ALLSOURCE_CONTROL_URL` presence |
| **Cross-tool workflow hints** | Confluent, Axiom | Add "Use `list_event_types` first" to query tool descriptions |
| **Multi-content responses** | Elasticsearch | Pair TOON data with a brief natural language summary block |
| **Rich descriptions as tutorials** | Axiom | Expand `query_events` description with query syntax examples |
| **Schema-first discovery** | Axiom, Apollo | Promote discover → query → drill-down workflow in tool ordering |
| **Guided exploration funnel** | Apollo | Order tools: list → search → query → get (narrowing scope) |
| **Read-only mode** | ClickHouse, Elasticsearch | Add `ALLSOURCE_READ_ONLY=true` flag to disable mutation tools |
| **Lazy client init** | Confluent | Don't fail startup if Core is temporarily unreachable |

### Patterns to Skip

| Pattern | Source | Why Skip |
|---------|--------|----------|
| Raw DSL passthrough | Elasticsearch | Our query API is simpler than ES DSL — purpose-built tools are better UX |
| SQL-is-the-API (few tools) | ClickHouse | We already have 47 to 73 purpose-built tools — that's our strength |
| Full OAuth 2.1 | Apollo | API key auth is sufficient for MCP; OAuth adds complexity for CLI use |
| Raw JSON output | Axiom | TOON already gives us 50% token savings — keep it |
