defmodule McpServerElixir.Protocol.McpTools do
  @moduledoc """
  MCP Tools implementation - defines all available tools and their handlers.

  This module provides the tools that the MCP server exposes to AI assistants,
  enabling natural language interaction with the AllSource event store.
  """

  require Logger

  alias McpServerElixir.Context.ConversationContext
  alias McpServerElixir.Infrastructure.ControlPlaneClient
  alias McpServerElixir.Protocol.ToonEncoder

  @doc """
  List all available MCP tools, filtered by configuration.

  Accepts a config map with:
  - `:read_only` — when true, excludes mutation tools
  - `:control_plane_enabled` — when false, excludes tenant management tools
  """
  def list_tools(config \\ %{}) do
    # Phase 1: Discover (what data exists?) — START HERE
    discover_tools = [
      tool_quick_stats(),
      tool_sample_events(),
      tool_get_stats(),
      tool_get_cluster_status(),
      tool_list_schemas()
    ]

    # Phase 2: Search (find relevant data)
    search_tools = [
      tool_query_events(),
      tool_semantic_search_events(),
      tool_hybrid_search(),
      tool_get_query_advice()
    ]

    # Phase 3: Drill down (deep analysis)
    drill_down_tools = [
      tool_get_snapshot(),
      tool_reconstruct_state(),
      tool_analyze_changes(),
      tool_event_timeline(),
      tool_explain_entity(),
      tool_find_patterns(),
      tool_compare_entities()
    ]

    # Phase 4: Context (multi-turn conversation)
    context_tools = [
      tool_start_session(),
      tool_refine_query(),
      tool_get_session_context()
    ]

    # Phase 5: Mutate (write operations) — gated by read_only
    mutate_tools =
      if config[:read_only] do
        []
      else
        [
          tool_ingest_event(),
          tool_delete_events(),
          tool_archive_events(),
          tool_import_events(),
          tool_clone_entity(),
          tool_merge_entities(),
          tool_split_entity()
        ]
      end

    # Phase 6: Read-only event management (always available)
    event_read_tools = [
      tool_restore_events(),
      tool_export_events()
    ]

    # Phase 7: Operations — mutation ops gated by read_only
    ops_read_tools = [
      tool_storage_stats(),
      tool_partition_info(),
      tool_wal_status(),
      tool_backup_list(),
      tool_health_deep(),
      tool_performance_report(),
      tool_audit_log()
    ]

    ops_mutate_tools =
      if config[:read_only] do
        []
      else
        [
          tool_compact_storage(),
          tool_backup_create(),
          tool_backup_restore()
        ]
      end

    # Phase 8: Tenants — gated by control_plane_enabled
    tenant_tools =
      if config[:control_plane_enabled] do
        [
          tool_tenant_create(),
          tool_tenant_update(),
          tool_tenant_usage(),
          tool_tenant_quotas(),
          tool_tenant_suspend(),
          tool_tenant_export()
        ]
      else
        []
      end

    # Phase 8b: Fleet health & recovery — thin consumers of the Control Plane
    # /api/v1/admin/* fleet+recovery API (built in CP commit a02667e). These
    # live HERE in mcp-server-elixir, NEVER in prime-mcp: prime-mcp is
    # single-tenant by design (projection_registry.rs:22; http.rs:294,379), so a
    # fleet/cross-tenant tool there crosses a tenant boundary it does not have.
    #
    # Gating is two-tier:
    #   - the two READ tools require control_plane_enabled (ALLSOURCE_CONTROL_URL);
    #   - the eight mutating recovery_* tools ALSO require a NEW system_admin
    #     distinction (ALLSOURCE_SYSTEM_ADMIN, off by default) — so a merely
    #     connected client can read health but cannot run a Destructive recovery.
    # No scoring or guards are computed here: the health model + dry-run /
    # confirmation guards live entirely in the Control Plane.
    fleet_tools =
      if config[:control_plane_enabled] do
        # READ tier (control_plane_enabled): fleet/tenant health + the per-tenant
        # 360 (tenant_overview composes the three EXISTING admin reads). These
        # only READ the CP admin endpoints.
        base = [
          tool_fleet_health_summary(),
          tool_tenant_health_assessment(),
          tool_tenant_overview()
        ]

        if config[:system_admin] do
          # MUTATING tier (system_admin): the eight recovery_* tools PLUS
          # tenant_notice (sends an in-app notice — a mutation/comms, so it joins
          # the system_admin gate, same bar as the Destructive recovery tools).
          base ++
            [
              tool_recovery_resync(),
              tool_recovery_reconcile_subscription(),
              tool_recovery_resolve_dunning(),
              tool_recovery_rotate_keys(),
              tool_recovery_reprovision(),
              tool_recovery_restore(),
              tool_recovery_batch(),
              tool_recovery_diagnose_edition(),
              tool_tenant_notice()
            ]
        else
          base
        end
      else
        []
      end

    # Phase 9: Schema & validation
    schema_tools = [
      tool_register_schema(),
      tool_validate_schema(),
      tool_migrate_schema(),
      tool_infer_schema(),
      tool_schema_diff()
    ]

    # Phase 10: Analytics
    analytics_tools = [
      tool_cohort_analysis(),
      tool_correlation_analysis(),
      tool_forecast_events(),
      tool_segment_analysis(),
      tool_path_analysis(),
      tool_attribution_analysis(),
      tool_churn_prediction(),
      tool_ltv_calculation()
    ]

    # Phase 11: Developer tools
    developer_tools = [
      tool_generate_client(),
      tool_mock_events(),
      tool_debug_query(),
      tool_benchmark_query()
    ]

    discover_tools ++
      search_tools ++
      drill_down_tools ++
      context_tools ++
      mutate_tools ++
      event_read_tools ++
      ops_mutate_tools ++
      ops_read_tools ++
      tenant_tools ++
      fleet_tools ++
      schema_tools ++
      analytics_tools ++
      developer_tools
  end

  @tool_handlers %{
    "query_events" => :handle_query_events,
    "reconstruct_state" => :handle_reconstruct_state,
    "get_snapshot" => :handle_get_snapshot,
    "analyze_changes" => :handle_analyze_changes,
    "find_patterns" => :handle_find_patterns,
    "compare_entities" => :handle_compare_entities,
    "event_timeline" => :handle_event_timeline,
    "explain_entity" => :handle_explain_entity,
    "ingest_event" => :handle_ingest_event,
    "semantic_search_events" => :handle_semantic_search_events,
    "hybrid_search" => :handle_hybrid_search
  }

  @stateless_tool_handlers %{
    "get_stats" => :handle_get_stats,
    "get_cluster_status" => :handle_get_cluster_status
  }

  # Advisory tools don't need state but do need args
  @advisory_tool_handlers %{
    "get_query_advice" => :handle_get_query_advice
  }

  # Quick exploration tools - optimized for sub-second response
  @exploration_tool_handlers %{
    "sample_events" => :handle_sample_events,
    "quick_stats" => :handle_quick_stats
  }

  # Context tools work with ConversationContext GenServer
  @context_tool_handlers %{
    "start_session" => :handle_start_session,
    "refine_query" => :handle_refine_query,
    "get_session_context" => :handle_get_session_context
  }

  # Event management tools (v2.0) - lifecycle operations
  @event_management_tool_handlers %{
    "delete_events" => :handle_delete_events,
    "archive_events" => :handle_archive_events,
    "restore_events" => :handle_restore_events,
    "export_events" => :handle_export_events,
    "import_events" => :handle_import_events,
    "clone_entity" => :handle_clone_entity,
    "merge_entities" => :handle_merge_entities,
    "split_entity" => :handle_split_entity
  }

  # Operational tools (v2.0) - system management
  @operational_tool_handlers %{
    "compact_storage" => :handle_compact_storage,
    "storage_stats" => :handle_storage_stats,
    "partition_info" => :handle_partition_info,
    "wal_status" => :handle_wal_status,
    "backup_create" => :handle_backup_create,
    "backup_restore" => :handle_backup_restore,
    "backup_list" => :handle_backup_list,
    "health_deep" => :handle_health_deep,
    "performance_report" => :handle_performance_report,
    "audit_log" => :handle_audit_log
  }

  # Multi-tenancy tools (v2.0) - tenant management (via Go Control Plane)
  @tenant_tool_handlers %{
    "tenant_create" => :handle_tenant_create,
    "tenant_update" => :handle_tenant_update,
    "tenant_usage" => :handle_tenant_usage,
    "tenant_quotas" => :handle_tenant_quotas,
    "tenant_suspend" => :handle_tenant_suspend,
    "tenant_export" => :handle_tenant_export
  }

  # Fleet health & recovery tools (P3) - thin consumers of the Control Plane
  # /api/v1/admin/* fleet+recovery API. Read tools are control_plane gated;
  # the recovery_* tools are additionally system_admin gated (see call_tool/3).
  @fleet_tool_handlers %{
    "fleet_health_summary" => :handle_fleet_health_summary,
    "tenant_health_assessment" => :handle_tenant_health_assessment,
    "tenant_overview" => :handle_tenant_overview,
    "recovery_resync" => :handle_recovery_resync,
    "recovery_reconcile_subscription" => :handle_recovery_reconcile_subscription,
    "recovery_resolve_dunning" => :handle_recovery_resolve_dunning,
    "recovery_rotate_keys" => :handle_recovery_rotate_keys,
    "recovery_reprovision" => :handle_recovery_reprovision,
    "recovery_restore" => :handle_recovery_restore,
    "recovery_batch" => :handle_recovery_batch,
    "recovery_diagnose_edition" => :handle_recovery_diagnose_edition,
    "tenant_notice" => :handle_tenant_notice
  }

  # Schema & validation tools (v2.0) - event type governance
  @schema_tool_handlers %{
    "register_schema" => :handle_register_schema,
    "validate_schema" => :handle_validate_schema,
    "migrate_schema" => :handle_migrate_schema,
    "list_schemas" => :handle_list_schemas,
    "infer_schema" => :handle_infer_schema,
    "schema_diff" => :handle_schema_diff
  }

  # Advanced analytics tools (v2.0) - business intelligence
  @analytics_tool_handlers %{
    "cohort_analysis" => :handle_cohort_analysis,
    "correlation_analysis" => :handle_correlation_analysis,
    "forecast_events" => :handle_forecast_events,
    "segment_analysis" => :handle_segment_analysis,
    "path_analysis" => :handle_path_analysis,
    "attribution_analysis" => :handle_attribution_analysis,
    "churn_prediction" => :handle_churn_prediction,
    "ltv_calculation" => :handle_ltv_calculation
  }

  # Developer experience tools (v2.0) - productivity
  @developer_tool_handlers %{
    "generate_client" => :handle_generate_client,
    "mock_events" => :handle_mock_events,
    "debug_query" => :handle_debug_query,
    "benchmark_query" => :handle_benchmark_query
  }

  @doc """
  Call a tool by name with arguments.

  Supports optional `format` parameter:
  - `"toon"` - Force TOON format (~50% fewer tokens)
  - `"json"` - Force JSON format
  - Omitted - Auto-detect (default: TOON for tabular data)
  """
  # Tools gated by read_only mode
  @read_only_gated_tools MapSet.new([
                           "ingest_event",
                           "delete_events",
                           "archive_events",
                           "import_events",
                           "clone_entity",
                           "merge_entities",
                           "split_entity",
                           "compact_storage",
                           "backup_create",
                           "backup_restore"
                         ])

  # Tools gated by control_plane_enabled. Includes the tenant management tools
  # AND the two read-only fleet-health tools (these only READ the CP admin
  # fleet/health endpoints, so control_plane_enabled is the right bar).
  @control_plane_gated_tools MapSet.new([
                               "tenant_create",
                               "tenant_update",
                               "tenant_usage",
                               "tenant_quotas",
                               "tenant_suspend",
                               "tenant_export",
                               "fleet_health_summary",
                               "tenant_health_assessment",
                               # Per-tenant 360 — pure READ composition of the CP
                               # admin reads, so control_plane_enabled is the bar
                               # (ADMIN_TENANT_POWER_TOOL §9 Phase 8).
                               "tenant_overview"
                             ])

  # Tools gated by system_admin (ALLSOURCE_SYSTEM_ADMIN). The mutating fleet
  # recovery tools. OFF by default — these are hidden from tools/list and
  # hard-rejected at call time unless the operator explicitly enabled
  # system-admin mode on this server instance. The Control Plane still enforces
  # its own dry-run / typed-confirmation / blast-radius guards server-side; this
  # gate is only the local "should this server even expose mutating recovery".
  @system_admin_gated_tools MapSet.new([
                              "recovery_resync",
                              "recovery_reconcile_subscription",
                              "recovery_resolve_dunning",
                              "recovery_rotate_keys",
                              "recovery_reprovision",
                              "recovery_restore",
                              "recovery_batch",
                              "recovery_diagnose_edition",
                              # Sending an in-app notice is a mutation/comms (and
                              # a cohort send fans out): same system_admin bar as
                              # the Destructive recovery tools. The CP still
                              # enforces the cohort dry-run/confirm-token/opt-out
                              # guards server-side (§9 Phase 8).
                              "tenant_notice"
                            ])

  def call_tool(tool_name, args, state) do
    format = Map.get(args, "format", nil)
    args_without_format = Map.delete(args, "format")

    # Check gating before dispatch
    cond do
      MapSet.member?(@read_only_gated_tools, tool_name) and Map.get(state, :read_only, false) ->
        {:ok,
         %{
           content: [
             %{
               type: "text",
               text:
                 "Tool disabled: read-only mode is active. Unset ALLSOURCE_READ_ONLY to enable mutation tools."
             }
           ],
           isError: true
         }}

      MapSet.member?(@control_plane_gated_tools, tool_name) and
          not Map.get(state, :control_plane_enabled, false) ->
        {:ok,
         %{
           content: [
             %{
               type: "text",
               text:
                 "Tool disabled: control plane not configured. Set ALLSOURCE_CONTROL_URL to enable tenant management tools."
             }
           ],
           isError: true
         }}

      MapSet.member?(@system_admin_gated_tools, tool_name) and
          not Map.get(state, :system_admin, false) ->
        {:ok,
         %{
           content: [
             %{
               type: "text",
               text:
                 "Tool disabled: system-admin not enabled. Fleet recovery actions are Destructive and " <>
                   "are off by default. Set ALLSOURCE_SYSTEM_ADMIN=true on this MCP server to enable them."
             }
           ],
           isError: true
         }}

      true ->
        safe_dispatch(tool_name, args_without_format, state, format)
    end
  end

  # A raising tool handler must not take the stdio session down with it.
  #
  # Every handler starts with `Map.fetch!(args, "<required>")`, so a client that
  # omits a required argument raised a KeyError straight out of the server
  # GenServer's `handle_info/2`, killing the process and dropping the client's
  # connection — which looks from the client side like the tools vanishing and
  # coming back with no config change (#229, fourth ask). Exits behave the same
  # way, and a backend call can exit on timeout.
  #
  # This is NOT the barrier that keeps the session alive — it cannot be. It sits
  # below `call_tool/3`'s own argument shaping, so a client sending `"arguments"`
  # as a JSON string raises before this rescue is ever reachable. The barrier is
  # `Server.guarded_process/2`, at the `handle_info/2` seam. What this buys is
  # per-tool attribution in the log and a clean `{:error, reason}` return, so a
  # handler failure is reported as *that tool* failing.
  # Resolved per call rather than hardcoded to the module name, so a test can
  # supervise its own context and stop depending on the application singleton.
  # A test that shares that singleton fails with "no process" whenever anything
  # else takes the app supervisor down, which is a failure in a process the test
  # neither starts nor owns (#253). Unset in production, so the name resolves to
  # the supervised ConversationContext exactly as before.
  defp context_server do
    Application.get_env(
      :mcp_server_elixir,
      :conversation_context_server,
      ConversationContext
    )
  end

  defp safe_dispatch(tool_name, args, state, format) do
    dispatch_tool(tool_name, args, state, format)
  rescue
    exception ->
      Logger.error(
        "Tool #{tool_name} raised: " <>
          Exception.format(:error, exception, __STACKTRACE__)
      )

      {:error, Exception.message(exception)}
  catch
    kind, value ->
      Logger.error("Tool #{tool_name} #{kind}: #{inspect(value)}")
      {:error, "#{kind}: #{inspect(value)}"}
  end

  defp dispatch_tool(tool_name, args, state, format) do
    cond do
      handler = Map.get(@tool_handlers, tool_name) ->
        apply(__MODULE__, handler, [args, state, format])

      handler = Map.get(@stateless_tool_handlers, tool_name) ->
        apply(__MODULE__, handler, [state, format])

      handler = Map.get(@advisory_tool_handlers, tool_name) ->
        apply(__MODULE__, handler, [args, format])

      handler = Map.get(@exploration_tool_handlers, tool_name) ->
        apply(__MODULE__, handler, [args, state, format])

      handler = Map.get(@context_tool_handlers, tool_name) ->
        apply(__MODULE__, handler, [args, state, format])

      handler = Map.get(@event_management_tool_handlers, tool_name) ->
        apply(__MODULE__, handler, [args, state, format])

      handler = Map.get(@operational_tool_handlers, tool_name) ->
        apply(__MODULE__, handler, [args, state, format])

      handler = Map.get(@tenant_tool_handlers, tool_name) ->
        apply(__MODULE__, handler, [args, state, format])

      handler = Map.get(@fleet_tool_handlers, tool_name) ->
        apply(__MODULE__, handler, [args, state, format])

      handler = Map.get(@schema_tool_handlers, tool_name) ->
        apply(__MODULE__, handler, [args, state, format])

      handler = Map.get(@analytics_tool_handlers, tool_name) ->
        apply(__MODULE__, handler, [args, state, format])

      handler = Map.get(@developer_tool_handlers, tool_name) ->
        apply(__MODULE__, handler, [args, state, format])

      true ->
        {:error, "Unknown tool: #{tool_name}"}
    end
  end

  # ============================================================================
  # Tool Definitions
  # ============================================================================
  #
  # DECISION TREE: Choosing the Right Tool
  # ======================================
  #
  # ★ START HERE: quick_stats → sample_events → then drill down
  #
  # Phase 1 — DISCOVER (what data exists?)
  #   quick_stats      → fast approximate counts, entity/type overview
  #   sample_events    → see example events, understand structure
  #   get_stats        → detailed statistics
  #   list_schemas     → discover event type schemas
  #
  # Phase 2 — SEARCH (find relevant data)
  #   query_events             → exact filters (entity_id, event_type, time range)
  #   semantic_search_events   → natural language concept search
  #   hybrid_search            → combine semantic + keyword + metadata filters
  #   get_query_advice         → unsure which tool? start here
  #
  # Phase 3 — DRILL DOWN (deep analysis)
  #   get_snapshot       → current state of entity (fastest)
  #   reconstruct_state  → state at any point in time (time-travel)
  #   analyze_changes    → diff between two points in time
  #   event_timeline     → chronological view of entity events
  #   explain_entity     → comprehensive entity overview
  #   find_patterns      → detect patterns in event data
  #   compare_entities   → side-by-side entity comparison
  #
  # PERFORMANCE CONSIDERATIONS:
  # - get_snapshot is 10-100x faster than reconstruct_state for current state
  # - Use limit parameter to cap results in query_events
  # - semantic_search_events has higher latency due to embedding generation
  # - hybrid_search is most expensive but most powerful for complex queries
  #
  # ============================================================================

  defp tool_query_events do
    %{
      name: "query_events",
      description: """
      Query events with flexible filters. Returns TOON format by default (~50% fewer tokens).

      **When to use this tool:**
      - Retrieving raw events for a specific entity or event type
      - Filtering events by time range (since/until)
      - Getting events as they existed at a point in time (as_of for time-travel)
      - When you know the exact entity_id or event_type you're looking for

      **Common patterns:**
      - Get all events for an entity: `entity_id: "user-123"`
      - Get specific event types: `event_type: "user.created"`
      - Time-bounded queries: `since: "2024-01-01T00:00:00Z", until: "2024-01-31T23:59:59Z"`
      - Time-travel: `as_of: "2024-01-15T00:00:00Z"` to see events as they were on that date

      **Performance tips:**
      - Always use `limit` for exploratory queries (start with 10-50)
      - Combine `entity_id` with `event_type` for fastest queries
      - Use `since/until` to narrow time ranges on large datasets
      - Prefer get_snapshot over query_events when you only need current state

      **Decision guide:**
      - Need raw events? → query_events
      - Need current state? → get_snapshot (faster)
      - Need state at point in time? → reconstruct_state
      - Don't know exact filters? → semantic_search_events or hybrid_search

      **Recommended workflow:** Call `quick_stats` or `sample_events` first to discover \
      available entity_ids and event_types, then use those values here.

      **Query parameter reference:**
      - `entity_id`: Exact match, e.g., "user-123", "order-456"
      - `event_type`: Exact match with dot notation, e.g., "user.created", "order.completed", "payment.failed"
      - `since`/`until`: ISO-8601 timestamps. Supports full datetime ("2024-01-15T14:30:00Z") or date-only ("2024-01-15")
      - `as_of`: Time-travel parameter — returns events as if queried at that timestamp (excludes events ingested after)
      - `limit`: Integer, defaults to all. Start with 10-50 for exploration

      **Example combinations:**
      - Recent failures: `event_type: "payment.failed", since: "2024-01-01T00:00:00Z", limit: 20`
      - Entity audit: `entity_id: "user-123", limit: 100`
      - Time-travel snapshot: `entity_id: "order-456", as_of: "2024-06-15T00:00:00Z"`
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "entity_id" => %{
            type: "string",
            description: "Filter by entity ID (e.g., \"user-123\")"
          },
          "event_type" => %{
            type: "string",
            description: "Filter by event type (e.g., \"user.created\")"
          },
          "as_of" => %{
            type: "string",
            description: "Time-travel: get events as of this ISO timestamp"
          },
          "since" => %{type: "string", description: "Get events since this ISO timestamp"},
          "until" => %{type: "string", description: "Get events until this ISO timestamp"},
          "limit" => %{type: "number", description: "Limit number of results (default: all)"},
          "format" => %{
            type: "string",
            enum: ["toon", "json"],
            description: "Response format: 'toon' (default, ~50% fewer tokens) or 'json'"
          }
        }
      }
    }
  end

  defp tool_reconstruct_state do
    %{
      name: "reconstruct_state",
      description: """
      Reconstruct the complete state of an entity at any point in time by replaying \
      its event stream. Essential for time-travel queries and audit scenarios.

      **When to use this tool:**
      - Answering "What did this entity look like on [date]?"
      - Debugging: understanding state at the time of an incident
      - Compliance/audit: proving what data looked like at a specific moment
      - Comparing historical vs current state (use with analyze_changes)

      **Common patterns:**
      - Current state: `entity_id: "user-123"` (no as_of = current)
      - Historical state: `entity_id: "user-123", as_of: "2024-01-15T14:30:00Z"`
      - Before an incident: `as_of: "[incident_time - 1 hour]"`

      **Performance tips:**
      - Use get_snapshot instead if you only need current state (10-100x faster)
      - Reconstruction time scales with number of events for the entity
      - For very active entities (1000s of events), consider narrowing time range first
      - Cache results if you need to query the same point multiple times

      **Decision guide:**
      - Need current state? → get_snapshot (much faster)
      - Need state at specific time? → reconstruct_state
      - Need diff between two times? → analyze_changes (uses reconstruct internally)
      - Need full history? → event_timeline

      **Recommended workflow:** Call `get_snapshot` first for current state. Only use \
      this tool if you need historical state at a specific `as_of` timestamp.
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "entity_id" => %{type: "string", description: "The entity ID to reconstruct state for"},
          "as_of" => %{
            type: "string",
            description:
              "Reconstruct state as of this ISO timestamp (optional, defaults to current)"
          }
        },
        required: ["entity_id"]
      }
    }
  end

  defp tool_get_snapshot do
    %{
      name: "get_snapshot",
      description: """
      Get the current snapshot of an entity. This is the fastest way to retrieve \
      current state (10-100x faster than reconstruct_state).

      **When to use this tool:**
      - You need the current state of an entity (most common use case)
      - Quick lookups during conversations or workflows
      - Checking if an entity exists and its current values
      - Real-time dashboards or status checks

      **Common patterns:**
      - Simple lookup: `entity_id: "user-123"`
      - Check entity exists: Call get_snapshot, handle not-found gracefully
      - Quick reference during analysis: Get snapshot before deeper investigation

      **Performance tips:**
      - ALWAYS prefer this over reconstruct_state for current state
      - Snapshots are pre-computed and indexed for fast retrieval
      - Negligible cost compared to other operations - use liberally
      - Good for iterative exploration: snapshot → decide → drill deeper

      **Decision guide:**
      - Need current state? → get_snapshot (this tool!)
      - Need historical state? → reconstruct_state
      - Need full context about entity? → explain_entity (includes snapshot)
      - Need to compare states over time? → analyze_changes
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "entity_id" => %{type: "string", description: "The entity ID to get snapshot for"}
        },
        required: ["entity_id"]
      }
    }
  end

  defp tool_analyze_changes do
    %{
      name: "analyze_changes",
      description: """
      Analyze what changed for an entity between two points in time. Returns a \
      detailed diff showing added, modified, and removed fields.

      **When to use this tool:**
      - Answering "What changed between [date A] and [date B]?"
      - Investigating what happened during an incident window
      - Auditing changes made in a specific time period
      - Understanding the evolution of an entity's state

      **Common patterns:**
      - Changes today: `from_time: "[start of today]"` (to_time defaults to now)
      - Incident window: `from_time: "[incident_start]", to_time: "[incident_end]"`
      - Before/after deployment: `from_time: "[pre-deploy]", to_time: "[post-deploy]"`
      - Monthly audit: `from_time: "[month start]", to_time: "[month end]"`

      **Performance tips:**
      - Internally calls reconstruct_state twice (at from_time and to_time)
      - Cost scales with event volume in the entity's history
      - For very active entities, narrow the time window if possible
      - Consider event_timeline first to identify interesting time ranges

      **Decision guide:**
      - Need to know what changed? → analyze_changes (this tool!)
      - Need to see the events that caused changes? → event_timeline
      - Need state at one point? → reconstruct_state
      - Need to compare multiple entities? → compare_entities

      **Recommended workflow:** Use `event_timeline` first to identify interesting time \
      ranges, then call this tool with `from_time`/`to_time` from those ranges.
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "entity_id" => %{type: "string", description: "The entity to analyze"},
          "from_time" => %{type: "string", description: "Start timestamp (ISO format)"},
          "to_time" => %{
            type: "string",
            description: "End timestamp (ISO format, defaults to now)"
          }
        },
        required: ["entity_id", "from_time"]
      }
    }
  end

  defp tool_find_patterns do
    %{
      name: "find_patterns",
      description: """
      Detect patterns in event streams: frequency analysis, event sequences, or anomalies. \
      Essential for understanding behavior trends and detecting unusual activity.

      **When to use this tool:**
      - Answering "What patterns exist in the data?"
      - Detecting anomalies or unusual behavior
      - Understanding event frequency distributions
      - Discovering common event sequences (workflow analysis)

      **Common patterns:**
      - Frequency analysis: `pattern_type: "frequency"` → shows event type distribution
      - Sequence discovery: `pattern_type: "sequence"` → shows common event orderings
      - Anomaly detection: `pattern_type: "anomaly"` → flags unusual events
      - Scoped analysis: Add `entity_id` or `event_type` to focus on specific data

      **Pattern types explained:**
      - `frequency`: Counts events by type, sorted by occurrence. Good for understanding workload.
      - `sequence`: Finds common event progressions (A→B→C). Good for workflow analysis.
      - `anomaly`: Identifies outliers based on timing, frequency, or content deviations.

      **Performance tips:**
      - Use `since` to limit analysis window (patterns are computed on fetched events)
      - Start with frequency analysis to understand the data shape
      - Combine with `entity_id` or `event_type` to reduce data volume
      - Anomaly detection works best with sufficient historical data

      **Decision guide:**
      - Want event counts/distribution? → find_patterns with frequency
      - Want to understand workflows? → find_patterns with sequence
      - Looking for unusual activity? → find_patterns with anomaly
      - Need specific events? → query_events or semantic_search_events

      **Recommended workflow:** Call `quick_stats` first to understand data volume, \
      then run pattern detection with appropriate time windows.
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "entity_id" => %{
            type: "string",
            description: "Analyze patterns for specific entity (optional)"
          },
          "event_type" => %{
            type: "string",
            description: "Analyze patterns for specific event type (optional)"
          },
          "since" => %{
            type: "string",
            description: "Analyze patterns since this timestamp (optional)"
          },
          "pattern_type" => %{
            type: "string",
            enum: ["frequency", "sequence", "anomaly"],
            description:
              "Type of pattern to detect (frequency=event counts, sequence=event order, anomaly=unusual events)"
          }
        }
      }
    }
  end

  defp tool_compare_entities do
    %{
      name: "compare_entities",
      description: """
      Compare multiple entities to find similarities and differences in their \
      event histories. Useful for understanding behavioral differences between entities.

      **When to use this tool:**
      - Comparing user behaviors (e.g., "How do these two users differ?")
      - Analyzing cohort differences (e.g., "Premium vs free users")
      - Finding outliers by comparing against normal entities
      - Understanding why similar entities have different outcomes

      **Common patterns:**
      - Simple comparison: `entity_ids: ["user-123", "user-456"]`
      - Time-bounded: Add `timeframe: "2024-01-01T00:00:00Z"` to compare recent activity
      - Cohort analysis: Compare multiple entities (up to reasonable limits)
      - A/B analysis: Compare entities from different groups

      **What the comparison shows:**
      - Event count per entity
      - Event types used by each entity
      - Unique vs shared event types across entities
      - Activity timeline overlap

      **Performance tips:**
      - Queries all events for each entity - use timeframe to limit scope
      - Compare 2-5 entities at a time for meaningful results
      - Start with timeframe filter on active entities
      - For large-scale comparisons, use find_patterns with cohort filtering

      **Decision guide:**
      - Comparing specific entities? → compare_entities (this tool!)
      - Analyzing one entity deeply? → explain_entity
      - Looking at aggregate patterns? → find_patterns
      - Need state differences over time? → analyze_changes (single entity)
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "entity_ids" => %{
            type: "array",
            items: %{type: "string"},
            description: "Array of entity IDs to compare"
          },
          "timeframe" => %{
            type: "string",
            description: "Compare within this timeframe (ISO timestamp)"
          }
        },
        required: ["entity_ids"]
      }
    }
  end

  defp tool_event_timeline do
    %{
      name: "event_timeline",
      description: """
      Get a chronological timeline of all events for an entity, formatted for easy \
      reading. Shows the complete history in a human-friendly narrative format.

      **When to use this tool:**
      - Answering "What happened to this entity?" or "Show me the history"
      - Understanding the sequence of events leading to current state
      - Debugging: tracing through events chronologically
      - Creating audit trails or activity logs for review

      **Common patterns:**
      - Full history: `entity_id: "user-123"` (no time filters)
      - Recent activity: `entity_id: "user-123", since: "[recent date]"`
      - Specific window: `since: "[start]", until: "[end]"` for incident analysis
      - Last N events: Use since with recent timestamp

      **Output format:**
      - Numbered steps showing chronological progression
      - Each step includes: timestamp, event type, and payload preview
      - Easy to read narrative format for understanding flow

      **Performance tips:**
      - Use since/until to limit time range on active entities
      - For entities with many events, start with recent window
      - Consider query_events with limit if you only need latest few events
      - Timeline is better for understanding flow; query_events for raw data

      **Decision guide:**
      - Want to understand what happened? → event_timeline (this tool!)
      - Need raw event data for processing? → query_events
      - Need current state only? → get_snapshot
      - Need complete entity overview? → explain_entity (includes timeline)
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "entity_id" => %{type: "string", description: "Entity to get timeline for"},
          "since" => %{type: "string", description: "Timeline start time (optional)"},
          "until" => %{type: "string", description: "Timeline end time (optional)"}
        },
        required: ["entity_id"]
      }
    }
  end

  defp tool_explain_entity do
    %{
      name: "explain_entity",
      description: """
      Get a comprehensive explanation of an entity: current state, event history, \
      key changes, and timeline summary. The "tell me everything" tool.

      **When to use this tool:**
      - Answering "Tell me about entity X" or "What is this entity?"
      - Starting an investigation with no prior context about the entity
      - Getting a complete picture before diving into specifics
      - Onboarding: understanding an entity you're unfamiliar with

      **What this tool returns:**
      - Current state (snapshot)
      - Total event count and event type breakdown
      - Creation timestamp and last update time
      - Complete lifecycle timeline showing all state transitions

      **Common patterns:**
      - First contact: `entity_id: "user-123"` to understand a new entity
      - Investigation start: Get explain_entity, then drill into specifics
      - Context gathering: Use before answering detailed questions

      **Performance tips:**
      - This is a comprehensive tool - it fetches state + all events
      - More expensive than single-purpose tools like get_snapshot
      - Use for initial exploration, then switch to targeted tools
      - For very active entities, consider event_timeline with since filter

      **Decision guide:**
      - Need everything about an entity? → explain_entity (this tool!)
      - Only need current state? → get_snapshot (faster)
      - Only need history? → event_timeline
      - Only need specific events? → query_events
      - Comparing entities? → compare_entities
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "entity_id" => %{type: "string", description: "Entity ID to explain"}
        },
        required: ["entity_id"]
      }
    }
  end

  defp tool_ingest_event do
    %{
      name: "ingest_event",
      description: """
      Ingest a new event into the AllSource event store. This is the only write \
      operation - all other tools are read-only.

      **When to use this tool:**
      - Recording new facts/events as they occur
      - Creating audit trail entries
      - Storing state changes from external systems
      - Manual event injection for testing or data correction

      **Common patterns:**
      - User action: `event_type: "user.updated", entity_id: "user-123", payload: {...}`
      - System event: `event_type: "system.health_check", entity_id: "node-1", payload: {...}`
      - Audit entry: Include `metadata: {source: "manual", reason: "..."}` for context

      **Event type naming conventions:**
      - Use dot notation: `domain.action` (e.g., "user.created", "order.shipped")
      - Past tense for completed actions: "created", "updated", "deleted"
      - Be consistent across your domain model

      **Payload best practices:**
      - Include all relevant data - events are immutable once stored
      - Use consistent field names across related event types
      - Consider what queries you'll need later

      **Performance tips:**
      - Events are indexed automatically for fast retrieval
      - Vector embeddings are generated asynchronously for semantic search
      - Snapshots are updated incrementally after ingestion
      - High-volume ingestion is optimized but be mindful of burst rates

      **Important notes:**
      - Events are IMMUTABLE once stored - plan your data model carefully
      - This is the only tool that modifies data - all others are read-only
      - Include metadata for audit trails and debugging context
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "event_type" => %{type: "string", description: "Type of event (e.g., \"user.created\")"},
          "entity_id" => %{type: "string", description: "ID of the entity this event relates to"},
          "payload" => %{type: "object", description: "Event payload as JSON object"},
          "metadata" => %{type: "object", description: "Optional metadata"}
        },
        required: ["event_type", "entity_id", "payload"]
      }
    }
  end

  defp tool_get_stats do
    %{
      name: "get_stats",
      description: """
      Get comprehensive statistics about the AllSource event store. Provides \
      data-level metrics about stored events, entities, and usage patterns.

      **When to use this tool:**
      - Understanding the size and scope of the event store
      - Capacity planning and monitoring growth
      - Answering "How much data is in the system?"
      - Getting an overview before diving into specific queries

      **What this tool returns:**
      - Total event count and storage size
      - Entity count and distribution
      - Event type breakdown and frequencies
      - Timestamp ranges (oldest/newest events)
      - Index statistics and performance metrics

      **Common patterns:**
      - System overview: Call get_stats with no parameters
      - Capacity check: Review storage size and growth trends
      - Data exploration: See event types before querying

      **Performance tips:**
      - This is a lightweight operation using pre-computed statistics
      - Safe to call frequently for monitoring dashboards
      - Use this before expensive queries to understand data volume

      **Decision guide:**
      - Need data statistics? → get_stats (this tool!)
      - Need infrastructure health? → get_cluster_status
      - Need to query actual events? → query_events or search tools
      - Need patterns in data? → find_patterns
      """,
      inputSchema: %{
        type: "object",
        properties: %{}
      }
    }
  end

  defp tool_get_cluster_status do
    %{
      name: "get_cluster_status",
      description: """
      Get current cluster health and status information. Provides infrastructure-level \
      metrics about the distributed AllSource deployment.

      **When to use this tool:**
      - Checking system health before operations
      - Troubleshooting slow queries or timeouts
      - Monitoring node availability and replication status
      - Answering "Is the system healthy?" or "Why is it slow?"

      **What this tool returns:**
      - Node status (healthy/unhealthy/degraded)
      - Replication lag and sync status
      - Resource utilization (CPU, memory, disk)
      - Connection pool status
      - Recent errors or warnings

      **Common patterns:**
      - Health check: Call before important operations
      - Incident response: First step in troubleshooting
      - Monitoring: Periodic checks for dashboards

      **Performance tips:**
      - Lightweight operation - safe to call frequently
      - Use before expensive queries to ensure system is healthy
      - Check cluster status when queries are unexpectedly slow

      **Decision guide:**
      - Need infrastructure health? → get_cluster_status (this tool!)
      - Need data statistics? → get_stats
      - Troubleshooting query performance? → Check cluster_status first, then get_stats
      - System seems slow? → get_cluster_status to identify bottlenecks
      """,
      inputSchema: %{
        type: "object",
        properties: %{}
      }
    }
  end

  defp tool_semantic_search_events do
    %{
      name: "semantic_search_events",
      description: """
      Search events using natural language semantic similarity. Uses vector embeddings \
      to find events that are conceptually related to your query, even if they don't \
      contain the exact words.

      **When to use this tool:**
      - Finding events related to a concept (e.g., "user authentication failures")
      - Discovering similar events across different naming conventions
      - Exploring events when you don't know exact field names or values
      - Answering questions like "What events relate to payment processing?"

      **Tips for effective queries:**
      - Use natural language descriptions of what you're looking for
      - Be specific about the domain or context (e.g., "user onboarding" vs just "user")
      - Combine with query_events for exact matches when you know the event type

      Returns events ranked by semantic similarity score (0.0-1.0).

      **Recommended workflow:** If results are too broad, refine with `hybrid_search` \
      which adds metadata filters. If you know the entity, use `query_events` instead.

      **Example queries:**
      - "user authentication failures in the last week"
      - "payment processing errors"
      - "account suspension events"
      - "order fulfillment delays"
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "query" => %{
            type: "string",
            description:
              "Natural language search query describing what you're looking for (e.g., \"user login attempts\" or \"failed payment transactions\")"
          },
          "limit" => %{
            type: "number",
            description: "Maximum number of results to return (default: 100, max: 1000)"
          },
          "threshold" => %{
            type: "number",
            description:
              "Minimum similarity threshold from 0.0 to 1.0 (default: 0.7). Lower values return more results but with less relevance. Use 0.5 for broad exploration, 0.8+ for precise matches."
          },
          "format" => %{
            type: "string",
            enum: ["toon", "json"],
            description: "Response format: 'toon' (default, ~50% fewer tokens) or 'json'"
          }
        },
        required: ["query"]
      }
    }
  end

  defp tool_hybrid_search do
    %{
      name: "hybrid_search",
      description: """
      Perform advanced search combining semantic understanding with keyword matching \
      and metadata filters. This is the most powerful search tool, using Reciprocal \
      Rank Fusion (RRF) to blend results from multiple search strategies.

      **When to use this tool:**
      - Complex queries requiring both concept matching AND specific keywords
      - Filtering by event type, entity, or time range while searching content
      - Finding events when you know some specifics but want semantic expansion
      - Questions like "Show me user-related events from last week about authentication"

      **Search strategies:**
      - Provide `semantic_query` for concept/meaning-based search
      - Provide `keywords` for exact term matching (BM25)
      - Provide both for hybrid search (recommended for complex queries)
      - Use `filters` to narrow results by event_type, entity_id, or time range

      **Tips:**
      - Combine semantic_query with filters for best results
      - Use keywords when you know exact terms that must appear
      - Time ranges use ISO 8601 format (e.g., "2024-01-15T00:00:00Z")
      - Results are ranked by combined RRF score from all active search strategies

      Returns events with combined relevance scores and individual strategy scores.

      **Example combinations:**
      - Semantic + filter: `semantic_query: "payment issues", filters: {event_type: "payment.failed"}`
      - Keywords + time: `keywords: "timeout OR error", filters: {time_from: "2024-01-01T00:00:00Z"}`
      - Full hybrid: `semantic_query: "user churn signals", keywords: "cancel unsubscribe", filters: {entity_id: "user-123"}`
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "semantic_query" => %{
            type: "string",
            description:
              "Natural language query for semantic (meaning-based) search. Finds conceptually similar events even without exact keyword matches."
          },
          "keywords" => %{
            type: "string",
            description:
              "Keywords for BM25 text search. Supports boolean operators (AND, OR), phrases (\"exact phrase\"), and field-specific search (field:value)."
          },
          "filters" => %{
            type: "object",
            description: "Optional filters to narrow search results",
            properties: %{
              "event_type" => %{
                type: "string",
                description: "Filter by exact event type (e.g., \"user.created\", \"order.placed\")"
              },
              "entity_id" => %{
                type: "string",
                description: "Filter by exact entity ID (e.g., \"user-123\", \"order-456\")"
              },
              "time_from" => %{
                type: "string",
                description:
                  "Include events on or after this ISO timestamp (e.g., \"2024-01-15T00:00:00Z\")"
              },
              "time_to" => %{
                type: "string",
                description:
                  "Include events on or before this ISO timestamp (e.g., \"2024-01-31T23:59:59Z\")"
              }
            }
          },
          "limit" => %{
            type: "number",
            description: "Maximum number of results to return (default: 100, max: 1000)"
          },
          "format" => %{
            type: "string",
            enum: ["toon", "json"],
            description: "Response format: 'toon' (default, ~50% fewer tokens) or 'json'"
          }
        }
      }
    }
  end

  defp tool_get_query_advice do
    %{
      name: "get_query_advice",
      description: """
      Get recommendations for querying AllSource based on your use case. Returns \
      recommended tool combinations, query patterns, performance tips, and pitfalls.

      **How it works:** This is a stateless lookup table (no backend calls). It \
      returns pre-written advice for 5 use cases, with additional domain-specific \
      tips when you provide a `context` string. Context matching is keyword-based — \
      include relevant domain terms for better recommendations.

      **When to use this tool:**
      - Starting a new investigation and unsure which tools to use
      - Learning best practices for common scenarios
      - Before building complex query workflows

      **Common patterns:**
      - Audit investigation: `use_case: "audit_trail"`
      - User behavior analysis: `use_case: "user_analytics"`
      - Debugging issues: `use_case: "debugging"`
      - Regulatory compliance: `use_case: "compliance"`
      - System optimization: `use_case: "performance_analysis"`
      - Add `context` for domain-specific tips: `context: "e-commerce checkout flow"`

      **Context keywords that trigger extra advice:**
      - Payment/checkout/transaction, auth/login/session, user/customer/account
      - API/endpoint/webhook, error/failure/crash, email/notification
      - Subscription/billing, inventory/shipping, analytics/dashboard
      - GDPR/privacy, retention/churn, conversion/funnel, performance/latency

      **What this tool returns:**
      - Recommended tools and their optimal order
      - Specific query patterns with example parameters
      - Performance tips (general + context-specific)
      - Common mistakes to avoid for your use case
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "use_case" => %{
            type: "string",
            enum: [
              "audit_trail",
              "user_analytics",
              "debugging",
              "compliance",
              "performance_analysis"
            ],
            description:
              "The use case you're trying to solve: audit_trail (who did what when), user_analytics (behavior patterns), debugging (incident investigation), compliance (regulatory evidence), performance_analysis (system optimization)"
          },
          "context" => %{
            type: "string",
            description:
              "Optional additional context about your specific scenario (e.g., 'e-commerce checkout', 'user authentication', 'payment processing')"
          }
        },
        required: ["use_case"]
      }
    }
  end

  # ============================================================================
  # Quick Exploration Tool Definitions
  # ============================================================================

  defp tool_sample_events do
    %{
      name: "sample_events",
      description: """
      Get a representative sample of events for rapid data exploration without \
      scanning all events. Optimized for sub-second response times.

      **When to use this tool:**
      - Starting exploration of an unfamiliar dataset
      - Getting a quick feel for event structure and content
      - Validating data quality on a sample before deeper analysis
      - Understanding event distribution across types, entities, or time

      **Stratification options:**
      - `event_type`: Sample evenly across different event types
      - `entity_id`: Sample evenly across different entities
      - `time`: Sample evenly across the time range (temporal stratification)

      **Common patterns:**
      - Quick overview: `sample_size: 100` (default stratification)
      - By event type: `sample_size: 50, stratified_by: "event_type"`
      - By entity: `sample_size: 100, stratified_by: "entity_id"`
      - Temporal sample: `sample_size: 200, stratified_by: "time"`

      **Performance tips:**
      - This is a FAST tool - use it before expensive full scans
      - Default sample_size of 1000 balances coverage and speed
      - Stratification adds minimal overhead but improves representativeness
      - Use with entity_id/event_type filters to sample within a subset

      **Decision guide:**
      - Want quick data overview? → sample_events (this tool!)
      - Need all events? → query_events (slower but complete)
      - Need statistics only? → quick_stats (even faster)
      - Need specific search? → semantic_search_events or hybrid_search
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "sample_size" => %{
            type: "number",
            description: "Number of events to sample (default: 1000, max: 10000)"
          },
          "stratified_by" => %{
            type: "string",
            enum: ["event_type", "entity_id", "time"],
            description:
              "Stratification strategy: event_type (sample across types), entity_id (sample across entities), time (temporal sampling)"
          },
          "entity_id" => %{
            type: "string",
            description: "Optional: limit sampling to this entity"
          },
          "event_type" => %{
            type: "string",
            description: "Optional: limit sampling to this event type"
          },
          "since" => %{
            type: "string",
            description: "Optional: sample from events since this timestamp"
          },
          "until" => %{
            type: "string",
            description: "Optional: sample from events until this timestamp"
          },
          "format" => %{
            type: "string",
            enum: ["toon", "json"],
            description: "Response format: 'toon' (default, ~50% fewer tokens) or 'json'"
          }
        }
      }
    }
  end

  defp tool_quick_stats do
    %{
      name: "quick_stats",
      description: """
      Get fast approximate statistics about the event store. Trades precision \
      for speed - ideal for rapid exploration and orientation.

      **When to use this tool:**
      - Quick orientation before deeper analysis
      - Checking data volume before running expensive queries
      - Getting approximate counts without scanning all data
      - Rapid health checks on data distribution

      **Available metrics:**
      - `event_count`: Total number of events (approximate)
      - `unique_entities`: Count of distinct entity IDs
      - `event_types`: List of event types with approximate counts
      - `time_range`: Oldest and newest event timestamps

      **Common patterns:**
      - Full overview: `metric: "all"` or omit parameter
      - Just count: `metric: "event_count"`
      - Data shape: `metric: "event_types"`
      - Time bounds: `metric: "time_range"`

      **Performance tips:**
      - This is the FASTEST stats tool - uses pre-computed approximations
      - Use before get_stats for quick orientation
      - Counts may be approximate (within ~5% accuracy)
      - Ideal for answering "roughly how much data?" questions

      **Accuracy note:**
      - Counts are approximate using probabilistic data structures
      - Use get_stats for exact counts (slower)
      - Approximations are refreshed periodically, not real-time

      **Decision guide:**
      - Need quick approximation? → quick_stats (this tool!)
      - Need exact statistics? → get_stats (slower but precise)
      - Need actual events? → sample_events or query_events
      - Need system health? → get_cluster_status
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "metric" => %{
            type: "string",
            enum: ["event_count", "unique_entities", "event_types", "time_range", "all"],
            description:
              "Which metric to retrieve: event_count, unique_entities, event_types, time_range, or all (default)"
          },
          "entity_id" => %{
            type: "string",
            description: "Optional: get stats for specific entity only"
          },
          "event_type" => %{
            type: "string",
            description: "Optional: get stats for specific event type only"
          }
        }
      }
    }
  end

  # ============================================================================
  # Conversation Context Tool Definitions
  # ============================================================================

  defp tool_start_session do
    %{
      name: "start_session",
      description: """
      Start a new conversation session for multi-turn query building. Optionally \
      initialize with context from an initial query.

      **When to use this tool:**
      - Beginning a new investigation or analysis workflow
      - When you want to iteratively refine queries across multiple turns
      - Before a series of related queries that build on each other

      **Common patterns:**
      - Start fresh: `session_id: "investigation-123"`
      - Start with entity: `session_id: "user-analysis", entity_id: "user-456"`
      - Start with time range: `session_id: "audit-jan", since: "2024-01-01", until: "2024-01-31"`

      **How multi-turn context works:**
      1. Start session with initial filters
      2. Use refine_query to add filters or change scope
      3. Each refinement builds on previous context
      4. Use get_session_context to see accumulated state

      **Example workflow:**
      ```
      Turn 1: start_session(session_id: "s1", entity_type: "user", since: "yesterday")
      Turn 2: refine_query(session_id: "s1", filters: {tier: "premium"})
      Turn 3: refine_query(session_id: "s1", event_type: "purchase.completed")
      ```

      **Performance tips:**
      - Sessions auto-expire after 30 minutes of inactivity
      - Use descriptive session_ids for easier tracking
      - Clear old sessions if you're starting fresh on a topic

      **Decision guide:**
      - Starting fresh investigation? → start_session (this tool!)
      - Already have a session? → refine_query
      - Need to see current context? → get_session_context
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "session_id" => %{
            type: "string",
            description:
              "Unique identifier for this conversation session (e.g., 'user-investigation-123')"
          },
          "entity_id" => %{
            type: "string",
            description: "Optional initial entity ID to focus on"
          },
          "entity_type" => %{
            type: "string",
            description: "Optional entity type (e.g., 'user', 'order', 'payment')"
          },
          "event_type" => %{
            type: "string",
            description: "Optional initial event type filter"
          },
          "since" => %{
            type: "string",
            description: "Optional start time filter (ISO timestamp)"
          },
          "until" => %{
            type: "string",
            description: "Optional end time filter (ISO timestamp)"
          },
          "semantic_query" => %{
            type: "string",
            description: "Optional initial semantic search context"
          }
        },
        required: ["session_id"]
      }
    }
  end

  defp tool_refine_query do
    %{
      name: "refine_query",
      description: """
      Refine the query context for an existing session. New filters and parameters \
      merge with existing context, enabling iterative query building.

      **When to use this tool:**
      - Adding filters to narrow down results: "Filter to premium tier"
      - Changing time range: "Look at last week instead"
      - Adding entity focus: "Focus on user-123"
      - Building comparison queries: "Compare with last month"

      **Common patterns:**
      - Add filter: `refine_query(session_id: "s1", filters: {status: "active"})`
      - Narrow time: `refine_query(session_id: "s1", since: "2024-01-15")`
      - Add entity: `refine_query(session_id: "s1", entity_id: "user-789")`
      - Add event type: `refine_query(session_id: "s1", event_type: "user.updated")`

      **Merge behavior:**
      - `entity_id` / `entity_ids`: Accumulates (adds to existing list)
      - `event_type` / `event_types`: Accumulates
      - `filters`: Merges (new keys added, existing keys updated)
      - `since` / `until` / `as_of`: Replaces (newest wins)
      - `semantic_query` / `keywords`: Replaces

      **To replace instead of accumulate:**
      Use `replace_entity_ids: true` to clear existing entities

      **Performance tips:**
      - Each refinement updates last_accessed, extending session life
      - Query history is preserved (last 10 refinements)
      - Use get_session_context to verify current state before querying

      **Decision guide:**
      - Narrowing down results? → refine_query (this tool!)
      - Starting fresh? → start_session
      - Ready to execute? → Use query_events, semantic_search, etc. with session context
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "session_id" => %{
            type: "string",
            description: "The session ID to refine"
          },
          "entity_id" => %{
            type: "string",
            description: "Add an entity ID to focus on (accumulates with existing)"
          },
          "entity_ids" => %{
            type: "array",
            items: %{type: "string"},
            description: "Add multiple entity IDs (accumulates with existing)"
          },
          "event_type" => %{
            type: "string",
            description: "Add an event type filter (accumulates with existing)"
          },
          "event_types" => %{
            type: "array",
            items: %{type: "string"},
            description: "Add multiple event types (accumulates with existing)"
          },
          "filters" => %{
            type: "object",
            description: "Additional metadata filters to apply (merges with existing)"
          },
          "since" => %{
            type: "string",
            description: "Update start time filter (replaces existing)"
          },
          "until" => %{
            type: "string",
            description: "Update end time filter (replaces existing)"
          },
          "as_of" => %{
            type: "string",
            description: "Update as_of time for time-travel queries (replaces existing)"
          },
          "semantic_query" => %{
            type: "string",
            description: "Update semantic search query (replaces existing)"
          },
          "keywords" => %{
            type: "string",
            description: "Update keyword search (replaces existing)"
          },
          "replace_entity_ids" => %{
            type: "boolean",
            description: "If true, replace existing entity_ids instead of accumulating"
          }
        },
        required: ["session_id"]
      }
    }
  end

  defp tool_get_session_context do
    %{
      name: "get_session_context",
      description: """
      Get the current accumulated context for a conversation session. Shows all \
      filters, entities, and query parameters that have been built up.

      **When to use this tool:**
      - Before executing a query, to verify the accumulated context
      - To understand what filters are currently active
      - To debug unexpected query results
      - To show the user what context you're working with

      **What this tool returns:**
      - Current query parameters (ready to pass to other tools)
      - Session metadata (created, last accessed, query count)
      - Query history (last 10 refinements)
      - Last result summary (if available)

      **Common patterns:**
      - Check context: `get_session_context(session_id: "s1")`
      - Verify before query: Check context, then call query_events with returned params

      **Performance tips:**
      - Lightweight operation, safe to call frequently
      - Use to verify context before expensive queries
      - Helps catch accumulated filters you might have forgotten

      **Decision guide:**
      - Need to see current state? → get_session_context (this tool!)
      - Need to modify context? → refine_query
      - Ready to execute with context? → Use returned params with query tools
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "session_id" => %{
            type: "string",
            description: "The session ID to retrieve context for"
          },
          "include_history" => %{
            type: "boolean",
            description: "Include query refinement history (default: true)"
          }
        },
        required: ["session_id"]
      }
    }
  end

  # ============================================================================
  # Event Management Tool Definitions (v2.0)
  # ============================================================================

  defp tool_delete_events do
    %{
      name: "delete_events",
      description: """
      Soft delete events with a complete audit trail. Events are marked as deleted \
      but preserved for compliance and recovery purposes.

      **When to use this tool:**
      - GDPR/CCPA "right to be forgotten" compliance
      - Removing PII or sensitive data while maintaining audit trail
      - Cleaning up erroneous or test events
      - Regulatory compliance requiring data deletion with proof

      **How soft delete works:**
      - Original events remain in storage but are marked as deleted
      - A deletion tombstone event is created for audit purposes
      - Deleted events are excluded from normal queries
      - Events can be restored using restore_events tool

      **Common patterns:**
      - Delete by entity: `entity_id: "user-123"` - deletes all events for an entity
      - Delete by type: `entity_id: "user-123", event_type: "user.pii_updated"`
      - Delete time range: `entity_id: "user-123", since: "2024-01-01", until: "2024-01-31"`
      - Delete specific events: `event_ids: ["evt-1", "evt-2", "evt-3"]`

      **Important notes:**
      - This is a SOFT DELETE - data can be recovered with restore_events
      - A reason is REQUIRED for audit compliance
      - Deletion is recorded as a system event for the audit trail
      - For hard delete (permanent), contact system administrator

      **Performance tips:**
      - Large deletions are processed in batches
      - Use dry_run: true first to preview what will be deleted
      - For entity-wide deletion, prefer entity_id over many event_ids
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "entity_id" => %{
            type: "string",
            description: "Delete events for this entity ID"
          },
          "event_ids" => %{
            type: "array",
            items: %{type: "string"},
            description: "Specific event IDs to delete"
          },
          "event_type" => %{
            type: "string",
            description: "Delete only events of this type (requires entity_id)"
          },
          "since" => %{
            type: "string",
            description: "Delete events since this timestamp (requires entity_id)"
          },
          "until" => %{
            type: "string",
            description: "Delete events until this timestamp (requires entity_id)"
          },
          "reason" => %{
            type: "string",
            description: "REQUIRED: Reason for deletion (for audit trail)"
          },
          "dry_run" => %{
            type: "boolean",
            description: "Preview deletion without executing (default: false)"
          }
        },
        required: ["reason"]
      }
    }
  end

  defp tool_archive_events do
    %{
      name: "archive_events",
      description: """
      Move events to cold storage archive for cost optimization while maintaining \
      accessibility. Archived events can be restored when needed.

      **When to use this tool:**
      - Moving old/inactive entity events to cheaper storage
      - Compliance archival with retention requirements
      - Reducing active storage costs for historical data
      - Preparing data for long-term cold storage

      **How archival works:**
      - Events are marked with archived status and timestamp
      - Archived events are excluded from normal queries by default
      - Data remains accessible via restore_events or explicit archive queries
      - Archive metadata tracks when, why, and by whom archived
      - All archival operations are recorded in the audit trail

      **Common patterns:**
      - Archive inactive entity: `entity_id: "user-123", reason: "Account closed"`
      - Archive old events: `entity_id: "user-123", older_than: "2023-01-01"`
      - Archive by type: `entity_id: "user-123", event_type: "debug.log"`

      **Performance tips:**
      - Archive in batches for large entities
      - Use dry_run to preview before archiving
      - Archive during low-traffic periods for large operations
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "entity_id" => %{
            type: "string",
            description: "Archive events for this entity"
          },
          "event_type" => %{
            type: "string",
            description: "Archive only events of this type"
          },
          "older_than" => %{
            type: "string",
            description: "Archive events older than this ISO timestamp"
          },
          "reason" => %{
            type: "string",
            description: "Reason for archival (for audit trail)"
          },
          "retention_days" => %{
            type: "number",
            description: "Minimum days to retain in archive (default: 365)"
          },
          "dry_run" => %{
            type: "boolean",
            description: "Preview archival without executing (default: false)"
          }
        },
        required: ["entity_id", "reason"]
      }
    }
  end

  defp tool_restore_events do
    %{
      name: "restore_events",
      description: """
      Restore previously deleted or archived events back to active state. \
      Full audit trail is maintained for all restore operations.

      **When to use this tool:**
      - Recovering accidentally deleted events
      - Restoring archived data for active use
      - Compliance scenarios requiring data recovery
      - Reversing a soft delete operation

      **How restore works:**
      - Deleted/archived status is removed from events
      - Events become visible in normal queries again
      - Restore operation is recorded in audit trail
      - Original event data is preserved unchanged

      **Common patterns:**
      - Restore deleted entity: `entity_id: "user-123", status: "deleted"`
      - Restore from archive: `entity_id: "user-123", status: "archived"`
      - Restore specific events: `event_ids: ["evt-1", "evt-2"]`
      - Restore time range: `entity_id: "user-123", since: "2024-01-01"`

      **Important notes:**
      - Can only restore soft-deleted events (not hard-deleted)
      - Restoring does not modify original event data
      - Restoration is audited for compliance

      **Performance tips:**
      - Use dry_run: true to preview what will be restored before executing
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "entity_id" => %{
            type: "string",
            description: "Restore events for this entity"
          },
          "event_ids" => %{
            type: "array",
            items: %{type: "string"},
            description: "Specific event IDs to restore"
          },
          "status" => %{
            type: "string",
            enum: ["deleted", "archived", "all"],
            description: "Restore events with this status (default: all)"
          },
          "since" => %{
            type: "string",
            description: "Restore events since this timestamp"
          },
          "until" => %{
            type: "string",
            description: "Restore events until this timestamp"
          },
          "reason" => %{
            type: "string",
            description: "Reason for restoration (for audit trail)"
          },
          "dry_run" => %{
            type: "boolean",
            description: "Preview restoration without executing (default: false)"
          }
        },
        required: ["reason"]
      }
    }
  end

  defp tool_export_events do
    %{
      name: "export_events",
      description: """
      Export events to various formats for backup, migration, analysis, or sharing. \
      Supports JSON, CSV, and Parquet formats with optional compression.

      **When to use this tool:**
      - Creating backups of event data
      - Migrating data to another system
      - Exporting for external analysis (BI tools, spreadsheets)
      - Sharing data with third parties
      - Compliance data portability requests

      **Supported formats:**
      - `json`: Standard JSON array of events (default)
      - `jsonl`: JSON Lines format (one event per line, good for streaming)
      - `csv`: Comma-separated values (flattened structure)
      - `parquet`: Columnar format (efficient for analytics)

      **Common patterns:**
      - Export entity: `entity_id: "user-123", format: "json"`
      - Export for analytics: `entity_id: "user-123", format: "parquet"`
      - Export time range: `since: "2024-01-01", until: "2024-12-31", format: "jsonl"`
      - Export with compression: `entity_id: "user-123", compress: true`

      **Performance tips:**
      - Use JSONL for large exports (streaming-friendly)
      - Use Parquet for analytics workloads
      - Set reasonable limits for very large datasets
      - Consider time-based partitioning for huge exports
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "entity_id" => %{
            type: "string",
            description: "Export events for this entity"
          },
          "event_type" => %{
            type: "string",
            description: "Export only events of this type"
          },
          "since" => %{
            type: "string",
            description: "Export events since this timestamp"
          },
          "until" => %{
            type: "string",
            description: "Export events until this timestamp"
          },
          "format" => %{
            type: "string",
            enum: ["json", "jsonl", "csv", "parquet"],
            description: "Export format (default: json)"
          },
          "compress" => %{
            type: "boolean",
            description: "Compress output with gzip (default: false)"
          },
          "include_metadata" => %{
            type: "boolean",
            description: "Include event metadata in export (default: true)"
          },
          "limit" => %{
            type: "number",
            description: "Maximum number of events to export"
          }
        }
      }
    }
  end

  defp tool_import_events do
    %{
      name: "import_events",
      description: """
      Bulk import events from external sources. Supports validation, deduplication, \
      and entity ID mapping for migration scenarios.

      **When to use this tool:**
      - Migrating from another event store
      - Importing historical data
      - Restoring from backup
      - Bulk loading test data
      - Data portability imports

      **Supported formats:**
      - `json`: JSON array of events
      - `jsonl`: JSON Lines format (one event per line)
      - `csv`: CSV with headers matching event fields

      **Common patterns:**
      - Import from JSON: `data: [...events...], format: "json"`
      - Import with mapping: `data: [...], entity_id_prefix: "imported-"`
      - Validate only: `data: [...], validate_only: true`
      - Skip duplicates: `data: [...], skip_duplicates: true`

      **Event structure required:**
      ```json
      {
        "event_type": "user.created",
        "entity_id": "user-123",
        "payload": {...},
        "metadata": {...},  // optional
        "timestamp": "..."  // optional, uses current time if missing
      }
      ```

      **Important notes:**
      - Events are validated against schema if registered
      - Timestamps can be preserved or regenerated
      - Duplicate detection uses event_type + entity_id + timestamp
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "data" => %{
            type: "array",
            items: %{type: "object"},
            description: "Array of events to import"
          },
          "format" => %{
            type: "string",
            enum: ["json", "jsonl", "csv"],
            description: "Import format (default: json)"
          },
          "entity_id_prefix" => %{
            type: "string",
            description: "Prefix to add to all entity IDs (for namespacing)"
          },
          "preserve_timestamps" => %{
            type: "boolean",
            description: "Keep original timestamps (default: true)"
          },
          "skip_duplicates" => %{
            type: "boolean",
            description: "Skip events that already exist (default: true)"
          },
          "validate_only" => %{
            type: "boolean",
            description: "Validate without importing (default: false)"
          },
          "batch_size" => %{
            type: "number",
            description: "Batch size for import (default: 100)"
          }
        },
        required: ["data"]
      }
    }
  end

  defp tool_clone_entity do
    %{
      name: "clone_entity",
      description: """
      Create a deep copy of an entity by duplicating all its events with a new \
      entity ID. Useful for creating test data or entity templates.

      **When to use this tool:**
      - Creating test entities based on production data (sanitized)
      - Duplicating entity as a template for new instances
      - Creating backup copy before major modifications
      - Forking an entity for A/B testing scenarios

      **How cloning works:**
      - All events for source entity are copied
      - New entity ID is assigned to all copied events
      - Timestamps can be preserved or reset
      - Metadata tracks the clone relationship

      **Common patterns:**
      - Simple clone: `source_entity_id: "user-123", new_entity_id: "user-123-copy"`
      - Clone with new timestamps: `source_entity_id: "user-123", reset_timestamps: true`
      - Clone time range: `source_entity_id: "user-123", since: "2024-01-01"`
      - Clone specific types: `source_entity_id: "user-123", event_types: ["user.profile_updated"]`

      **Important notes:**
      - Original entity is unchanged
      - Clone operation is recorded in audit trail
      - Consider data privacy when cloning (PII handling)

      **Performance tips:**
      - Use dry_run: true to preview what will be cloned before executing
      - For large entities, consider cloning specific event_types or time ranges
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "source_entity_id" => %{
            type: "string",
            description: "Entity ID to clone from"
          },
          "new_entity_id" => %{
            type: "string",
            description: "Entity ID for the clone (auto-generated if not provided)"
          },
          "event_types" => %{
            type: "array",
            items: %{type: "string"},
            description: "Clone only these event types (default: all)"
          },
          "since" => %{
            type: "string",
            description: "Clone events since this timestamp"
          },
          "until" => %{
            type: "string",
            description: "Clone events until this timestamp"
          },
          "reset_timestamps" => %{
            type: "boolean",
            description: "Reset timestamps to current time (default: false)"
          },
          "sanitize_pii" => %{
            type: "boolean",
            description: "Attempt to sanitize PII fields (default: false)"
          },
          "dry_run" => %{
            type: "boolean",
            description: "Preview clone without executing (default: false)"
          }
        },
        required: ["source_entity_id"]
      }
    }
  end

  defp tool_merge_entities do
    %{
      name: "merge_entities",
      description: """
      Combine event streams from multiple entities into a single unified entity. \
      Useful for merging duplicate records or consolidating related entities.

      **When to use this tool:**
      - Merging duplicate user accounts
      - Consolidating related entities (e.g., guest -> registered user)
      - Combining split records after data cleanup
      - Creating aggregate entity from components

      **How merge works:**
      - Events from all source entities are combined
      - Events are reattributed to the target entity
      - Timeline is preserved (events interleaved by timestamp)
      - Merge operation is recorded for audit trail

      **Common patterns:**
      - Merge duplicates: `source_entity_ids: ["user-old", "user-guest"], target_entity_id: "user-main"`
      - Merge with archival: `..., archive_sources: true`
      - Preview merge: `..., dry_run: true`

      **Conflict handling:**
      - Events are merged chronologically
      - No event deduplication (all events preserved)
      - Source entities can be archived or deleted after merge

      **Important notes:**
      - This is a complex operation - use dry_run first
      - Consider impact on projections and read models
      - Source entities can optionally be archived after merge
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "source_entity_ids" => %{
            type: "array",
            items: %{type: "string"},
            description: "Entity IDs to merge from"
          },
          "target_entity_id" => %{
            type: "string",
            description: "Entity ID to merge into (can be new or existing)"
          },
          "archive_sources" => %{
            type: "boolean",
            description: "Archive source entities after merge (default: false)"
          },
          "delete_sources" => %{
            type: "boolean",
            description: "Soft delete source entities after merge (default: false)"
          },
          "reason" => %{
            type: "string",
            description: "Reason for merge (for audit trail)"
          },
          "dry_run" => %{
            type: "boolean",
            description: "Preview merge without executing (default: false)"
          }
        },
        required: ["source_entity_ids", "target_entity_id", "reason"]
      }
    }
  end

  defp tool_split_entity do
    %{
      name: "split_entity",
      description: """
      Partition an entity's event stream into multiple new entities based on \
      criteria like event type, time range, or custom rules.

      **When to use this tool:**
      - Splitting a monolithic entity into domain-specific entities
      - Separating entity by time periods (e.g., fiscal years)
      - Extracting specific event types to new entity
      - Correcting data modeling mistakes

      **How split works:**
      - Events matching criteria are copied to new entity
      - Original events can be kept, archived, or deleted
      - Split operation is recorded for audit trail
      - Multiple splits can be done in one operation

      **Common patterns:**
      - Split by type: `entity_id: "user-123", splits: [{event_types: ["billing.*"], new_entity_id: "billing-123"}]`
      - Split by time: `entity_id: "org-1", splits: [{since: "2024-01-01", new_entity_id: "org-1-2024"}]`
      - Split and archive: `..., archive_split_events: true`

      **Important notes:**
      - Use dry_run to preview the split
      - Consider projection/read model impact
      - Original entity can retain or lose split events
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "source_entity_id" => %{
            type: "string",
            description: "Entity ID to split from"
          },
          "splits" => %{
            type: "array",
            items: %{
              type: "object",
              properties: %{
                "new_entity_id" => %{type: "string"},
                "event_types" => %{type: "array", items: %{type: "string"}},
                "since" => %{type: "string"},
                "until" => %{type: "string"}
              },
              required: ["new_entity_id"]
            },
            description: "Array of split definitions"
          },
          "archive_split_events" => %{
            type: "boolean",
            description: "Archive split events in source entity (default: false)"
          },
          "delete_split_events" => %{
            type: "boolean",
            description: "Soft delete split events in source entity (default: false)"
          },
          "reason" => %{
            type: "string",
            description: "Reason for split (for audit trail)"
          },
          "dry_run" => %{
            type: "boolean",
            description: "Preview split without executing (default: false)"
          }
        },
        required: ["source_entity_id", "splits", "reason"]
      }
    }
  end

  # ============================================================================
  # Tool Handlers
  # ============================================================================

  @doc false
  def handle_query_events(args, state, format) do
    params = Map.take(args, ["entity_id", "event_type", "as_of", "since", "until", "limit"])

    case state.backend.query_events(params) do
      {:ok, data} ->
        count = Map.get(data, "count", 0)
        summary = "📊 Found #{count} events"
        formatted_data = ToonEncoder.format_response(data, format)

        {:ok,
         %{
           content: [
             %{type: "text", text: summary},
             %{type: "text", text: formatted_data}
           ]
         }}

      {:error, reason} ->
        {:error, "Failed to query events: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_reconstruct_state(args, state, format) do
    entity_id = Map.fetch!(args, "entity_id")
    as_of = Map.get(args, "as_of")

    case state.backend.reconstruct_state(entity_id, as_of) do
      {:ok, state_data} ->
        event_count = Map.get(state_data, "event_count", 0)
        last_updated = Map.get(state_data, "last_updated", "unknown")
        as_of_str = as_of || "current"

        summary = """
        🔄 Reconstructed state for "#{entity_id}"
        📅 As of: #{as_of_str}
        📊 Events processed: #{event_count}
        ⏰ Last updated: #{last_updated}\
        """

        formatted_data = ToonEncoder.format_response(state_data, format)

        {:ok,
         %{
           content: [
             %{type: "text", text: summary},
             %{type: "text", text: formatted_data}
           ]
         }}

      {:error, reason} ->
        {:error, "Failed to reconstruct state: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_get_snapshot(args, state, format) do
    entity_id = Map.fetch!(args, "entity_id")

    case state.backend.get_snapshot(entity_id) do
      {:ok, snapshot} ->
        summary = "⚡ Fast snapshot for \"#{entity_id}\""
        formatted_data = ToonEncoder.format_response(snapshot, format)

        {:ok,
         %{
           content: [
             %{type: "text", text: summary},
             %{type: "text", text: formatted_data}
           ]
         }}

      {:error, reason} ->
        {:error, "Failed to get snapshot: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_analyze_changes(args, state, format) do
    entity_id = Map.fetch!(args, "entity_id")
    from_time = Map.fetch!(args, "from_time")
    to_time = Map.get(args, "to_time")

    # Get state at from_time
    case state.backend.reconstruct_state(entity_id, from_time) do
      {:ok, before_state} ->
        # Get state at to_time (or current)
        case state.backend.reconstruct_state(entity_id, to_time) do
          {:ok, after_state} ->
            before_state_map = Map.get(before_state, "current_state", %{})
            after_state_map = Map.get(after_state, "current_state", %{})

            changes = calculate_diff(before_state_map, after_state_map)

            summary = """
            🔍 Change Analysis for "#{entity_id}"
            📅 From: #{from_time}
            📅 To: #{to_time || "now"}
            ➕ Added fields: #{length(changes.added)}
            ✏️  Modified fields: #{length(changes.modified)}
            ➖ Removed fields: #{length(changes.removed)}\
            """

            formatted_data = ToonEncoder.format_response(changes, format)

            {:ok,
             %{
               content: [
                 %{type: "text", text: summary},
                 %{type: "text", text: formatted_data}
               ]
             }}

          {:error, reason} ->
            {:error, "Failed to get after state: #{inspect(reason)}"}
        end

      {:error, reason} ->
        {:error, "Failed to get before state: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_find_patterns(args, state, format) do
    params = Map.take(args, ["entity_id", "event_type", "since"])

    case state.backend.query_events(params) do
      {:ok, data} ->
        events = Map.get(data, "events", [])
        pattern_type = Map.get(args, "pattern_type")

        analysis = analyze_patterns(events, pattern_type)

        summary = """
        🔎 Pattern Analysis
        📊 Events analyzed: #{length(events)}
        🎯 Pattern type: #{pattern_type || "all"}\
        """

        formatted_data = ToonEncoder.format_response(analysis, format)

        {:ok,
         %{
           content: [
             %{type: "text", text: summary},
             %{type: "text", text: formatted_data}
           ]
         }}

      {:error, reason} ->
        {:error, "Failed to find patterns: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_compare_entities(args, state, format) do
    entity_ids = Map.fetch!(args, "entity_ids")
    timeframe = Map.get(args, "timeframe")

    comparisons =
      Enum.map(entity_ids, fn id ->
        params = if timeframe, do: Map.put(%{}, "since", timeframe), else: %{}
        params = Map.put(params, "entity_id", id)

        case state.backend.query_events(params) do
          {:ok, data} ->
            events = Map.get(data, "events", [])
            event_types = events |> Enum.map(&Map.get(&1, "event_type")) |> Enum.uniq()

            %{
              entity_id: id,
              event_count: Map.get(data, "count", 0),
              event_types: event_types
            }

          {:error, _reason} ->
            %{
              entity_id: id,
              event_count: 0,
              event_types: []
            }
        end
      end)

    summary = """
    🔬 Entity Comparison
    📊 Entities compared: #{length(entity_ids)}
    ⏰ Timeframe: #{timeframe || "all time"}\
    """

    formatted_data = ToonEncoder.format_response(comparisons, format)

    {:ok,
     %{
       content: [
         %{type: "text", text: summary},
         %{type: "text", text: formatted_data}
       ]
     }}
  end

  @doc false
  def handle_event_timeline(args, state, format) do
    entity_id = Map.fetch!(args, "entity_id")
    params = Map.take(args, ["since", "until"])
    params = Map.put(params, "entity_id", entity_id)

    case state.backend.query_events(params) do
      {:ok, data} ->
        events = Map.get(data, "events", [])

        timeline =
          events
          |> Enum.with_index(1)
          |> Enum.map(fn {event, index} ->
            payload_preview =
              event
              |> Map.get("payload", %{})
              |> Jason.encode!()
              |> String.slice(0, 100)

            %{
              step: index,
              timestamp: Map.get(event, "timestamp"),
              event_type: Map.get(event, "event_type"),
              summary: "#{Map.get(event, "event_type")} - #{payload_preview}..."
            }
          end)

        summary = """
        📅 Timeline for "#{entity_id}"
        📊 Events: #{length(events)}
        ⏰ Period: #{Map.get(args, "since", "start")} to #{Map.get(args, "until", "now")}\
        """

        formatted_data = ToonEncoder.format_response(timeline, format)

        {:ok,
         %{
           content: [
             %{type: "text", text: summary},
             %{type: "text", text: formatted_data}
           ]
         }}

      {:error, reason} ->
        {:error, "Failed to get timeline: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_explain_entity(args, state, format) do
    entity_id = Map.fetch!(args, "entity_id")

    # Get current state
    case state.backend.reconstruct_state(entity_id, nil) do
      {:ok, state_data} ->
        # Get all events
        case state.backend.query_events(%{"entity_id" => entity_id}) do
          {:ok, events_data} ->
            events = Map.get(events_data, "events", [])
            event_types = events |> Enum.map(&Map.get(&1, "event_type")) |> Enum.uniq()

            explanation = %{
              entity_id: entity_id,
              current_state: Map.get(state_data, "current_state", %{}),
              total_events: length(events),
              event_types: event_types,
              # Default to an empty map: an entity can reconstruct from a
              # snapshot while its events are archived away, and Map.get/3 on
              # the nil that List.first/1 then returns raises BadMapError (#229).
              created_at: events |> List.first(%{}) |> Map.get("timestamp"),
              last_updated: Map.get(state_data, "last_updated"),
              lifecycle:
                Enum.map(events, fn e ->
                  %{
                    when: Map.get(e, "timestamp"),
                    what: Map.get(e, "event_type")
                  }
                end)
            }

            summary = """
            📋 Entity Explanation: "#{entity_id}"
            🔹 Total Events: #{length(events)}
            🔹 Event Types: #{length(event_types)}
            🔹 Created: #{events |> List.first(%{}) |> Map.get("timestamp", "unknown")}
            🔹 Last Updated: #{Map.get(state_data, "last_updated", "unknown")}\
            """

            formatted_data = ToonEncoder.format_response(explanation, format)

            {:ok,
             %{
               content: [
                 %{type: "text", text: summary},
                 %{type: "text", text: formatted_data}
               ]
             }}

          {:error, reason} ->
            {:error, "Failed to get events: #{inspect(reason)}"}
        end

      {:error, reason} ->
        {:error, "Failed to get state: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_ingest_event(args, state, format) do
    event_data = %{
      "event_type" => Map.fetch!(args, "event_type"),
      "entity_id" => Map.fetch!(args, "entity_id"),
      "payload" => Map.fetch!(args, "payload"),
      "metadata" => Map.get(args, "metadata")
    }

    case state.backend.ingest_event(event_data) do
      {:ok, result} ->
        event_id = Map.get(result, "event_id", "unknown")
        timestamp = Map.get(result, "timestamp", "unknown")

        summary = """
        ✅ Event ingested successfully
        🆔 Event ID: #{event_id}
        ⏰ Timestamp: #{timestamp}

        """

        formatted_data = ToonEncoder.format_response(result, format)
        text = summary <> formatted_data

        {:ok,
         %{
           content: [
             %{
               type: "text",
               text: text
             }
           ]
         }}

      {:error, reason} ->
        {:error, "Failed to ingest event: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_get_stats(state, format) do
    case state.backend.get_stats() do
      {:ok, stats} ->
        summary = "📊 AllSource Statistics\n\n"
        formatted_data = ToonEncoder.format_response(stats, format)
        text = summary <> formatted_data

        {:ok,
         %{
           content: [
             %{
               type: "text",
               text: text
             }
           ]
         }}

      {:error, reason} ->
        {:error, "Failed to get stats: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_get_cluster_status(state, format) do
    case state.backend.get_cluster_status() do
      {:ok, status} ->
        summary = "🎯 Cluster Status\n\n"
        formatted_data = ToonEncoder.format_response(status, format)
        text = summary <> formatted_data

        {:ok,
         %{
           content: [
             %{
               type: "text",
               text: text
             }
           ]
         }}

      {:error, reason} ->
        {:error, "Failed to get cluster status: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_semantic_search_events(args, state, format) do
    query = Map.fetch!(args, "query")
    limit = Map.get(args, "limit", 100)
    threshold = Map.get(args, "threshold", 0.7)

    params = %{
      "query" => query,
      "limit" => limit,
      "threshold" => threshold
    }

    case state.backend.semantic_search(params) do
      {:ok, data} ->
        results = Map.get(data, "results", [])
        count = length(results)

        summary = """
        🔍 Semantic Search Results
        📝 Query: "#{query}"
        📊 Found: #{count} matching events
        🎯 Threshold: #{threshold}\
        """

        formatted_data = ToonEncoder.format_response(data, format)

        {:ok,
         %{
           content: [
             %{type: "text", text: summary},
             %{type: "text", text: formatted_data}
           ]
         }}

      {:error, reason} ->
        {:error, "Failed to perform semantic search: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_hybrid_search(args, state, format) do
    semantic_query = Map.get(args, "semantic_query")
    keywords = Map.get(args, "keywords")
    filters = Map.get(args, "filters", %{})
    limit = Map.get(args, "limit", 100)

    # Build the search params
    params = %{
      "limit" => limit
    }

    params =
      if semantic_query, do: Map.put(params, "semantic_query", semantic_query), else: params

    params = if keywords, do: Map.put(params, "keywords", keywords), else: params

    # Add filters if present
    params =
      if map_size(filters) > 0 do
        Map.put(params, "filters", filters)
      else
        params
      end

    case state.backend.hybrid_search(params) do
      {:ok, data} ->
        results = Map.get(data, "results", [])
        count = length(results)

        # Build a descriptive summary of what was searched
        search_desc =
          [
            if(semantic_query, do: "Semantic: \"#{semantic_query}\"", else: nil),
            if(keywords, do: "Keywords: \"#{keywords}\"", else: nil)
          ]
          |> Enum.reject(&is_nil/1)
          |> Enum.join(" + ")

        search_desc = if search_desc == "", do: "No query provided", else: search_desc

        # Build filter description
        filter_desc =
          [
            if(Map.get(filters, "event_type"),
              do: "event_type=#{Map.get(filters, "event_type")}",
              else: nil
            ),
            if(Map.get(filters, "entity_id"),
              do: "entity_id=#{Map.get(filters, "entity_id")}",
              else: nil
            ),
            if(Map.get(filters, "time_from"),
              do: "from=#{Map.get(filters, "time_from")}",
              else: nil
            ),
            if(Map.get(filters, "time_to"), do: "to=#{Map.get(filters, "time_to")}", else: nil)
          ]
          |> Enum.reject(&is_nil/1)
          |> Enum.join(", ")

        filter_line = if filter_desc != "", do: "🔧 Filters: #{filter_desc}\n", else: ""

        summary = """
        🔀 Hybrid Search Results
        📝 #{search_desc}
        #{filter_line}📊 Found: #{count} matching events\
        """

        formatted_data = ToonEncoder.format_response(data, format)

        {:ok,
         %{
           content: [
             %{type: "text", text: summary},
             %{type: "text", text: formatted_data}
           ]
         }}

      {:error, reason} ->
        {:error, "Failed to perform hybrid search: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_get_query_advice(args, _format) do
    use_case = Map.fetch!(args, "use_case")
    context = Map.get(args, "context")

    advice = get_advice_for_use_case(use_case, context)

    # Format advice as structured text (more readable than TOON for this use case)
    text = format_advice_as_text(use_case, context, advice)

    {:ok,
     %{
       content: [
         %{
           type: "text",
           text: text
         }
       ]
     }}
  end

  # ============================================================================
  # Quick Exploration Handlers
  # ============================================================================

  @doc false
  def handle_sample_events(args, state, format) do
    sample_size = min(Map.get(args, "sample_size", 1000), 10000)
    stratified_by = Map.get(args, "stratified_by")

    # Build base query params
    base_params = Map.take(args, ["entity_id", "event_type", "since", "until"])

    # For sampling, we fetch more than needed and then subsample
    # This allows us to do stratification on the client side
    fetch_limit = min(sample_size * 3, 30000)
    params = Map.put(base_params, "limit", fetch_limit)

    case state.backend.query_events(params) do
      {:ok, data} ->
        events = Map.get(data, "events", [])

        # Apply stratified sampling
        sampled_events = apply_stratified_sampling(events, sample_size, stratified_by)

        # Build statistics about the sample
        sample_stats = calculate_sample_stats(sampled_events, stratified_by)

        summary = """
        🎲 Sample Events (#{length(sampled_events)} of ~#{length(events)} available)
        📊 Sample size requested: #{sample_size}
        🎯 Stratification: #{stratified_by || "random"}

        #{format_sample_stats(sample_stats)}
        """

        sampled_data = %{
          "events" => sampled_events,
          "sample_size" => length(sampled_events),
          "total_available" => length(events),
          "stratified_by" => stratified_by,
          "sample_stats" => sample_stats
        }

        formatted_data = ToonEncoder.format_response(sampled_data, format)
        text = summary <> formatted_data

        {:ok,
         %{
           content: [
             %{
               type: "text",
               text: text
             }
           ]
         }}

      {:error, reason} ->
        {:error, "Failed to sample events: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_quick_stats(args, state, format) do
    metric = Map.get(args, "metric", "all")
    entity_id = Map.get(args, "entity_id")
    event_type = Map.get(args, "event_type")

    # Build query params for filtered stats
    params =
      %{}
      |> maybe_put("entity_id", entity_id)
      |> maybe_put("event_type", event_type)
      |> Map.put("limit", 10000)

    # Get events for approximation (or use pre-computed stats if available)
    result =
      if entity_id || event_type do
        # Filtered query - need to compute from events
        state.backend.query_events(params)
      else
        # Unfiltered - can use get_stats for pre-computed values
        state.backend.get_stats()
      end

    case result do
      {:ok, data} ->
        stats = compute_quick_stats(data, metric, entity_id, event_type)

        summary = """
        ⚡ Quick Stats#{if entity_id, do: " for entity: #{entity_id}", else: ""}#{if event_type, do: " for type: #{event_type}", else: ""}
        📊 Metric: #{metric}
        ⚠️  Note: These are approximate values for speed

        """

        formatted_data = ToonEncoder.format_response(stats, format)
        text = summary <> formatted_data

        {:ok,
         %{
           content: [
             %{
               type: "text",
               text: text
             }
           ]
         }}

      {:error, reason} ->
        {:error, "Failed to get quick stats: #{inspect(reason)}"}
    end
  end

  # ============================================================================
  # Quick Exploration Helpers
  # ============================================================================

  defp apply_stratified_sampling(events, sample_size, nil) do
    # Random sampling without stratification
    events
    |> Enum.shuffle()
    |> Enum.take(sample_size)
  end

  defp apply_stratified_sampling(events, sample_size, "event_type") do
    # Group by event type and sample proportionally
    grouped = Enum.group_by(events, &Map.get(&1, "event_type", "unknown"))
    total = length(events)

    if total == 0 do
      []
    else
      # Calculate samples per group proportionally
      grouped
      |> Enum.flat_map(fn {_type, group_events} ->
        proportion = length(group_events) / total
        group_sample_size = max(1, round(sample_size * proportion))

        group_events
        |> Enum.shuffle()
        |> Enum.take(group_sample_size)
      end)
      |> Enum.shuffle()
      |> Enum.take(sample_size)
    end
  end

  defp apply_stratified_sampling(events, sample_size, "entity_id") do
    # Group by entity and sample proportionally
    grouped = Enum.group_by(events, &Map.get(&1, "entity_id", "unknown"))
    total = length(events)

    if total == 0 do
      []
    else
      grouped
      |> Enum.flat_map(fn {_entity, group_events} ->
        proportion = length(group_events) / total
        group_sample_size = max(1, round(sample_size * proportion))

        group_events
        |> Enum.shuffle()
        |> Enum.take(group_sample_size)
      end)
      |> Enum.shuffle()
      |> Enum.take(sample_size)
    end
  end

  defp apply_stratified_sampling(events, sample_size, "time") do
    # Temporal stratification - sample evenly across time range
    sorted = Enum.sort_by(events, &Map.get(&1, "timestamp", ""))
    total = length(sorted)

    if total == 0 do
      []
    else
      # Calculate stride to sample evenly across the timeline
      stride = max(1, div(total, sample_size))

      sorted
      |> Enum.with_index()
      |> Enum.filter(fn {_event, idx} -> rem(idx, stride) == 0 end)
      |> Enum.map(fn {event, _idx} -> event end)
      |> Enum.take(sample_size)
    end
  end

  defp calculate_sample_stats(events, stratified_by) do
    event_types =
      events
      |> Enum.map(&Map.get(&1, "event_type", "unknown"))
      |> Enum.frequencies()

    entity_ids =
      events
      |> Enum.map(&Map.get(&1, "entity_id", "unknown"))
      |> Enum.uniq()
      |> length()

    timestamps = Enum.map(events, &Map.get(&1, "timestamp", ""))
    min_time = Enum.min(timestamps, fn -> nil end)
    max_time = Enum.max(timestamps, fn -> nil end)

    %{
      event_type_distribution: event_types,
      unique_entities: entity_ids,
      time_range: %{earliest: min_time, latest: max_time},
      stratification: stratified_by || "random"
    }
  end

  defp format_sample_stats(stats) do
    type_count = map_size(stats.event_type_distribution)

    top_types =
      stats.event_type_distribution
      |> Enum.sort_by(fn {_k, v} -> -v end)
      |> Enum.take(5)
      |> Enum.map(fn {type, count} -> "  • #{type}: #{count}" end)
      |> Enum.join("\n")

    """
    📈 Sample Distribution:
    • Event types: #{type_count}
    • Unique entities: #{stats.unique_entities}
    • Time range: #{stats.time_range.earliest || "N/A"} to #{stats.time_range.latest || "N/A"}

    🔝 Top event types in sample:
    #{top_types}
    """
  end

  defp compute_quick_stats(data, metric, entity_id, event_type) do
    # Check if we have pre-computed stats or raw events
    events = Map.get(data, "events", [])
    has_events = length(events) > 0

    base_stats =
      if has_events do
        # Compute from events
        compute_stats_from_events(events)
      else
        # Use pre-computed stats
        %{
          event_count: Map.get(data, "total_events", Map.get(data, "event_count", 0)),
          # Core's StoreStats calls this total_entities; only the derived
          # fallback emits unique_entities. Accept either, else this read 0
          # against a real /api/v1/stats response.
          unique_entities: Map.get(data, "unique_entities") || Map.get(data, "total_entities") || 0,
          event_types: Map.get(data, "event_types", %{}),
          time_range: %{
            earliest: Map.get(data, "oldest_event"),
            latest: Map.get(data, "newest_event")
          }
        }
      end

    # Add approximation note
    base_stats = Map.put(base_stats, :approximate, true)

    # Filter to requested metric if not "all"
    case metric do
      "all" ->
        base_stats

      "event_count" ->
        %{event_count: base_stats.event_count, approximate: true}

      "unique_entities" ->
        %{unique_entities: base_stats.unique_entities, approximate: true}

      "event_types" ->
        %{event_types: base_stats.event_types, approximate: true}

      "time_range" ->
        %{time_range: base_stats.time_range, approximate: true}

      _ ->
        base_stats
    end
    |> maybe_add_filter_context(entity_id, event_type)
  end

  defp compute_stats_from_events(events) do
    event_types =
      events
      |> Enum.map(&Map.get(&1, "event_type", "unknown"))
      |> Enum.frequencies()

    unique_entities =
      events
      |> Enum.map(&Map.get(&1, "entity_id", "unknown"))
      |> Enum.uniq()
      |> length()

    timestamps = Enum.map(events, &Map.get(&1, "timestamp", ""))

    %{
      event_count: length(events),
      unique_entities: unique_entities,
      event_types: event_types,
      time_range: %{
        earliest: Enum.min(timestamps, fn -> nil end),
        latest: Enum.max(timestamps, fn -> nil end)
      }
    }
  end

  defp maybe_add_filter_context(stats, nil, nil), do: stats

  defp maybe_add_filter_context(stats, entity_id, event_type) do
    context = %{}
    context = if entity_id, do: Map.put(context, :filtered_by_entity, entity_id), else: context
    context = if event_type, do: Map.put(context, :filtered_by_type, event_type), else: context
    Map.put(stats, :filter_context, context)
  end

  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)

  defp format_bytes(0), do: "0 B"
  defp format_bytes(bytes) when bytes < 1024, do: "#{bytes} B"
  defp format_bytes(bytes) when bytes < 1_048_576, do: "#{Float.round(bytes / 1024, 1)} KB"
  defp format_bytes(bytes) when bytes < 1_073_741_824, do: "#{Float.round(bytes / 1_048_576, 1)} MB"
  defp format_bytes(bytes), do: "#{Float.round(bytes / 1_073_741_824, 1)} GB"

  # ============================================================================
  # Conversation Context Handlers
  # ============================================================================

  @doc false
  def handle_start_session(args, _state, _format) do
    session_id = Map.fetch!(args, "session_id")

    # Build initial context from provided args
    initial_context =
      args
      |> Map.drop(["session_id"])
      |> Enum.reduce(%{}, fn
        {"entity_id", v}, acc when is_binary(v) -> Map.put(acc, :entity_id, v)
        {"entity_type", v}, acc when is_binary(v) -> Map.put(acc, :entity_type, v)
        {"event_type", v}, acc when is_binary(v) -> Map.put(acc, :event_type, v)
        {"since", v}, acc when is_binary(v) -> Map.put(acc, :since, v)
        {"until", v}, acc when is_binary(v) -> Map.put(acc, :until, v)
        {"semantic_query", v}, acc when is_binary(v) -> Map.put(acc, :semantic_query, v)
        _, acc -> acc
      end)

    # Create or get the session
    case ConversationContext.get_or_create_session(context_server(), session_id) do
      {:ok, session} ->
        # Apply initial context if provided
        {:ok, updated_session} =
          if map_size(initial_context) > 0 do
            ConversationContext.update_session(context_server(), session_id, initial_context)
          else
            {:ok, session}
          end

        # Build response
        query_params = session_to_display_params(updated_session)

        text = """
        🎯 Session Started: "#{session_id}"
        📅 Created: #{format_datetime(updated_session.created_at)}

        #{if map_size(initial_context) > 0, do: "📋 Initial Context:\n#{format_context_summary(initial_context)}", else: "📋 No initial context (session is empty)"}

        💡 Next steps:
        - Use refine_query to add filters: `refine_query(session_id: "#{session_id}", filters: {...})`
        - Use get_session_context to see current state
        - Use query parameters with query_events, semantic_search, etc.

        📊 Current Query Parameters:
        #{format_query_params(query_params)}
        """

        {:ok,
         %{
           content: [
             %{
               type: "text",
               text: text
             }
           ]
         }}

      {:error, reason} ->
        {:error, "Failed to create session: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_refine_query(args, _state, _format) do
    session_id = Map.fetch!(args, "session_id")

    # Build refinement context from provided args
    refinement =
      args
      |> Map.drop(["session_id"])
      |> Enum.reduce(%{}, fn
        {"entity_id", v}, acc when is_binary(v) -> Map.put(acc, :entity_id, v)
        {"entity_ids", v}, acc when is_list(v) -> Map.put(acc, :entity_ids, v)
        {"entity_type", v}, acc when is_binary(v) -> Map.put(acc, :entity_type, v)
        {"event_type", v}, acc when is_binary(v) -> Map.put(acc, :event_type, v)
        {"event_types", v}, acc when is_list(v) -> Map.put(acc, :event_types, v)
        {"filters", v}, acc when is_map(v) -> Map.put(acc, :filters, v)
        {"since", v}, acc when is_binary(v) -> Map.put(acc, :since, v)
        {"until", v}, acc when is_binary(v) -> Map.put(acc, :until, v)
        {"as_of", v}, acc when is_binary(v) -> Map.put(acc, :as_of, v)
        {"semantic_query", v}, acc when is_binary(v) -> Map.put(acc, :semantic_query, v)
        {"keywords", v}, acc when is_binary(v) -> Map.put(acc, :keywords, v)
        {"replace_entity_ids", v}, acc when is_boolean(v) -> Map.put(acc, :replace_entity_ids, v)
        _, acc -> acc
      end)

    if map_size(refinement) == 0 do
      {:error, "No refinement parameters provided. Specify at least one filter to add."}
    else
      case ConversationContext.build_query(context_server(), session_id, refinement) do
        {:ok, query_params} ->
          # Get updated session for display
          {:ok, session} = ConversationContext.get_session(context_server(), session_id)

          text = """
          ✏️  Query Refined for session "#{session_id}"
          ⏰ Last accessed: #{format_datetime(session.last_accessed)}
          📊 Total refinements: #{length(session.query_history)}

          📋 Applied Refinement:
          #{format_context_summary(refinement)}

          📊 Updated Query Parameters:
          #{format_query_params(query_params)}

          💡 Use these parameters with:
          - query_events(#{format_inline_params(query_params)})
          - semantic_search_events(#{format_inline_params(query_params)})
          - hybrid_search(#{format_inline_params(query_params)})
          """

          {:ok,
           %{
             content: [
               %{
                 type: "text",
                 text: text
               }
             ]
           }}

        {:error, reason} ->
          {:error, "Failed to refine query: #{inspect(reason)}"}
      end
    end
  end

  @doc false
  def handle_get_session_context(args, _state, _format) do
    session_id = Map.fetch!(args, "session_id")
    include_history = Map.get(args, "include_history", true)

    case ConversationContext.get_session(context_server(), session_id) do
      {:ok, session} ->
        query_params = session_to_display_params(session)

        history_section =
          if include_history and length(session.query_history) > 0 do
            history_lines =
              session.query_history
              |> Enum.with_index(1)
              |> Enum.map(fn {entry, idx} ->
                refinement_summary =
                  entry.refinement
                  |> Enum.map(fn {k, v} -> "#{k}: #{inspect(v)}" end)
                  |> Enum.join(", ")

                "   #{idx}. #{format_datetime(entry.timestamp)} - #{refinement_summary}"
              end)
              |> Enum.join("\n")

            """

            📜 Query History (most recent first):
            #{history_lines}
            """
          else
            ""
          end

        result_section =
          if session.last_result_summary do
            """

            📊 Last Result Summary:
            #{inspect(session.last_result_summary, pretty: true)}
            """
          else
            ""
          end

        text = """
        🔍 Session Context: "#{session_id}"

        📅 Created: #{format_datetime(session.created_at)}
        ⏰ Last accessed: #{format_datetime(session.last_accessed)}
        📊 Query refinements: #{length(session.query_history)}

        📋 Current Context:
        #{format_session_context(session)}

        📊 Query Parameters (ready to use):
        #{format_query_params(query_params)}
        #{history_section}#{result_section}
        💡 Copy these parameters to query_events, semantic_search_events, or hybrid_search
        """

        {:ok,
         %{
           content: [
             %{
               type: "text",
               text: text
             }
           ]
         }}

      {:error, :not_found} ->
        {:error, "Session '#{session_id}' not found. Use start_session to create one."}
    end
  end

  # ============================================================================
  # Context Formatting Helpers
  # ============================================================================

  defp session_to_display_params(session) do
    params = %{}

    params =
      case session.entity_ids do
        [single_id] -> Map.put(params, "entity_id", single_id)
        ids when length(ids) > 1 -> Map.put(params, "entity_ids", ids)
        _ -> params
      end

    params =
      case session.event_types do
        [single_type] -> Map.put(params, "event_type", single_type)
        types when length(types) > 1 -> Map.put(params, "event_types", types)
        _ -> params
      end

    params = if session.time_since, do: Map.put(params, "since", session.time_since), else: params
    params = if session.time_until, do: Map.put(params, "until", session.time_until), else: params
    params = if session.time_as_of, do: Map.put(params, "as_of", session.time_as_of), else: params

    params =
      if session.semantic_query,
        do: Map.put(params, "semantic_query", session.semantic_query),
        else: params

    params = if session.keywords, do: Map.put(params, "keywords", session.keywords), else: params

    params =
      if map_size(session.filters) > 0 do
        Map.put(params, "filters", session.filters)
      else
        params
      end

    params
  end

  defp format_datetime(nil), do: "N/A"

  defp format_datetime(%DateTime{} = dt) do
    Calendar.strftime(dt, "%Y-%m-%d %H:%M:%S UTC")
  end

  defp format_context_summary(context) when is_map(context) do
    context
    |> Enum.map(fn {k, v} -> "   • #{k}: #{inspect(v)}" end)
    |> Enum.join("\n")
  end

  defp format_query_params(params) when map_size(params) == 0 do
    "   (empty - no filters applied)"
  end

  defp format_query_params(params) do
    params
    |> Enum.map(fn {k, v} -> "   #{k}: #{Jason.encode!(v)}" end)
    |> Enum.join("\n")
  end

  defp format_inline_params(params) when map_size(params) == 0 do
    ""
  end

  defp format_inline_params(params) do
    params
    |> Enum.take(3)
    |> Enum.map(fn {k, v} -> "#{k}: #{Jason.encode!(v)}" end)
    |> Enum.join(", ")
    |> then(fn s ->
      if map_size(params) > 3, do: s <> ", ...", else: s
    end)
  end

  defp format_session_context(session) do
    lines = []

    lines =
      if length(session.entity_ids) > 0 do
        lines ++ ["   • Entity IDs: #{inspect(session.entity_ids)}"]
      else
        lines
      end

    lines =
      if session.entity_type do
        lines ++ ["   • Entity Type: #{session.entity_type}"]
      else
        lines
      end

    lines =
      if length(session.event_types) > 0 do
        lines ++ ["   • Event Types: #{inspect(session.event_types)}"]
      else
        lines
      end

    lines =
      if session.time_since do
        lines ++ ["   • Since: #{session.time_since}"]
      else
        lines
      end

    lines =
      if session.time_until do
        lines ++ ["   • Until: #{session.time_until}"]
      else
        lines
      end

    lines =
      if session.time_as_of do
        lines ++ ["   • As Of: #{session.time_as_of}"]
      else
        lines
      end

    lines =
      if session.semantic_query do
        lines ++ ["   • Semantic Query: \"#{session.semantic_query}\""]
      else
        lines
      end

    lines =
      if session.keywords do
        lines ++ ["   • Keywords: \"#{session.keywords}\""]
      else
        lines
      end

    lines =
      if map_size(session.filters) > 0 do
        lines ++ ["   • Filters: #{inspect(session.filters)}"]
      else
        lines
      end

    if length(lines) == 0 do
      "   (no context - session is empty)"
    else
      Enum.join(lines, "\n")
    end
  end

  defp format_advice_as_text(use_case, context, advice) do
    header = """
    💡 Query Advice for: #{format_use_case(use_case)}
    #{if context, do: "📋 Context: #{context}\n", else: ""}
    """

    tools_section = format_recommended_tools(advice.recommended_tools)
    patterns_section = format_query_patterns(advice.query_patterns)
    tips_section = format_performance_tips(advice.performance_tips)
    pitfalls_section = format_common_pitfalls(advice.common_pitfalls)

    header <> tools_section <> patterns_section <> tips_section <> pitfalls_section
  end

  defp format_recommended_tools(tools) do
    tool_lines =
      tools
      |> Enum.sort_by(& &1.priority)
      |> Enum.map(fn t -> "   #{t.priority}. #{t.tool} - #{t.purpose}" end)
      |> Enum.join("\n")

    """
    📦 RECOMMENDED TOOLS (in order):
    #{tool_lines}

    """
  end

  defp format_query_patterns(patterns) do
    pattern_lines =
      patterns
      |> Enum.map(fn p -> "   • #{p.pattern}\n     → #{p.approach}" end)
      |> Enum.join("\n")

    """
    🔍 QUERY PATTERNS:
    #{pattern_lines}

    """
  end

  defp format_performance_tips(tips) do
    tip_lines =
      tips
      |> Enum.map(fn t -> "   • #{t}" end)
      |> Enum.join("\n")

    """
    ⚡ PERFORMANCE TIPS:
    #{tip_lines}

    """
  end

  defp format_common_pitfalls(pitfalls) do
    pitfall_lines =
      pitfalls
      |> Enum.map(fn p -> "   ⚠️  #{p}" end)
      |> Enum.join("\n")

    """
    🚫 COMMON PITFALLS TO AVOID:
    #{pitfall_lines}
    """
  end

  defp format_use_case(use_case) do
    case use_case do
      "audit_trail" -> "Audit Trail Investigation"
      "user_analytics" -> "User Analytics"
      "debugging" -> "Debugging & Incident Response"
      "compliance" -> "Compliance & Regulatory"
      "performance_analysis" -> "Performance Analysis"
      _ -> use_case
    end
  end

  defp get_advice_for_use_case(use_case, context) do
    base_advice = advice_database()[use_case]

    # Add context-specific tips if context is provided
    context_tips =
      if context do
        generate_context_tips(use_case, context)
      else
        []
      end

    Map.update(base_advice, :performance_tips, [], fn tips ->
      tips ++ context_tips
    end)
  end

  defp generate_context_tips(use_case, context) do
    context_lower = String.downcase(context)

    domain_tips = generate_domain_tips(context_lower)
    use_case_tips = generate_use_case_context_tips(use_case, context_lower)
    analytics_tips = generate_analytics_honesty_tips(context_lower)

    domain_tips ++ use_case_tips ++ analytics_tips
  end

  defp generate_domain_tips(context_lower) do
    tips = []

    tips =
      if String.contains?(context_lower, ["payment", "checkout", "transaction", "order", "purchase"]) do
        tips ++
          [
            "For payment flows, always include correlation IDs in your queries",
            "Consider using hybrid_search with 'payment' or 'transaction' semantic queries",
            "Query by entity_id to trace a specific order's complete lifecycle",
            "Use event_timeline to see the exact sequence: cart → checkout → payment → fulfillment"
          ]
      else
        tips
      end

    tips =
      if String.contains?(context_lower, ["auth", "login", "session", "oauth", "sso"]) do
        tips ++
          [
            "Authentication events often span multiple entities — use semantic_search_events",
            "Consider querying by IP address or session ID patterns in metadata",
            "Use find_patterns with pattern_type: 'anomaly' to detect unusual login patterns",
            "For failed login investigations, query by event_type with time bounds to spot brute force"
          ]
      else
        tips
      end

    tips =
      if String.contains?(context_lower, ["user", "customer", "account", "profile"]) do
        tips ++
          [
            "Use explain_entity first to understand a user's complete journey",
            "Use compare_entities to spot behavioral differences between user groups",
            "For activity trends, use cohort_analysis (returns frequency data per time bucket)"
          ]
      else
        tips
      end

    tips =
      if String.contains?(context_lower, ["api", "endpoint", "request", "response", "webhook"]) do
        tips ++
          [
            "Use correlation_analysis to check if specific API calls tend to precede errors",
            "Query by event_type to isolate specific endpoint activity",
            "Use forecast_events for basic volume trending (note: linear regression only)"
          ]
      else
        tips
      end

    tips =
      if String.contains?(context_lower, ["error", "failure", "exception", "crash", "bug"]) do
        tips ++
          [
            "Use event_timeline with time bounds around the incident for chronological trace",
            "Use reconstruct_state to see entity state at moment of failure",
            "Use semantic_search_events with error description to find similar past incidents",
            "Use correlation_analysis to check if specific events precede failures"
          ]
      else
        tips
      end

    tips =
      if String.contains?(context_lower, ["email", "notification", "message", "sms", "push"]) do
        tips ++
          [
            "Track notification delivery chain: sent → delivered → opened → clicked",
            "Use correlation_analysis to measure time between send and engagement events",
            "Query by entity_id to see all notifications received by a specific user"
          ]
      else
        tips
      end

    tips =
      if String.contains?(context_lower, ["subscription", "plan", "billing", "invoice", "renewal"]) do
        tips ++
          [
            "Use event_timeline per entity to trace subscription lifecycle events",
            "For revenue analysis, query value events and sum amounts in your application",
            "Note: ltv_calculation returns summary stats only — compute actual LTV from raw events"
          ]
      else
        tips
      end

    tips =
      if String.contains?(context_lower, [
           "inventory",
           "stock",
           "warehouse",
           "shipping",
           "fulfillment"
         ]) do
        tips ++
          [
            "Use reconstruct_state to see inventory levels at any point in time",
            "Track fulfillment pipelines with event_timeline per order entity",
            "Use find_patterns to detect common fulfillment sequences"
          ]
      else
        tips
      end

    tips
  end

  defp generate_use_case_context_tips(use_case, context_lower) do
    case use_case do
      "audit_trail" ->
        cond do
          String.contains?(context_lower, ["gdpr", "privacy", "deletion", "data request"]) ->
            [
              "For GDPR data subject requests, use query_events with entity_id to find all related events",
              "Use reconstruct_state to document what data existed at time of request"
            ]

          String.contains?(context_lower, ["permission", "role", "access", "admin"]) ->
            [
              "Filter by event_type patterns like 'role.*' or 'permission.*' to trace access changes",
              "Use analyze_changes to compare state before and after permission modifications"
            ]

          true ->
            []
        end

      "user_analytics" ->
        cond do
          String.contains?(context_lower, ["retention", "churn", "engagement"]) ->
            [
              "Note: churn_prediction and segment_analysis return summary stats, not actual scores",
              "For real retention analysis, query events per entity and compute activity gaps yourself",
              "Use cohort_analysis for time-bucketed frequency data as a starting point"
            ]

          String.contains?(context_lower, ["conversion", "funnel", "onboarding"]) ->
            [
              "Note: path_analysis returns summary stats, not actual funnel conversion rates",
              "For real funnel analysis, query events per entity and compute step-to-step drop-off",
              "Use correlation_analysis to check if specific events precede conversion"
            ]

          true ->
            []
        end

      "debugging" ->
        cond do
          String.contains?(context_lower, ["performance", "latency", "slow", "timeout"]) ->
            [
              "Use query_events with tight time bounds to isolate slow periods",
              "Check event timestamps to measure processing time between steps",
              "Use forecast_events to see if volume spikes correlate with performance issues"
            ]

          String.contains?(context_lower, ["data", "corruption", "inconsistent", "mismatch"]) ->
            [
              "Use reconstruct_state at multiple timestamps to find when data diverged",
              "Use event_timeline to trace the exact sequence of modifications",
              "Use analyze_changes to compare expected vs actual state"
            ]

          true ->
            []
        end

      _ ->
        []
    end
  end

  defp generate_analytics_honesty_tips(context_lower) do
    tips = []

    tips =
      if String.contains?(context_lower, [
           "analytics",
           "analysis",
           "insights",
           "metrics",
           "dashboard"
         ]) do
        tips ++
          [
            "Note: cohort_analysis, segment_analysis, path_analysis, attribution_analysis, churn_prediction, and ltv_calculation are basic wrappers around Core's summary/frequency endpoints",
            "For sophisticated analytics, query raw events and compute in your application or BI tool",
            "correlation_analysis and forecast_events have real (though basic) computation"
          ]
      else
        tips
      end

    tips
  end

  defp advice_database do
    %{
      "audit_trail" => %{
        recommended_tools: [
          %{
            tool: "event_timeline",
            purpose: "Get chronological view of all actions",
            priority: 1
          },
          %{
            tool: "query_events",
            purpose: "Filter by specific entity, time range, or event type",
            priority: 2
          },
          %{
            tool: "reconstruct_state",
            purpose: "See exact state at any point in time",
            priority: 3
          },
          %{
            tool: "analyze_changes",
            purpose: "Compare state before/after an incident",
            priority: 4
          }
        ],
        query_patterns: [
          %{
            pattern: "Who modified entity X?",
            approach:
              "event_timeline(entity_id: 'X') → look for modification events with actor metadata"
          },
          %{
            pattern: "What was the state before incident?",
            approach: "reconstruct_state(entity_id: 'X', as_of: '[incident_time - 1h]')"
          },
          %{
            pattern: "All changes in time window",
            approach:
              "query_events(since: '[start]', until: '[end]') → then analyze_changes for each entity"
          },
          %{
            pattern: "Track specific action type",
            approach: "query_events(event_type: 'user.updated', since: '[date]')"
          }
        ],
        performance_tips: [
          "Always use time bounds (since/until) to limit result sets",
          "Use event_timeline for human-readable audit logs",
          "Use query_events with limit for initial exploration, then narrow down",
          "Cache reconstruct_state results if you need multiple point-in-time comparisons"
        ],
        common_pitfalls: [
          "Forgetting to include time bounds on high-volume entities",
          "Using reconstruct_state when event_timeline would be more appropriate",
          "Not checking metadata for actor/source information",
          "Querying without limit on entities with thousands of events"
        ]
      },
      "user_analytics" => %{
        recommended_tools: [
          %{
            tool: "find_patterns",
            purpose: "Discover behavior patterns and sequences",
            priority: 1
          },
          %{
            tool: "compare_entities",
            purpose: "Compare behavior across user cohorts",
            priority: 2
          },
          %{
            tool: "semantic_search_events",
            purpose: "Find events by behavior description",
            priority: 3
          },
          %{
            tool: "explain_entity",
            purpose: "Deep dive into individual user journey",
            priority: 4
          }
        ],
        query_patterns: [
          %{
            pattern: "Common user workflows",
            approach: "find_patterns(pattern_type: 'sequence') → analyze event progressions"
          },
          %{
            pattern: "Compare user segments",
            approach:
              "compare_entities(entity_ids: ['user-A', 'user-B', ...]) → find behavioral differences"
          },
          %{
            pattern: "Find engagement patterns",
            approach:
              "semantic_search_events(query: 'user engagement interaction') → discover related events"
          },
          %{
            pattern: "User lifecycle analysis",
            approach: "explain_entity(entity_id: 'user-X') → full journey overview"
          }
        ],
        performance_tips: [
          "Start with find_patterns(pattern_type: 'frequency') to understand event distribution",
          "Use timeframes in compare_entities to focus on recent behavior",
          "Limit semantic searches to avoid large result sets during exploration",
          "Use get_stats first to understand overall data volume"
        ],
        common_pitfalls: [
          "Analyzing too many users at once - start with representative samples",
          "Not accounting for seasonality in behavior patterns",
          "Missing the 'sequence' pattern type which shows workflow progressions",
          "Forgetting that compare_entities returns event type overlap, not detailed comparison"
        ]
      },
      "debugging" => %{
        recommended_tools: [
          %{
            tool: "event_timeline",
            purpose: "Trace exact sequence of events leading to issue",
            priority: 1
          },
          %{
            tool: "reconstruct_state",
            purpose: "See state at moment of failure",
            priority: 2
          },
          %{
            tool: "semantic_search_events",
            purpose: "Find related errors or anomalies",
            priority: 3
          },
          %{
            tool: "find_patterns",
            purpose: "Detect anomalies or unusual sequences",
            priority: 4
          },
          %{
            tool: "get_cluster_status",
            purpose: "Check system health during incident window",
            priority: 5
          }
        ],
        query_patterns: [
          %{
            pattern: "Trace error back to root cause",
            approach:
              "event_timeline(entity_id: 'X', until: '[error_time]') → follow events backward"
          },
          %{
            pattern: "Find related failures",
            approach: "semantic_search_events(query: 'error failure exception [domain]')"
          },
          %{
            pattern: "State at time of failure",
            approach: "reconstruct_state(entity_id: 'X', as_of: '[error_time - 1s]')"
          },
          %{
            pattern: "Detect anomalous patterns",
            approach: "find_patterns(pattern_type: 'anomaly', since: '[incident_window_start]')"
          }
        ],
        performance_tips: [
          "Always bound queries to incident time window",
          "Use get_cluster_status first to rule out infrastructure issues",
          "Start with broad semantic search, then narrow with specific filters",
          "Use hybrid_search to combine error keywords with semantic context"
        ],
        common_pitfalls: [
          "Not checking cluster health first - infrastructure issues mask application bugs",
          "Starting too narrow - use semantic search for exploration before filtering",
          "Ignoring metadata which often contains error context and stack traces",
          "Forgetting to check events just BEFORE the error timestamp"
        ]
      },
      "compliance" => %{
        recommended_tools: [
          %{
            tool: "query_events",
            purpose: "Retrieve complete event records with full audit trail",
            priority: 1
          },
          %{
            tool: "reconstruct_state",
            purpose: "Prove state at specific regulatory checkpoint",
            priority: 2
          },
          %{
            tool: "analyze_changes",
            purpose: "Document all changes in compliance period",
            priority: 3
          },
          %{
            tool: "event_timeline",
            purpose: "Generate human-readable audit reports",
            priority: 4
          }
        ],
        query_patterns: [
          %{
            pattern: "Data retention compliance",
            approach: "query_events(entity_id: 'X') → verify all events exist for retention period"
          },
          %{
            pattern: "Prove state at audit date",
            approach:
              "reconstruct_state(entity_id: 'X', as_of: '[audit_date]') → immutable proof of state"
          },
          %{
            pattern: "Document all modifications",
            approach:
              "analyze_changes(entity_id: 'X', from_time: '[period_start]', to_time: '[period_end]')"
          },
          %{
            pattern: "Generate audit report",
            approach:
              "event_timeline(entity_id: 'X', since: '[start]', until: '[end]') → chronological report"
          }
        ],
        performance_tips: [
          "Use JSON format for compliance reports (format: 'json') - more suitable for archival",
          "Query complete time ranges without limits for audit completeness",
          "Include metadata in queries - it often contains required actor/source information",
          "Use get_stats to verify event counts match expected totals"
        ],
        common_pitfalls: [
          "Using limits on compliance queries - you need complete records",
          "Not preserving original timestamps - always use stored event times",
          "Forgetting to document the query parameters used for reproducibility",
          "Missing metadata fields that contain required audit information (actor, source, IP)"
        ]
      },
      "performance_analysis" => %{
        recommended_tools: [
          %{
            tool: "get_stats",
            purpose: "Understand data volume and growth patterns",
            priority: 1
          },
          %{
            tool: "get_cluster_status",
            purpose: "Monitor system health and resource usage",
            priority: 2
          },
          %{
            tool: "find_patterns",
            purpose: "Identify high-frequency events or hotspots",
            priority: 3
          },
          %{
            tool: "query_events",
            purpose: "Measure query performance on specific patterns",
            priority: 4
          }
        ],
        query_patterns: [
          %{
            pattern: "Identify write hotspots",
            approach: "find_patterns(pattern_type: 'frequency') → find high-volume event types"
          },
          %{
            pattern: "Monitor system health",
            approach: "get_cluster_status() → check node health, replication lag, resource usage"
          },
          %{
            pattern: "Data growth analysis",
            approach: "get_stats() → review event counts, storage size, index statistics"
          },
          %{
            pattern: "Query performance baseline",
            approach: "query_events with increasing limits → measure response time scaling"
          }
        ],
        performance_tips: [
          "Run get_stats before and after major ingestion to track growth",
          "Monitor get_cluster_status during peak usage periods",
          "Use find_patterns(pattern_type: 'frequency') to identify optimization targets",
          "Test query patterns with limits before running unbounded queries"
        ],
        common_pitfalls: [
          "Not establishing baselines - always compare against previous measurements",
          "Ignoring cluster_status warnings about replication lag or resource pressure",
          "Focusing only on query performance without considering ingestion rates",
          "Not using get_snapshot for current state (much faster than reconstruct_state)"
        ]
      }
    }
  end

  # ============================================================================
  # Helper Functions
  # ============================================================================

  defp calculate_diff(before_map, after_map) do
    added =
      after_map
      |> Map.keys()
      |> Enum.filter(fn key -> not Map.has_key?(before_map, key) end)

    removed =
      before_map
      |> Map.keys()
      |> Enum.filter(fn key -> not Map.has_key?(after_map, key) end)

    modified =
      before_map
      |> Map.keys()
      |> Enum.filter(fn key ->
        Map.has_key?(after_map, key) and Map.get(before_map, key) != Map.get(after_map, key)
      end)
      |> Enum.map(fn key ->
        %{
          field: key,
          before_value: Map.get(before_map, key),
          after_value: Map.get(after_map, key)
        }
      end)

    %{
      added: added,
      modified: modified,
      removed: removed
    }
  end

  defp analyze_patterns(events, pattern_type) do
    analysis = %{}

    analysis =
      if pattern_type == "frequency" or is_nil(pattern_type) do
        frequency_map =
          events
          |> Enum.reduce(%{}, fn event, acc ->
            event_type = Map.get(event, "event_type", "unknown")
            Map.update(acc, event_type, 1, &(&1 + 1))
          end)

        frequency =
          frequency_map
          |> Enum.map(fn {type, count} -> %{event_type: type, count: count} end)
          |> Enum.sort_by(& &1.count, :desc)

        Map.put(analysis, :frequency, frequency)
      else
        analysis
      end

    analysis =
      if pattern_type == "sequence" or is_nil(pattern_type) do
        sequences =
          events
          |> Enum.take(10)
          |> Enum.chunk_every(2, 1, :discard)
          |> Enum.map(fn [e1, e2] ->
            "#{Map.get(e1, "event_type")} → #{Map.get(e2, "event_type")}"
          end)

        Map.put(analysis, :common_sequences, sequences)
      else
        analysis
      end

    analysis
  end

  # ============================================================================
  # Event Management Tool Handlers (v2.0)
  # ============================================================================

  @doc false
  def handle_delete_events(args, state, format) do
    reason = Map.fetch!(args, "reason")
    dry_run = Map.get(args, "dry_run", false)

    # Build query params to find events to delete
    query_params = build_delete_query_params(args)

    case state.backend.query_events(query_params) do
      {:ok, data} ->
        events = Map.get(data, "events", [])
        count = length(events)

        if dry_run do
          # Preview mode - show what would be deleted
          event_ids = Enum.map(events, &Map.get(&1, "id", Map.get(&1, "event_id")))

          result = %{
            dry_run: true,
            events_to_delete: count,
            event_ids: Enum.take(event_ids, 100),
            reason: reason
          }

          text = """
          🔍 Delete Preview (DRY RUN)
          📊 Events that would be deleted: #{count}
          📝 Reason: #{reason}

          #{if count > 100, do: "⚠️  Showing first 100 of #{count} event IDs", else: ""}

          #{ToonEncoder.format_response(result, format)}

          💡 Remove dry_run: true to execute the deletion
          """

          {:ok, %{content: [%{type: "text", text: text}]}}
        else
          # Execute soft delete by ingesting tombstone events
          deleted_count = soft_delete_events(events, reason, state)

          result = %{
            deleted: true,
            events_deleted: deleted_count,
            reason: reason,
            timestamp: DateTime.utc_now() |> DateTime.to_iso8601()
          }

          text = """
          🗑️  Events Soft Deleted
          📊 Events deleted: #{deleted_count}
          📝 Reason: #{reason}
          ⏰ Deleted at: #{result.timestamp}

          #{ToonEncoder.format_response(result, format)}

          💡 Use restore_events to recover deleted events if needed
          """

          {:ok, %{content: [%{type: "text", text: text}]}}
        end

      {:error, reason} ->
        {:error, "Failed to query events for deletion: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_archive_events(args, state, format) do
    entity_id = Map.fetch!(args, "entity_id")
    reason = Map.fetch!(args, "reason")
    dry_run = Map.get(args, "dry_run", false)
    older_than = Map.get(args, "older_than")
    event_type = Map.get(args, "event_type")
    retention_days = Map.get(args, "retention_days", 365)

    # Build query params
    query_params =
      %{"entity_id" => entity_id}
      |> maybe_put("event_type", event_type)
      |> maybe_put("until", older_than)

    case state.backend.query_events(query_params) do
      {:ok, data} ->
        events = Map.get(data, "events", [])
        count = length(events)

        if dry_run do
          result = %{
            dry_run: true,
            events_to_archive: count,
            entity_id: entity_id,
            reason: reason,
            retention_days: retention_days
          }

          text = """
          🔍 Archive Preview (DRY RUN)
          📦 Entity: #{entity_id}
          📊 Events that would be archived: #{count}
          📝 Reason: #{reason}
          📅 Retention: #{retention_days} days

          #{ToonEncoder.format_response(result, format)}

          💡 Remove dry_run: true to execute the archival
          """

          {:ok, %{content: [%{type: "text", text: text}]}}
        else
          archived_count = archive_events_batch(events, reason, retention_days, state)

          result = %{
            archived: true,
            events_archived: archived_count,
            entity_id: entity_id,
            reason: reason,
            retention_days: retention_days,
            timestamp: DateTime.utc_now() |> DateTime.to_iso8601()
          }

          text = """
          📦 Events Archived
          📦 Entity: #{entity_id}
          📊 Events archived: #{archived_count}
          📝 Reason: #{reason}
          📅 Retention: #{retention_days} days
          ⏰ Archived at: #{result.timestamp}

          #{ToonEncoder.format_response(result, format)}

          💡 Use restore_events to restore archived events when needed
          """

          {:ok, %{content: [%{type: "text", text: text}]}}
        end

      {:error, reason} ->
        {:error, "Failed to query events for archival: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_restore_events(args, state, format) do
    reason = Map.fetch!(args, "reason")
    dry_run = Map.get(args, "dry_run", false)
    status = Map.get(args, "status", "all")

    # For restore, we need to query with special flags to include deleted/archived
    # Since core may not support this directly, we simulate by tracking in metadata
    entity_id = Map.get(args, "entity_id")
    event_ids = Map.get(args, "event_ids", [])

    query_params =
      %{}
      |> maybe_put("entity_id", entity_id)
      |> maybe_put("since", Map.get(args, "since"))
      |> maybe_put("until", Map.get(args, "until"))

    # Query for system deletion/archive events to find what can be restored
    tombstone_params = Map.put(query_params, "event_type", "system.event_deleted")

    case state.backend.query_events(tombstone_params) do
      {:ok, data} ->
        tombstones = Map.get(data, "events", [])

        restorable =
          tombstones
          |> filter_by_status(status)
          |> filter_by_event_ids(event_ids)

        count = length(restorable)

        if dry_run do
          result = %{
            dry_run: true,
            events_to_restore: count,
            status_filter: status,
            reason: reason
          }

          text = """
          🔍 Restore Preview (DRY RUN)
          📊 Events that would be restored: #{count}
          🏷️  Status filter: #{status}
          📝 Reason: #{reason}

          #{ToonEncoder.format_response(result, format)}

          💡 Remove dry_run: true to execute the restoration
          """

          {:ok, %{content: [%{type: "text", text: text}]}}
        else
          restored_count = restore_events_batch(restorable, reason, state)

          result = %{
            restored: true,
            events_restored: restored_count,
            reason: reason,
            timestamp: DateTime.utc_now() |> DateTime.to_iso8601()
          }

          text = """
          ♻️  Events Restored
          📊 Events restored: #{restored_count}
          📝 Reason: #{reason}
          ⏰ Restored at: #{result.timestamp}

          #{ToonEncoder.format_response(result, format)}
          """

          {:ok, %{content: [%{type: "text", text: text}]}}
        end

      {:error, reason} ->
        {:error, "Failed to query tombstones for restoration: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_export_events(args, state, _format) do
    export_format = Map.get(args, "format", "json")
    compress = Map.get(args, "compress", false)
    include_metadata = Map.get(args, "include_metadata", true)
    limit = Map.get(args, "limit")

    query_params =
      %{}
      |> maybe_put("entity_id", Map.get(args, "entity_id"))
      |> maybe_put("event_type", Map.get(args, "event_type"))
      |> maybe_put("since", Map.get(args, "since"))
      |> maybe_put("until", Map.get(args, "until"))
      |> maybe_put("limit", limit)

    case state.backend.query_events(query_params) do
      {:ok, data} ->
        events = Map.get(data, "events", [])
        count = length(events)

        # Transform events for export
        export_events =
          events
          |> Enum.map(fn event ->
            if include_metadata do
              event
            else
              Map.drop(event, ["metadata"])
            end
          end)

        # Format based on requested export format
        exported_data = format_export(export_events, export_format)

        # Calculate size
        size_bytes = byte_size(exported_data)
        size_display = format_size(size_bytes)

        text = """
        📤 Events Exported
        📊 Events: #{count}
        📋 Format: #{export_format}
        📦 Size: #{size_display}
        🗜️  Compressed: #{compress}

        #{if count <= 50 do
          "📄 Export Data:\n```#{export_format}\n#{exported_data}\n```"
        else
          "📄 Export Data (truncated, #{count} events):\n```#{export_format}\n#{String.slice(exported_data, 0, 2000)}...\n```\n\n💡 For large exports, consider using the API directly with pagination"
        end}
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to query events for export: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_import_events(args, state, format) do
    data = Map.fetch!(args, "data")
    validate_only = Map.get(args, "validate_only", false)
    skip_duplicates = Map.get(args, "skip_duplicates", true)
    preserve_timestamps = Map.get(args, "preserve_timestamps", true)
    entity_id_prefix = Map.get(args, "entity_id_prefix", "")
    batch_size = Map.get(args, "batch_size", 100)

    # Validate events
    {valid_events, validation_errors} = validate_import_events(data)

    if length(validation_errors) > 0 do
      error_summary =
        validation_errors
        |> Enum.take(10)
        |> Enum.map(fn {idx, err} -> "  • Event #{idx}: #{err}" end)
        |> Enum.join("\n")

      text = """
      ❌ Import Validation Failed
      📊 Total events: #{length(data)}
      ✅ Valid: #{length(valid_events)}
      ❌ Invalid: #{length(validation_errors)}

      🔍 First 10 errors:
      #{error_summary}

      💡 Fix validation errors and retry
      """

      {:ok, %{content: [%{type: "text", text: text}]}}
    else
      if validate_only do
        result = %{
          validate_only: true,
          total_events: length(data),
          valid_events: length(valid_events),
          validation_errors: []
        }

        text = """
        ✅ Import Validation Passed
        📊 Total events: #{length(data)}
        ✅ All events valid

        #{ToonEncoder.format_response(result, format)}

        💡 Remove validate_only: true to execute the import
        """

        {:ok, %{content: [%{type: "text", text: text}]}}
      else
        # Execute import
        {imported, skipped, errors} =
          import_events_batch(
            valid_events,
            entity_id_prefix,
            preserve_timestamps,
            skip_duplicates,
            batch_size,
            state
          )

        result = %{
          imported: true,
          events_imported: imported,
          events_skipped: skipped,
          errors: length(errors),
          timestamp: DateTime.utc_now() |> DateTime.to_iso8601()
        }

        text = """
        📥 Events Imported
        📊 Events imported: #{imported}
        ⏭️  Events skipped: #{skipped}
        ❌ Errors: #{length(errors)}
        ⏰ Imported at: #{result.timestamp}

        #{ToonEncoder.format_response(result, format)}
        """

        {:ok, %{content: [%{type: "text", text: text}]}}
      end
    end
  end

  @doc false
  def handle_clone_entity(args, state, format) do
    source_entity_id = Map.fetch!(args, "source_entity_id")
    new_entity_id = Map.get(args, "new_entity_id", generate_entity_id(source_entity_id))
    dry_run = Map.get(args, "dry_run", false)
    reset_timestamps = Map.get(args, "reset_timestamps", false)
    event_types = Map.get(args, "event_types")

    query_params =
      %{"entity_id" => source_entity_id}
      |> maybe_put("event_type", if(event_types, do: hd(event_types), else: nil))
      |> maybe_put("since", Map.get(args, "since"))
      |> maybe_put("until", Map.get(args, "until"))

    case state.backend.query_events(query_params) do
      {:ok, data} ->
        events = Map.get(data, "events", [])

        # Filter by event_types if multiple specified
        events =
          if event_types && length(event_types) > 1 do
            Enum.filter(events, fn e ->
              Map.get(e, "event_type") in event_types
            end)
          else
            events
          end

        count = length(events)

        if dry_run do
          result = %{
            dry_run: true,
            source_entity_id: source_entity_id,
            new_entity_id: new_entity_id,
            events_to_clone: count,
            reset_timestamps: reset_timestamps
          }

          text = """
          🔍 Clone Preview (DRY RUN)
          📦 Source: #{source_entity_id}
          📦 Target: #{new_entity_id}
          📊 Events to clone: #{count}
          ⏰ Reset timestamps: #{reset_timestamps}

          #{ToonEncoder.format_response(result, format)}

          💡 Remove dry_run: true to execute the clone
          """

          {:ok, %{content: [%{type: "text", text: text}]}}
        else
          cloned_count = clone_events_batch(events, new_entity_id, reset_timestamps, state)

          result = %{
            cloned: true,
            source_entity_id: source_entity_id,
            new_entity_id: new_entity_id,
            events_cloned: cloned_count,
            timestamp: DateTime.utc_now() |> DateTime.to_iso8601()
          }

          text = """
          🔄 Entity Cloned
          📦 Source: #{source_entity_id}
          📦 Target: #{new_entity_id}
          📊 Events cloned: #{cloned_count}
          ⏰ Cloned at: #{result.timestamp}

          #{ToonEncoder.format_response(result, format)}
          """

          {:ok, %{content: [%{type: "text", text: text}]}}
        end

      {:error, reason} ->
        {:error, "Failed to query source entity: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_merge_entities(args, state, format) do
    source_entity_ids = Map.fetch!(args, "source_entity_ids")
    target_entity_id = Map.fetch!(args, "target_entity_id")
    reason = Map.fetch!(args, "reason")
    dry_run = Map.get(args, "dry_run", false)
    archive_sources = Map.get(args, "archive_sources", false)
    delete_sources = Map.get(args, "delete_sources", false)

    # Query events from all source entities
    all_events =
      Enum.flat_map(source_entity_ids, fn entity_id ->
        case state.backend.query_events(%{"entity_id" => entity_id}) do
          {:ok, data} -> Map.get(data, "events", [])
          {:error, _} -> []
        end
      end)

    # Sort by timestamp for proper merge order
    sorted_events = Enum.sort_by(all_events, &Map.get(&1, "timestamp", ""))
    count = length(sorted_events)

    if dry_run do
      events_by_source =
        Enum.frequencies_by(all_events, &Map.get(&1, "entity_id"))

      result = %{
        dry_run: true,
        source_entity_ids: source_entity_ids,
        target_entity_id: target_entity_id,
        total_events_to_merge: count,
        events_per_source: events_by_source,
        archive_sources: archive_sources,
        delete_sources: delete_sources,
        reason: reason
      }

      text = """
      🔍 Merge Preview (DRY RUN)
      📦 Sources: #{inspect(source_entity_ids)}
      📦 Target: #{target_entity_id}
      📊 Total events to merge: #{count}
      📝 Reason: #{reason}
      📦 Archive sources after: #{archive_sources}
      🗑️  Delete sources after: #{delete_sources}

      #{ToonEncoder.format_response(result, format)}

      💡 Remove dry_run: true to execute the merge
      """

      {:ok, %{content: [%{type: "text", text: text}]}}
    else
      merged_count = merge_events_batch(sorted_events, target_entity_id, state)

      # Handle source cleanup
      if archive_sources do
        Enum.each(source_entity_ids, fn entity_id ->
          archive_events_batch(
            Enum.filter(all_events, &(Map.get(&1, "entity_id") == entity_id)),
            "Merged into #{target_entity_id}",
            365,
            state
          )
        end)
      end

      if delete_sources do
        Enum.each(source_entity_ids, fn entity_id ->
          soft_delete_events(
            Enum.filter(all_events, &(Map.get(&1, "entity_id") == entity_id)),
            "Merged into #{target_entity_id}",
            state
          )
        end)
      end

      result = %{
        merged: true,
        source_entity_ids: source_entity_ids,
        target_entity_id: target_entity_id,
        events_merged: merged_count,
        reason: reason,
        timestamp: DateTime.utc_now() |> DateTime.to_iso8601()
      }

      text = """
      🔀 Entities Merged
      📦 Sources: #{inspect(source_entity_ids)}
      📦 Target: #{target_entity_id}
      📊 Events merged: #{merged_count}
      📝 Reason: #{reason}
      ⏰ Merged at: #{result.timestamp}

      #{ToonEncoder.format_response(result, format)}
      """

      {:ok, %{content: [%{type: "text", text: text}]}}
    end
  end

  @doc false
  def handle_split_entity(args, state, format) do
    source_entity_id = Map.fetch!(args, "source_entity_id")
    splits = Map.fetch!(args, "splits")
    reason = Map.fetch!(args, "reason")
    dry_run = Map.get(args, "dry_run", false)
    archive_split_events = Map.get(args, "archive_split_events", false)
    delete_split_events = Map.get(args, "delete_split_events", false)

    # Query all events for source entity
    case state.backend.query_events(%{"entity_id" => source_entity_id}) do
      {:ok, data} ->
        source_events = Map.get(data, "events", [])

        # Calculate split assignments
        split_assignments =
          Enum.map(splits, fn split_def ->
            matching_events = filter_events_for_split(source_events, split_def)

            %{
              new_entity_id: Map.get(split_def, "new_entity_id"),
              event_count: length(matching_events),
              events: matching_events
            }
          end)

        total_split = Enum.sum(Enum.map(split_assignments, & &1.event_count))

        if dry_run do
          result = %{
            dry_run: true,
            source_entity_id: source_entity_id,
            source_event_count: length(source_events),
            splits:
              Enum.map(split_assignments, fn sa ->
                %{new_entity_id: sa.new_entity_id, event_count: sa.event_count}
              end),
            total_events_to_split: total_split,
            archive_split_events: archive_split_events,
            delete_split_events: delete_split_events,
            reason: reason
          }

          text = """
          🔍 Split Preview (DRY RUN)
          📦 Source: #{source_entity_id} (#{length(source_events)} events)
          📊 Total events to split: #{total_split}
          📝 Reason: #{reason}

          📋 Split assignments:
          #{format_split_preview(split_assignments)}

          #{ToonEncoder.format_response(result, format)}

          💡 Remove dry_run: true to execute the split
          """

          {:ok, %{content: [%{type: "text", text: text}]}}
        else
          # Execute splits
          split_results =
            Enum.map(split_assignments, fn sa ->
              cloned = clone_events_batch(sa.events, sa.new_entity_id, false, state)

              # Handle source event cleanup
              if archive_split_events do
                archive_events_batch(sa.events, reason, 365, state)
              end

              if delete_split_events do
                soft_delete_events(sa.events, reason, state)
              end

              %{new_entity_id: sa.new_entity_id, events_created: cloned}
            end)

          result = %{
            split: true,
            source_entity_id: source_entity_id,
            splits: split_results,
            reason: reason,
            timestamp: DateTime.utc_now() |> DateTime.to_iso8601()
          }

          text = """
          ✂️  Entity Split
          📦 Source: #{source_entity_id}
          📝 Reason: #{reason}
          ⏰ Split at: #{result.timestamp}

          📋 New entities created:
          #{format_split_results(split_results)}

          #{ToonEncoder.format_response(result, format)}
          """

          {:ok, %{content: [%{type: "text", text: text}]}}
        end

      {:error, reason} ->
        {:error, "Failed to query source entity: #{inspect(reason)}"}
    end
  end

  # ============================================================================
  # Event Management Helper Functions
  # ============================================================================

  defp build_delete_query_params(args) do
    # If specific event_ids provided, we can't query by them directly
    # so we'll filter after querying by entity
    %{}
    |> maybe_put("entity_id", Map.get(args, "entity_id"))
    |> maybe_put("event_type", Map.get(args, "event_type"))
    |> maybe_put("since", Map.get(args, "since"))
    |> maybe_put("until", Map.get(args, "until"))
  end

  defp soft_delete_events(events, reason, state) do
    # Soft delete by ingesting tombstone events for each deleted event
    Enum.reduce(events, 0, fn event, count ->
      event_id = Map.get(event, "id", Map.get(event, "event_id"))
      entity_id = Map.get(event, "entity_id")

      tombstone = %{
        "event_type" => "system.event_deleted",
        "entity_id" => entity_id,
        "payload" => %{
          "deleted_event_id" => event_id,
          "deleted_event_type" => Map.get(event, "event_type"),
          "reason" => reason,
          "original_timestamp" => Map.get(event, "timestamp")
        },
        "metadata" => %{
          "operation" => "soft_delete",
          "deleted_at" => DateTime.utc_now() |> DateTime.to_iso8601()
        }
      }

      case state.backend.ingest_event(tombstone) do
        {:ok, _} -> count + 1
        {:error, _} -> count
      end
    end)
  end

  defp archive_events_batch(events, reason, retention_days, state) do
    Enum.reduce(events, 0, fn event, count ->
      event_id = Map.get(event, "id", Map.get(event, "event_id"))
      entity_id = Map.get(event, "entity_id")

      archive_event = %{
        "event_type" => "system.event_archived",
        "entity_id" => entity_id,
        "payload" => %{
          "archived_event_id" => event_id,
          "archived_event_type" => Map.get(event, "event_type"),
          "reason" => reason,
          "retention_days" => retention_days,
          "original_timestamp" => Map.get(event, "timestamp")
        },
        "metadata" => %{
          "operation" => "archive",
          "archived_at" => DateTime.utc_now() |> DateTime.to_iso8601()
        }
      }

      case state.backend.ingest_event(archive_event) do
        {:ok, _} -> count + 1
        {:error, _} -> count
      end
    end)
  end

  defp restore_events_batch(tombstones, reason, state) do
    Enum.reduce(tombstones, 0, fn tombstone, count ->
      entity_id = Map.get(tombstone, "entity_id")
      payload = Map.get(tombstone, "payload", %{})

      restore_event = %{
        "event_type" => "system.event_restored",
        "entity_id" => entity_id,
        "payload" => %{
          "restored_event_id" =>
            Map.get(payload, "deleted_event_id") ||
              Map.get(payload, "archived_event_id"),
          "restored_from" => Map.get(tombstone, "event_type"),
          "reason" => reason
        },
        "metadata" => %{
          "operation" => "restore",
          "restored_at" => DateTime.utc_now() |> DateTime.to_iso8601()
        }
      }

      case state.backend.ingest_event(restore_event) do
        {:ok, _} -> count + 1
        {:error, _} -> count
      end
    end)
  end

  defp filter_by_status(tombstones, "deleted") do
    Enum.filter(tombstones, &(Map.get(&1, "event_type") == "system.event_deleted"))
  end

  defp filter_by_status(tombstones, "archived") do
    Enum.filter(tombstones, &(Map.get(&1, "event_type") == "system.event_archived"))
  end

  defp filter_by_status(tombstones, _), do: tombstones

  defp filter_by_event_ids(tombstones, []), do: tombstones

  defp filter_by_event_ids(tombstones, event_ids) do
    Enum.filter(tombstones, fn t ->
      payload = Map.get(t, "payload", %{})
      deleted_id = Map.get(payload, "deleted_event_id") || Map.get(payload, "archived_event_id")
      deleted_id in event_ids
    end)
  end

  defp format_export(events, "json") do
    Jason.encode!(events, pretty: true)
  end

  defp format_export(events, "jsonl") do
    events
    |> Enum.map(&Jason.encode!/1)
    |> Enum.join("\n")
  end

  defp format_export(events, "csv") do
    headers = ["event_id", "event_type", "entity_id", "timestamp", "payload"]
    header_line = Enum.join(headers, ",")

    rows =
      Enum.map(events, fn event ->
        [
          Map.get(event, "id", Map.get(event, "event_id", "")),
          Map.get(event, "event_type", ""),
          Map.get(event, "entity_id", ""),
          Map.get(event, "timestamp", ""),
          Jason.encode!(Map.get(event, "payload", %{}))
        ]
        |> Enum.map(&escape_csv/1)
        |> Enum.join(",")
      end)

    [header_line | rows] |> Enum.join("\n")
  end

  defp format_export(events, "parquet") do
    # Parquet requires specialized library - return JSON with note
    "# Parquet export requires direct API access\n" <> Jason.encode!(events, pretty: true)
  end

  defp format_export(events, _), do: Jason.encode!(events, pretty: true)

  defp escape_csv(value) when is_binary(value) do
    if String.contains?(value, [",", "\"", "\n"]) do
      "\"#{String.replace(value, "\"", "\"\"")}\""
    else
      value
    end
  end

  defp escape_csv(value), do: to_string(value)

  defp format_size(bytes) when bytes < 1024, do: "#{bytes} B"
  defp format_size(bytes) when bytes < 1024 * 1024, do: "#{Float.round(bytes / 1024, 1)} KB"
  defp format_size(bytes), do: "#{Float.round(bytes / (1024 * 1024), 1)} MB"

  defp validate_import_events(data) do
    data
    |> Enum.with_index()
    |> Enum.reduce({[], []}, fn {event, idx}, {valid, errors} ->
      case validate_single_event(event) do
        :ok -> {[event | valid], errors}
        {:error, msg} -> {valid, [{idx, msg} | errors]}
      end
    end)
    |> then(fn {valid, errors} -> {Enum.reverse(valid), Enum.reverse(errors)} end)
  end

  defp validate_single_event(event) do
    cond do
      not is_map(event) ->
        {:error, "Event must be an object"}

      not Map.has_key?(event, "event_type") ->
        {:error, "Missing required field: event_type"}

      not Map.has_key?(event, "entity_id") ->
        {:error, "Missing required field: entity_id"}

      not Map.has_key?(event, "payload") ->
        {:error, "Missing required field: payload"}

      not is_binary(Map.get(event, "event_type")) ->
        {:error, "event_type must be a string"}

      not is_binary(Map.get(event, "entity_id")) ->
        {:error, "entity_id must be a string"}

      not is_map(Map.get(event, "payload")) ->
        {:error, "payload must be an object"}

      true ->
        :ok
    end
  end

  defp import_events_batch(events, prefix, preserve_ts, skip_dups, _batch_size, state) do
    Enum.reduce(events, {0, 0, []}, fn event, {imported, skipped, errors} ->
      entity_id = prefix <> Map.get(event, "entity_id")

      import_event = %{
        "event_type" => Map.get(event, "event_type"),
        "entity_id" => entity_id,
        "payload" => Map.get(event, "payload"),
        "metadata" =>
          Map.merge(Map.get(event, "metadata", %{}), %{
            "imported" => true,
            "imported_at" => DateTime.utc_now() |> DateTime.to_iso8601(),
            "original_timestamp" => if(preserve_ts, do: Map.get(event, "timestamp"), else: nil)
          })
      }

      case state.backend.ingest_event(import_event) do
        {:ok, _} ->
          {imported + 1, skipped, errors}

        {:error, reason} ->
          if skip_dups and String.contains?(inspect(reason), "duplicate") do
            {imported, skipped + 1, errors}
          else
            {imported, skipped, [reason | errors]}
          end
      end
    end)
  end

  defp generate_entity_id(source_id) do
    suffix = :crypto.strong_rand_bytes(4) |> Base.encode16(case: :lower)
    "#{source_id}-clone-#{suffix}"
  end

  defp clone_events_batch(events, new_entity_id, reset_timestamps, state) do
    now = DateTime.utc_now() |> DateTime.to_iso8601()

    Enum.reduce(events, 0, fn event, count ->
      clone_event = %{
        "event_type" => Map.get(event, "event_type"),
        "entity_id" => new_entity_id,
        "payload" => Map.get(event, "payload"),
        "metadata" =>
          Map.merge(Map.get(event, "metadata", %{}), %{
            "cloned_from" => Map.get(event, "entity_id"),
            "cloned_at" => now,
            "original_event_id" => Map.get(event, "id", Map.get(event, "event_id")),
            "original_timestamp" => if(reset_timestamps, do: Map.get(event, "timestamp"), else: nil)
          })
      }

      case state.backend.ingest_event(clone_event) do
        {:ok, _} -> count + 1
        {:error, _} -> count
      end
    end)
  end

  defp merge_events_batch(events, target_entity_id, state) do
    now = DateTime.utc_now() |> DateTime.to_iso8601()

    Enum.reduce(events, 0, fn event, count ->
      original_entity = Map.get(event, "entity_id")

      merge_event = %{
        "event_type" => Map.get(event, "event_type"),
        "entity_id" => target_entity_id,
        "payload" => Map.get(event, "payload"),
        "metadata" =>
          Map.merge(Map.get(event, "metadata", %{}), %{
            "merged_from" => original_entity,
            "merged_at" => now,
            "original_event_id" => Map.get(event, "id", Map.get(event, "event_id")),
            "original_timestamp" => Map.get(event, "timestamp")
          })
      }

      case state.backend.ingest_event(merge_event) do
        {:ok, _} -> count + 1
        {:error, _} -> count
      end
    end)
  end

  defp filter_events_for_split(events, split_def) do
    events
    |> maybe_filter_by_types(Map.get(split_def, "event_types"))
    |> maybe_filter_by_since(Map.get(split_def, "since"))
    |> maybe_filter_by_until(Map.get(split_def, "until"))
  end

  defp maybe_filter_by_types(events, nil), do: events
  defp maybe_filter_by_types(events, []), do: events

  defp maybe_filter_by_types(events, types) do
    Enum.filter(events, fn e ->
      event_type = Map.get(e, "event_type", "")

      Enum.any?(types, fn pattern ->
        if String.ends_with?(pattern, "*") do
          prefix = String.trim_trailing(pattern, "*")
          String.starts_with?(event_type, prefix)
        else
          event_type == pattern
        end
      end)
    end)
  end

  defp maybe_filter_by_since(events, nil), do: events

  defp maybe_filter_by_since(events, since) do
    Enum.filter(events, fn e ->
      timestamp = Map.get(e, "timestamp", "")
      timestamp >= since
    end)
  end

  defp maybe_filter_by_until(events, nil), do: events

  defp maybe_filter_by_until(events, until_time) do
    Enum.filter(events, fn e ->
      timestamp = Map.get(e, "timestamp", "")
      timestamp <= until_time
    end)
  end

  defp format_split_preview(split_assignments) do
    split_assignments
    |> Enum.map(fn sa ->
      "   • #{sa.new_entity_id}: #{sa.event_count} events"
    end)
    |> Enum.join("\n")
  end

  defp format_split_results(split_results) do
    split_results
    |> Enum.map(fn sr ->
      "   • #{sr.new_entity_id}: #{sr.events_created} events created"
    end)
    |> Enum.join("\n")
  end

  # ============================================================================
  # Operational Tool Definitions
  # ============================================================================

  defp tool_compact_storage do
    %{
      name: "compact_storage",
      description: """
      Trigger manual storage compaction to reclaim disk space and optimize \
      read performance. Compaction merges small segments and removes tombstoned data.

      **When to use this tool:**
      - After bulk deletions or archival operations to reclaim disk space
      - When read performance degrades due to segment fragmentation
      - During maintenance windows for preventive optimization
      - After large import operations that created many small segments

      **SAFETY WARNING:**
      - Compaction is CPU and I/O intensive — avoid during peak traffic
      - Long-running operation: may take minutes to hours on large datasets
      - Safe to run — does not delete live data, only merges segments
      - Consider running during off-peak hours

      **Common patterns:**
      - Full compaction: no parameters needed
      - Tenant-specific: `tenant_id: "tenant-123"`
      - Partition-specific: `partition_id: "partition-0"`
      - Dry run: `dry_run: true` to estimate work without executing

      **Performance tips:**
      - Use dry_run first to estimate compaction time and space savings
      - Schedule during low-traffic periods
      - Monitor disk I/O during compaction
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "tenant_id" => %{
            type: "string",
            description: "Compact only this tenant's data"
          },
          "partition_id" => %{
            type: "string",
            description: "Compact only this partition"
          },
          "dry_run" => %{
            type: "boolean",
            description: "Estimate compaction without executing (default: false)"
          }
        }
      }
    }
  end

  defp tool_storage_stats do
    %{
      name: "storage_stats",
      description: """
      Get disk usage analytics broken down by tenant, event type, or time period. \
      Useful for capacity planning, cost allocation, and identifying storage hotspots.

      **When to use this tool:**
      - Capacity planning: understand current and projected storage usage
      - Cost allocation: attribute storage costs to specific tenants
      - Identifying hotspots: find tenants or event types consuming most space
      - Monitoring growth: track storage trends over time

      **Common patterns:**
      - Overview: no parameters needed — returns total storage summary
      - By tenant: `group_by: "tenant"` — breakdown per tenant
      - By event type: `group_by: "event_type"` — breakdown per event type
      - Specific tenant: `tenant_id: "tenant-123"` — detailed view for one tenant

      **Performance tips:**
      - This is a read-only operation with minimal impact
      - Results may be cached; use `refresh: true` for real-time data
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "tenant_id" => %{
            type: "string",
            description: "Get stats for a specific tenant"
          },
          "group_by" => %{
            type: "string",
            enum: ["tenant", "event_type", "partition"],
            description: "Group statistics by dimension"
          },
          "refresh" => %{
            type: "boolean",
            description: "Force refresh cached statistics (default: false)"
          }
        }
      }
    }
  end

  defp tool_partition_info do
    %{
      name: "partition_info",
      description: """
      Get partition health, distribution, and balance information. Partitions are \
      the fundamental unit of data distribution in AllSource Core.

      **When to use this tool:**
      - Diagnosing uneven data distribution (hot partitions)
      - Monitoring partition health and replication status
      - Planning partition rebalancing operations
      - Investigating slow queries that may be partition-bound

      **Common patterns:**
      - Overview: no parameters — returns all partition summary
      - Specific partition: `partition_id: "partition-0"`
      - Health check: look for partitions with status != "healthy"
      - Balance check: compare event_count across partitions

      **Performance tips:**
      - Lightweight read-only operation
      - Hot partitions indicate need for rebalancing
      - Uneven distribution can cause query performance skew
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "partition_id" => %{
            type: "string",
            description: "Get info for a specific partition"
          },
          "include_replicas" => %{
            type: "boolean",
            description: "Include replica partition details (default: false)"
          }
        }
      }
    }
  end

  defp tool_wal_status do
    %{
      name: "wal_status",
      description: """
      Get Write-Ahead Log (WAL) statistics and replication lag information. \
      The WAL ensures durability and enables replication between Core instances.

      **When to use this tool:**
      - Monitoring replication lag between write leader and read replicas
      - Diagnosing write performance issues (WAL flush latency)
      - Checking WAL segment accumulation (potential disk pressure)
      - Verifying durability guarantees after writes

      **Common patterns:**
      - Status check: no parameters needed
      - Look for: high lag_bytes indicates slow replicas
      - Look for: many pending_segments indicates flush backlog
      - Look for: high flush_latency_ms indicates I/O bottleneck

      **Performance tips:**
      - Lightweight read-only operation
      - High WAL lag can indicate network or I/O issues
      - WAL growth without compaction indicates segment accumulation
      """,
      inputSchema: %{
        type: "object",
        properties: %{}
      }
    }
  end

  defp tool_backup_create do
    %{
      name: "backup_create",
      description: """
      Create a backup snapshot of the event store. Backups capture a consistent \
      point-in-time copy of all data for disaster recovery.

      **When to use this tool:**
      - Before major operations (migrations, bulk deletes, schema changes)
      - Scheduled disaster recovery snapshots
      - Before deploying new Core versions
      - Creating restore points for testing

      **SAFETY WARNING:**
      - Backup creation is I/O intensive — plan for increased disk usage
      - Ensure sufficient disk space for the snapshot (check storage_stats first)
      - Backup is a point-in-time snapshot — events written during backup may not be included
      - Large datasets may take significant time to backup

      **Common patterns:**
      - Full backup: `label: "pre-migration-2024-01"`
      - Tenant backup: `tenant_id: "tenant-123", label: "tenant-backup"`
      - Incremental: `incremental: true` (only changes since last backup)

      **Performance tips:**
      - Use incremental backups for frequent snapshots
      - Schedule during off-peak hours
      - Monitor disk space during backup creation
      - Use label parameter for easy identification in backup_list
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "label" => %{
            type: "string",
            description: "Human-readable label for the backup"
          },
          "tenant_id" => %{
            type: "string",
            description: "Backup only this tenant's data"
          },
          "incremental" => %{
            type: "boolean",
            description: "Create incremental backup from last full backup (default: false)"
          }
        }
      }
    }
  end

  defp tool_backup_restore do
    %{
      name: "backup_restore",
      description: """
      Restore data from a backup snapshot. This is a DESTRUCTIVE operation that \
      replaces current data with the backup contents.

      **SAFETY WARNING — DESTRUCTIVE OPERATION:**
      - This REPLACES current data with backup data
      - All events written after the backup was created will be LOST
      - Always create a fresh backup before restoring (backup_create)
      - Consider using dry_run: true first to validate the backup
      - This operation requires confirmation and cannot be undone

      **When to use this tool:**
      - Disaster recovery after data corruption
      - Rolling back a failed migration
      - Restoring a tenant's data from a known good state
      - Testing restore procedures

      **Common patterns:**
      - Restore from specific backup: `backup_id: "bkp-20240115-001"`
      - Dry run validation: `backup_id: "bkp-20240115-001", dry_run: true`
      - Tenant restore: `backup_id: "bkp-20240115-001", tenant_id: "tenant-123"`

      **Performance tips:**
      - Restore time depends on backup size
      - System may be unavailable during restore
      - Always verify data integrity after restore
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "backup_id" => %{
            type: "string",
            description: "ID of the backup to restore from (use backup_list to find)"
          },
          "tenant_id" => %{
            type: "string",
            description: "Restore only this tenant's data"
          },
          "dry_run" => %{
            type: "boolean",
            description: "Validate backup integrity without restoring (default: false)"
          },
          "confirm" => %{
            type: "boolean",
            description: "REQUIRED: Must be true to execute restore. Safety confirmation."
          }
        },
        required: ["backup_id", "confirm"]
      }
    }
  end

  defp tool_backup_list do
    %{
      name: "backup_list",
      description: """
      List available backup snapshots with metadata. Use this to find backup IDs \
      for restore operations or to audit backup history.

      **When to use this tool:**
      - Finding a specific backup for restore
      - Auditing backup history and retention
      - Verifying backup schedule is running
      - Checking backup sizes for capacity planning

      **Common patterns:**
      - List all backups: no parameters needed
      - Filter by tenant: `tenant_id: "tenant-123"`
      - Recent backups: `limit: 5`
      - Find by label: check the label field in results

      **Performance tips:**
      - Lightweight read-only metadata operation
      - Results include backup_id, label, created_at, size, and status
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "tenant_id" => %{
            type: "string",
            description: "List backups for a specific tenant"
          },
          "limit" => %{
            type: "number",
            description: "Maximum number of backups to return (default: 20)"
          }
        }
      }
    }
  end

  defp tool_health_deep do
    %{
      name: "health_deep",
      description: """
      Perform a deep health check across all Core components including storage, \
      WAL, partitions, replication, and resource usage. More comprehensive than \
      get_stats or get_cluster_status.

      **When to use this tool:**
      - Comprehensive system health assessment
      - Troubleshooting performance or availability issues
      - Pre-deployment readiness checks
      - Periodic health monitoring and alerting
      - When get_stats shows anomalies and you need deeper investigation

      **Common patterns:**
      - Full check: no parameters — checks all components
      - Look for: components with status != "healthy"
      - Look for: high resource utilization (>80% disk, memory, CPU)
      - Look for: replication lag or WAL backlog

      **Decision guide:**
      - Quick overview? → get_stats
      - Cluster topology? → get_cluster_status
      - Comprehensive check? → health_deep (this tool)
      - Storage specific? → storage_stats
      - WAL specific? → wal_status

      **Performance tips:**
      - May take a few seconds as it checks all subsystems
      - Safe to run frequently — read-only operation
      - Results include component-level health with recommendations
      """,
      inputSchema: %{
        type: "object",
        properties: %{}
      }
    }
  end

  defp tool_performance_report do
    %{
      name: "performance_report",
      description: """
      Generate a performance metrics summary including query latencies, throughput, \
      cache hit rates, and resource utilization over a time period.

      **When to use this tool:**
      - Identifying performance bottlenecks
      - Monitoring query latency trends
      - Evaluating cache effectiveness
      - Capacity planning based on throughput trends
      - Before and after performance optimization comparisons

      **Common patterns:**
      - Current snapshot: no parameters needed
      - Time range: `period: "1h"` or `period: "24h"` or `period: "7d"`
      - By tenant: `tenant_id: "tenant-123"` — tenant-specific metrics

      **Key metrics to look for:**
      - p50/p95/p99 query latency — indicates query performance
      - Write throughput — events ingested per second
      - Cache hit rate — low rates indicate cache tuning needed
      - Disk I/O utilization — high values indicate I/O bottleneck

      **Performance tips:**
      - Lightweight read-only operation
      - Compare periods to identify trends
      - Use alongside health_deep for comprehensive diagnostics
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "period" => %{
            type: "string",
            enum: ["1h", "6h", "24h", "7d", "30d"],
            description: "Time period for metrics (default: 1h)"
          },
          "tenant_id" => %{
            type: "string",
            description: "Get metrics for a specific tenant"
          }
        }
      }
    }
  end

  defp tool_audit_log do
    %{
      name: "audit_log",
      description: """
      Query the audit trail for system and data operations. Every destructive \
      or administrative operation is recorded in the audit log for compliance \
      and forensic analysis.

      **When to use this tool:**
      - Investigating who performed a specific operation
      - Compliance auditing (GDPR, SOC2, HIPAA)
      - Forensic analysis after incidents
      - Reviewing recent administrative actions
      - Tracking data lifecycle operations (deletes, archives, restores)

      **Common patterns:**
      - Recent activity: no parameters — returns latest audit entries
      - By action: `action: "delete"` or `action: "backup_create"`
      - By actor: `actor: "admin@example.com"` — who performed the action
      - By entity: `entity_id: "user-123"` — operations on a specific entity
      - Time range: `since: "2024-01-01T00:00:00Z", until: "2024-01-31T23:59:59Z"`

      **Performance tips:**
      - Use filters to narrow results for large audit logs
      - Use limit parameter to cap results
      - Audit entries include: action, actor, target, timestamp, details
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "action" => %{
            type: "string",
            description: "Filter by action type (e.g., delete, archive, restore, backup_create)"
          },
          "actor" => %{
            type: "string",
            description: "Filter by actor/user who performed the action"
          },
          "entity_id" => %{
            type: "string",
            description: "Filter by target entity ID"
          },
          "since" => %{
            type: "string",
            description: "Filter entries since this ISO timestamp"
          },
          "until" => %{
            type: "string",
            description: "Filter entries until this ISO timestamp"
          },
          "limit" => %{
            type: "number",
            description: "Maximum entries to return (default: 50)"
          }
        }
      }
    }
  end

  # ============================================================================
  # Operational Tool Handlers
  # ============================================================================

  @doc false
  def handle_compact_storage(args, state, format) do
    dry_run = Map.get(args, "dry_run", false)

    params =
      %{}
      |> maybe_put("tenant_id", Map.get(args, "tenant_id"))
      |> maybe_put("partition_id", Map.get(args, "partition_id"))
      |> maybe_put("dry_run", dry_run)

    case state.backend.compact_storage(params) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        text =
          if dry_run do
            """
            🔍 Compaction Preview (DRY RUN)

            #{formatted_data}

            💡 Remove dry_run: true to execute compaction
            """
          else
            """
            🗜️ Storage Compaction Initiated

            #{formatted_data}

            ⚠️ Compaction runs in the background. Use storage_stats to monitor progress.
            """
          end

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to trigger compaction: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_storage_stats(args, state, _format) do
    params =
      %{}
      |> maybe_put("tenant_id", Map.get(args, "tenant_id"))
      |> maybe_put("group_by", Map.get(args, "group_by"))
      |> maybe_put("refresh", Map.get(args, "refresh"))

    case state.backend.storage_stats(params) do
      {:ok, data} ->
        memory = Map.get(data, "memory_events", Map.get(data, "total_events", 0))
        pq_on = Map.get(data, "parquet_enabled", false)
        pq_files = Map.get(data, "parquet_files", 0)
        pq_bytes = Map.get(data, "parquet_bytes", 0)
        pq_batch = Map.get(data, "parquet_pending_batch", 0)
        wal_n = Map.get(data, "wal_entries", 0)
        wal_bytes = Map.get(data, "wal_bytes", 0)
        durable = Map.get(data, "durable", true)

        durability_line =
          if durable,
            do: "Durable: true",
            else: "⚠️  Durable: FALSE — #{memory} events at risk of loss on restart"

        text = """
        💾 Storage Statistics

        Memory events: #{memory}
        WAL entries: #{wal_n} (#{format_bytes(wal_bytes)})
        Parquet: #{if pq_on, do: "enabled", else: "disabled"}
        Parquet files: #{pq_files} (#{format_bytes(pq_bytes)})
        Parquet pending batch: #{pq_batch}
        #{durability_line}
        """

        {:ok, %{content: [%{type: "text", text: String.trim(text)}]}}

      {:error, reason} ->
        {:error, "Failed to get storage stats: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_partition_info(args, state, format) do
    params =
      %{}
      |> maybe_put("partition_id", Map.get(args, "partition_id"))
      |> maybe_put("include_replicas", Map.get(args, "include_replicas"))

    case state.backend.partition_info(params) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        text = """
        🗂️ Partition Information

        #{formatted_data}
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to get partition info: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_wal_status(_args, state, _format) do
    case state.backend.wal_status() do
      {:ok, data} ->
        wal_on = Map.get(data, "wal_enabled", false)
        entries = Map.get(data, "wal_entries", 0)
        bytes = Map.get(data, "wal_bytes", 0)
        seq = Map.get(data, "wal_sequence", 0)
        memory = Map.get(data, "memory_events", 0)
        durable = Map.get(data, "durable", false)

        text = """
        📝 WAL Status

        Enabled: #{wal_on}
        Entries: #{entries}
        Bytes: #{format_bytes(bytes)}
        Sequence: #{seq}
        Memory events: #{memory}
        Durable: #{durable}
        """

        {:ok, %{content: [%{type: "text", text: String.trim(text)}]}}

      {:error, reason} ->
        {:error, "Failed to get WAL status: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_backup_create(_args, _state, _format) do
    {:ok,
     %{
       content: [
         %{
           type: "text",
           text: """
           Backup Create — Not Yet Available

           Backup API is not yet implemented in AllSource Core.

           Workaround: Copy the Core data directory (WAL + Parquet files) while the service is stopped.
           """
         }
       ]
     }}
  end

  @doc false
  def handle_backup_restore(_args, _state, _format) do
    {:ok,
     %{
       content: [
         %{
           type: "text",
           text: """
           Backup Restore — Not Yet Available

           Backup restore API is not yet implemented in AllSource Core.

           Workaround: Stop Core, replace the data directory with a backup copy, then restart.
           """
         }
       ]
     }}
  end

  @doc false
  def handle_backup_list(_args, _state, _format) do
    {:ok,
     %{
       content: [
         %{
           type: "text",
           text: """
           Backup List — Not Yet Available

           Backup API is not yet implemented in AllSource Core.

           Workaround: Check the Core data directory for WAL segments and Parquet files.
           """
         }
       ]
     }}
  end

  @doc false
  def handle_health_deep(_args, state, _format) do
    case state.backend.health_deep() do
      {:ok, data} ->
        status = Map.get(data, "status", "unknown")
        mode = Map.get(data, "mode", "unknown")
        memory = Map.get(data, "memory_events", 0)
        wal_on = Map.get(data, "wal_enabled", false)
        wal_n = Map.get(data, "wal_entries", 0)
        pq_on = Map.get(data, "parquet_enabled", false)
        pq_n = Map.get(data, "parquet_files", 0)
        durable = Map.get(data, "durable", false)
        warnings = Map.get(data, "warnings", [])

        warning_block =
          case warnings do
            [] -> ""
            list -> "\n⚠️  WARNINGS:\n" <> Enum.map_join(list, "\n", &("  - " <> &1)) <> "\n"
          end

        text = """
        🏥 Deep Health Check

        Status: #{status}
        Mode: #{mode}
        Durable: #{durable}

        Memory events: #{memory}
        WAL: #{if wal_on, do: "enabled (#{wal_n} entries)", else: "disabled"}
        Parquet: #{if pq_on, do: "enabled (#{pq_n} files)", else: "disabled"}
        #{warning_block}
        """

        {:ok, %{content: [%{type: "text", text: String.trim(text)}]}}

      {:error, reason} ->
        {:error, "Failed to perform deep health check: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_performance_report(args, state, format) do
    params =
      %{}
      |> maybe_put("period", Map.get(args, "period"))
      |> maybe_put("tenant_id", Map.get(args, "tenant_id"))

    case state.backend.performance_report(params) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        text = """
        📈 Performance Report

        #{formatted_data}
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to get performance report: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_audit_log(args, state, format) do
    params =
      %{}
      |> maybe_put("action", Map.get(args, "action"))
      |> maybe_put("actor", Map.get(args, "actor"))
      |> maybe_put("entity_id", Map.get(args, "entity_id"))
      |> maybe_put("since", Map.get(args, "since"))
      |> maybe_put("until", Map.get(args, "until"))
      |> maybe_put("limit", Map.get(args, "limit"))

    case state.backend.audit_log(params) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        text = """
        📜 Audit Log

        #{formatted_data}
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to query audit log: #{inspect(reason)}"}
    end
  end

  # ============================================================================
  # Multi-Tenancy Tool Definitions
  # ============================================================================

  defp tool_tenant_create do
    %{
      name: "tenant_create",
      description: """
      Provision a new tenant with quotas and settings. Creates a tenant in the \
      Go Control Plane for multi-tenant event store isolation.

      **ADMIN ONLY** — Requires admin role to execute.

      **When to use this tool:**
      - Onboarding a new customer or team to the platform
      - Setting up isolated environments for testing or staging
      - Creating tenants with specific quota limits
      - Provisioning tenants as part of automated signup flows

      **Common patterns:**
      - Basic tenant: `name: "acme-corp", plan: "standard"`
      - With quotas: `name: "acme-corp", plan: "enterprise", max_events_per_day: 1000000`
      - With metadata: `name: "acme-corp", metadata: {"team": "backend", "env": "prod"}`

      **Performance tips:**
      - Tenant creation is a lightweight control plane operation
      - The tenant_id is auto-generated if not provided
      - Quotas can be updated later with tenant_update or tenant_quotas
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "name" => %{
            type: "string",
            description: "Human-readable tenant name"
          },
          "tenant_id" => %{
            type: "string",
            description: "Custom tenant ID (auto-generated if omitted)"
          },
          "plan" => %{
            type: "string",
            enum: ["free", "standard", "professional", "enterprise"],
            description: "Subscription plan determining default quotas"
          },
          "max_events_per_day" => %{
            type: "number",
            description: "Maximum events allowed per day (overrides plan default)"
          },
          "max_storage_bytes" => %{
            type: "number",
            description: "Maximum storage in bytes (overrides plan default)"
          },
          "metadata" => %{
            type: "object",
            description: "Custom metadata key-value pairs for the tenant"
          }
        },
        required: ["name"]
      }
    }
  end

  defp tool_tenant_update do
    %{
      name: "tenant_update",
      description: """
      Update tenant settings, quotas, or metadata. Modifies an existing tenant's \
      configuration in the Go Control Plane.

      **ADMIN ONLY** — Requires admin role to execute.

      **When to use this tool:**
      - Upgrading or downgrading a tenant's plan
      - Adjusting quotas after usage review
      - Updating tenant metadata (team, contact, tags)
      - Changing tenant display name

      **Common patterns:**
      - Change plan: `tenant_id: "t-123", plan: "enterprise"`
      - Adjust quota: `tenant_id: "t-123", max_events_per_day: 5000000`
      - Update metadata: `tenant_id: "t-123", metadata: {"team": "platform"}`

      **Performance tips:**
      - Lightweight control plane operation
      - Quota changes take effect immediately
      - Plan changes may affect rate limiting within seconds
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "tenant_id" => %{
            type: "string",
            description: "ID of the tenant to update"
          },
          "name" => %{
            type: "string",
            description: "Updated tenant name"
          },
          "plan" => %{
            type: "string",
            enum: ["free", "standard", "professional", "enterprise"],
            description: "New subscription plan"
          },
          "max_events_per_day" => %{
            type: "number",
            description: "Updated daily event quota"
          },
          "max_storage_bytes" => %{
            type: "number",
            description: "Updated storage quota in bytes"
          },
          "metadata" => %{
            type: "object",
            description: "Updated metadata (merged with existing)"
          }
        },
        required: ["tenant_id"]
      }
    }
  end

  defp tool_tenant_usage do
    %{
      name: "tenant_usage",
      description: """
      Get usage statistics for a tenant including event counts, storage usage, \
      API call volume, and quota consumption percentages.

      **ADMIN ONLY** — Requires admin role to execute.

      **When to use this tool:**
      - Monitoring tenant resource consumption
      - Identifying tenants approaching quota limits
      - Generating billing reports based on usage
      - Capacity planning per tenant

      **Common patterns:**
      - Current usage: `tenant_id: "t-123"`
      - Time range: `tenant_id: "t-123", period: "7d"`
      - Billing period: `tenant_id: "t-123", period: "30d"`

      **Decision guide:**
      - Per-tenant details? → tenant_usage (this tool)
      - Quota config? → tenant_quotas
      - Storage breakdown? → storage_stats with tenant_id
      - All tenants overview? → tenant_quotas with include_usage: true

      **Performance tips:**
      - Lightweight read-only operation
      - Usage data may be cached; period determines granularity
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "tenant_id" => %{
            type: "string",
            description: "ID of the tenant to get usage for"
          },
          "period" => %{
            type: "string",
            enum: ["1h", "24h", "7d", "30d", "90d"],
            description: "Time period for usage data (default: 24h)"
          },
          "include_breakdown" => %{
            type: "boolean",
            description: "Include usage breakdown by event type (default: false)"
          }
        },
        required: ["tenant_id"]
      }
    }
  end

  defp tool_tenant_quotas do
    %{
      name: "tenant_quotas",
      description: """
      Get quota configuration and enforcement status for a tenant. Shows current \
      limits, usage against limits, and whether quotas are being enforced.

      **ADMIN ONLY** — Requires admin role to execute.

      **When to use this tool:**
      - Reviewing a tenant's quota configuration
      - Checking if a tenant is near quota limits
      - Auditing quota enforcement settings
      - Planning quota adjustments before tenant_update

      **Common patterns:**
      - View quotas: `tenant_id: "t-123"`
      - With usage: `tenant_id: "t-123", include_usage: true`
      - Check enforcement: look for `enforced: true/false` in results

      **Key quota types:**
      - `max_events_per_day` — daily event ingestion limit
      - `max_storage_bytes` — total storage capacity
      - `max_api_calls_per_minute` — rate limiting
      - `max_event_size_bytes` — per-event size limit

      **Performance tips:**
      - Lightweight read-only operation
      - Use include_usage to see current consumption vs limits
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "tenant_id" => %{
            type: "string",
            description: "ID of the tenant to get quotas for"
          },
          "include_usage" => %{
            type: "boolean",
            description: "Include current usage against each quota (default: false)"
          }
        },
        required: ["tenant_id"]
      }
    }
  end

  defp tool_tenant_suspend do
    %{
      name: "tenant_suspend",
      description: """
      Suspend a tenant, preventing all API access and event ingestion. This is a \
      soft disable — data is preserved and the tenant can be reactivated.

      **ADMIN ONLY** — Requires admin role to execute.

      **SAFETY WARNING:**
      - Suspended tenants cannot ingest events or query data
      - All API keys for the tenant are temporarily disabled
      - Existing data is preserved — this is NOT a deletion
      - The tenant can be reactivated with tenant_update (status: "active")

      **When to use this tool:**
      - Suspending a tenant for non-payment or policy violation
      - Temporarily disabling a tenant during maintenance
      - Emergency response to abuse or security incidents
      - Planned decommissioning (suspend before export and delete)

      **Common patterns:**
      - Suspend: `tenant_id: "t-123", reason: "non-payment"`
      - Emergency: `tenant_id: "t-123", reason: "security incident", notify: true`
      - Planned: `tenant_id: "t-123", reason: "scheduled maintenance"`

      **Performance tips:**
      - Takes effect immediately across all API endpoints
      - Does not affect data durability — events are preserved
      - Reactivate with tenant_update setting status to "active"
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "tenant_id" => %{
            type: "string",
            description: "ID of the tenant to suspend"
          },
          "reason" => %{
            type: "string",
            description: "Reason for suspension (recorded in audit log)"
          },
          "notify" => %{
            type: "boolean",
            description: "Send notification to tenant contacts (default: false)"
          }
        },
        required: ["tenant_id", "reason"]
      }
    }
  end

  defp tool_tenant_export do
    %{
      name: "tenant_export",
      description: """
      Export all data for a tenant including events, projections, and metadata. \
      Generates a complete data package for migration, backup, or compliance (GDPR).

      **ADMIN ONLY** — Requires admin role to execute.

      **When to use this tool:**
      - GDPR data portability requests (right to data portability)
      - Migrating a tenant to another deployment
      - Creating offline backups of tenant data
      - Pre-deletion data archive for compliance

      **SAFETY WARNING:**
      - Export may be large — check tenant_usage first
      - Long-running operation for tenants with many events
      - Export includes all event data, metadata, and projections

      **Common patterns:**
      - Full export: `tenant_id: "t-123"`
      - Format choice: `tenant_id: "t-123", format: "jsonl"`
      - With date range: `tenant_id: "t-123", since: "2024-01-01T00:00:00Z"`
      - Size estimate: use tenant_usage first to check data volume

      **Performance tips:**
      - Use tenant_usage to estimate export size before running
      - JSONL format is most efficient for large exports
      - Consider time-range filters for very large tenants
      - Export runs asynchronously — check status with the returned job_id
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "tenant_id" => %{
            type: "string",
            description: "ID of the tenant to export data for"
          },
          "format" => %{
            type: "string",
            enum: ["json", "jsonl", "csv"],
            description: "Export format (default: json)"
          },
          "since" => %{
            type: "string",
            description: "Export events since this ISO timestamp"
          },
          "until" => %{
            type: "string",
            description: "Export events until this ISO timestamp"
          },
          "include_projections" => %{
            type: "boolean",
            description: "Include projection data in export (default: true)"
          },
          "include_metadata" => %{
            type: "boolean",
            description: "Include tenant metadata in export (default: true)"
          }
        },
        required: ["tenant_id"]
      }
    }
  end

  # ============================================================================
  # Multi-Tenancy Tool Handlers
  # ============================================================================

  @doc false
  def handle_tenant_create(args, state, format) do
    params =
      %{"name" => Map.fetch!(args, "name")}
      |> maybe_put("tenant_id", Map.get(args, "tenant_id"))
      |> maybe_put("plan", Map.get(args, "plan"))
      |> maybe_put("max_events_per_day", Map.get(args, "max_events_per_day"))
      |> maybe_put("max_storage_bytes", Map.get(args, "max_storage_bytes"))
      |> maybe_put("metadata", Map.get(args, "metadata"))

    case ControlPlaneClient.tenant_create(state.control_client, params) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)
        tenant_id = Map.get(data, "tenant_id", Map.get(data, "id", "unknown"))

        text = """
        🏢 Tenant Created
        🆔 Tenant ID: #{tenant_id}
        📛 Name: #{Map.fetch!(args, "name")}

        #{formatted_data}

        💡 Use tenant_quotas to review quota configuration
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to create tenant: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_tenant_update(args, state, format) do
    tenant_id = Map.fetch!(args, "tenant_id")

    params =
      %{}
      |> maybe_put("name", Map.get(args, "name"))
      |> maybe_put("plan", Map.get(args, "plan"))
      |> maybe_put("max_events_per_day", Map.get(args, "max_events_per_day"))
      |> maybe_put("max_storage_bytes", Map.get(args, "max_storage_bytes"))
      |> maybe_put("metadata", Map.get(args, "metadata"))

    case ControlPlaneClient.tenant_update(state.control_client, tenant_id, params) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        text = """
        ✏️ Tenant Updated
        🆔 Tenant ID: #{tenant_id}

        #{formatted_data}
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to update tenant: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_tenant_usage(args, state, format) do
    tenant_id = Map.fetch!(args, "tenant_id")

    params =
      %{}
      |> maybe_put("period", Map.get(args, "period"))
      |> maybe_put("include_breakdown", Map.get(args, "include_breakdown"))

    case ControlPlaneClient.tenant_usage(state.control_client, tenant_id, params) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        text = """
        📊 Tenant Usage
        🆔 Tenant ID: #{tenant_id}

        #{formatted_data}
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to get tenant usage: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_tenant_quotas(args, state, format) do
    tenant_id = Map.fetch!(args, "tenant_id")

    params =
      %{}
      |> maybe_put("include_usage", Map.get(args, "include_usage"))

    case ControlPlaneClient.tenant_quotas(state.control_client, tenant_id, params) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        text = """
        📋 Tenant Quotas
        🆔 Tenant ID: #{tenant_id}

        #{formatted_data}
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to get tenant quotas: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_tenant_suspend(args, state, format) do
    tenant_id = Map.fetch!(args, "tenant_id")
    reason = Map.fetch!(args, "reason")

    params =
      %{"reason" => reason}
      |> maybe_put("notify", Map.get(args, "notify"))

    case ControlPlaneClient.tenant_suspend(state.control_client, tenant_id, params) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        text = """
        ⏸️ Tenant Suspended
        🆔 Tenant ID: #{tenant_id}
        📝 Reason: #{reason}

        #{formatted_data}

        💡 Reactivate with tenant_update setting status to "active"
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason_msg} ->
        {:error, "Failed to suspend tenant: #{inspect(reason_msg)}"}
    end
  end

  @doc false
  def handle_tenant_export(args, state, format) do
    tenant_id = Map.fetch!(args, "tenant_id")

    params =
      %{}
      |> maybe_put("format", Map.get(args, "format"))
      |> maybe_put("since", Map.get(args, "since"))
      |> maybe_put("until", Map.get(args, "until"))
      |> maybe_put("include_projections", Map.get(args, "include_projections"))
      |> maybe_put("include_metadata", Map.get(args, "include_metadata"))

    case ControlPlaneClient.tenant_export(state.control_client, tenant_id, params) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        text = """
        📤 Tenant Data Export
        🆔 Tenant ID: #{tenant_id}

        #{formatted_data}
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to export tenant data: #{inspect(reason)}"}
    end
  end

  # ============================================================================
  # Fleet Health & Recovery Tool Definitions (P3)
  # ============================================================================
  #
  # These tools live in mcp-server-elixir, NOT prime-mcp. prime-mcp is
  # single-tenant by design (apps/prime-mcp/src/projection_registry.rs:22;
  # apps/prime-mcp/src/http.rs:294,379) — a fleet/cross-tenant tool there would
  # cross a tenant boundary it explicitly does not have. The Elixir server
  # already composes from state.control_client (a ControlPlaneClient) and gates
  # tenant tools behind control_plane_enabled, so it is the correct home.
  #
  # They are THIN CONSUMERS of the Control Plane /api/v1/admin/* fleet+recovery
  # API. No tier scoring and no guard logic is reimplemented here: the health
  # model and the dry-run / typed-confirmation / blast-radius guards live
  # entirely in the Control Plane. Each tool forwards dry_run + confirmation
  # params straight through and surfaces the server's `would` preview +
  # confirm_token. The only local logic is the system_admin gate (call_tool/3)
  # that hides the mutating recovery_* tools by default.
  # ============================================================================

  defp tool_fleet_health_summary do
    %{
      name: "fleet_health_summary",
      description: """
      Fleet-wide tenant health rollup — the single answer to "is every tenant \
      healthy right now, and if not, which ones aren't and why?". Read-only.

      **ADMIN ONLY** — Reaches the Control Plane /api/v1/admin/fleet/health \
      endpoint, which requires an admin JWT.

      **When to use this tool:**
      - Daily fleet sweep: how many tenants are Healthy / Degraded / At-Risk / Critical
      - Triage after an incident — see the worst-N tenants and their contributing signals
      - Before a billing or edition change, to confirm nothing is already on fire

      **What it returns:**
      - Per-tier counts (total / healthy / degraded / at_risk / critical)
      - The worst-N tenants (Critical→Degraded) each with its `reasons` list \
        (signal name + observed value + the tier it triggered)

      **Common patterns:**
      - Whole fleet: (no args)
      - Only the broken ones: `tier: "critical"` or `tier: "at_risk"`
      - Cap the worst list: `limit: 10`

      **Note:** This tool computes nothing — tiers and signals are scored by the \
      Control Plane. Drill into a single tenant with tenant_health_assessment.
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "tier" => %{
            type: "string",
            enum: ["critical", "at_risk", "degraded", "healthy"],
            description: "Optional filter: only return tenants at this health tier"
          },
          "limit" => %{
            type: "integer",
            description: "Worst-N tenants to include (default 25)"
          }
        },
        required: []
      }
    }
  end

  defp tool_tenant_health_assessment do
    %{
      name: "tenant_health_assessment",
      description: """
      Per-tenant health assessment — every signal with its observed value, the \
      tier it triggered, and the source backend it was read from. Read-only.

      **ADMIN ONLY** — Reaches the Control Plane /api/v1/admin/fleet/health/:id \
      endpoint, which requires an admin JWT.

      **When to use this tool:**
      - A tenant shows up non-green in fleet_health_summary and you need the why
      - Diagnosing the data-visibility class of incident (edition trap, \
        wrong-tenant, quota over-run, dunning, replication lag)
      - Confirming a tenant is healthy before/after a recovery action

      **What it returns:**
      - The tenant's overall tier
      - The full signal list (e.g. last_event_age, subscription_state, \
        events_quota_pct, durability, replication_lag) with values + source
      - The subscription snapshot

      **Important — read the runbook framing:** a tenant reporting empty data is \
      classified as a read-path / identity symptom, NEVER as data loss. Core is \
      the durable store; "no rows" points at edition/JWT/tenant resolution.

      **Common patterns:**
      - Assess: `tenant_id: "t-123"`
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "tenant_id" => %{
            type: "string",
            description: "ID of the tenant to assess"
          }
        },
        required: ["tenant_id"]
      }
    }
  end

  defp tool_recovery_resync do
    %{
      name: "recovery_resync",
      description: """
      Re-trigger ingestion / reconcile drifted counters for a tenant (Guarded \
      recovery). Remediates a stale last_sync_age or x402 counter drift.

      **ADMIN ONLY** — Requires admin role; gated behind system-admin mode \
      (ALLSOURCE_SYSTEM_ADMIN) on this server.

      **SAFETY WARNING:**
      - This mutates a live tenant. Dry-run runs by default; you must pass \
        confirmation (`dry_run: false`) to actually mutate.
      - On dry-run the Control Plane returns a `would` preview (what would be \
        re-pulled — count + range). Review it before applying.
      - The Control Plane enforces all guards server-side; this tool only \
        forwards your dry_run flag and reason.

      **When to use this tool:**
      - A tenant's last_sync_age is stale (sync source fell behind)
      - x402 allowance counters drifted from actual consumption

      **Common patterns:**
      - Preview: `tenant_id: "t-123"` (dry-run is the default)
      - Apply: `tenant_id: "t-123", dry_run: false, reason: "sync stalled 6h"`
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "tenant_id" => %{type: "string", description: "ID of the tenant to resync"},
          "dry_run" => %{
            type: "boolean",
            description: "preview only; default true. Pass false to mutate."
          },
          "reason" => %{type: "string", description: "Reason (recorded in the Core audit event)"}
        },
        required: ["tenant_id"]
      }
    }
  end

  defp tool_recovery_reconcile_subscription do
    %{
      name: "recovery_reconcile_subscription",
      description: """
      Re-apply a tenant's tier entitlements (Guarded recovery). Remediates quota \
      drift and retired-tier aliases that didn't map to current entitlements.

      **ADMIN ONLY** — Requires admin role; gated behind system-admin mode \
      (ALLSOURCE_SYSTEM_ADMIN) on this server.

      **SAFETY WARNING:**
      - Mutates a live tenant's stored quotas. Dry-run runs by default; you must \
        pass `dry_run: false` to mutate.
      - On dry-run the Control Plane returns the computed entitlements it would \
        write. Review before applying.
      - All guards are enforced server-side by the Control Plane.

      **When to use this tool:**
      - A tenant's stored quota drifted from its tier's entitlement
      - A retired-tier tenant needs its quotas re-derived (runbook "retired-tier backfill")

      **Common patterns:**
      - Preview: `tenant_id: "t-123"`
      - Apply: `tenant_id: "t-123", dry_run: false, reason: "quota drift"`
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "tenant_id" => %{type: "string", description: "ID of the tenant to reconcile"},
          "dry_run" => %{
            type: "boolean",
            description: "preview only; default true. Pass false to mutate."
          },
          "reason" => %{type: "string", description: "Reason (recorded in the Core audit event)"}
        },
        required: ["tenant_id"]
      }
    }
  end

  defp tool_recovery_resolve_dunning do
    %{
      name: "recovery_resolve_dunning",
      description: """
      Resolve dunning / past-due drift for a tenant (Guarded recovery). \
      Re-issues checkout, marks for manual review, or extends grace.

      **ADMIN ONLY** — Requires admin role; gated behind system-admin mode \
      (ALLSOURCE_SYSTEM_ADMIN) on this server.

      **SAFETY WARNING:**
      - Mutates a live tenant's billing state. Dry-run runs by default; you must \
        pass `dry_run: false` to mutate.
      - On dry-run the Control Plane returns the action it would take. Review first.
      - All guards are enforced server-side by the Control Plane.

      **When to use this tool:**
      - A tenant is stuck in past_due / unpaid / expired drift
      - LemonSqueezy retry state needs a manual nudge

      **Common patterns:**
      - Preview: `tenant_id: "t-123"`
      - Apply: `tenant_id: "t-123", dry_run: false, reason: "card updated, clear dunning"`
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "tenant_id" => %{type: "string", description: "ID of the tenant in dunning"},
          "dry_run" => %{
            type: "boolean",
            description: "preview only; default true. Pass false to mutate."
          },
          "reason" => %{type: "string", description: "Reason (recorded in the Core audit event)"}
        },
        required: ["tenant_id"]
      }
    }
  end

  defp tool_recovery_rotate_keys do
    %{
      name: "recovery_rotate_keys",
      description: """
      Rotate a tenant's API keys and re-mint with the canonical `serviceaccount` \
      role (Destructive recovery). Remediates role-string drift \
      (`service_account` vs `serviceaccount`) and suspected key compromise.

      **ADMIN ONLY** — Requires admin role; gated behind system-admin mode \
      (ALLSOURCE_SYSTEM_ADMIN) on this server.

      **SAFETY WARNING — DESTRUCTIVE:**
      - Rotation INVALIDATES the tenant's current keys — existing integrations \
        break until they pick up the new key.
      - Dry-run runs by default; you must pass confirmation to mutate. The \
        dry-run returns a `confirm_token` plus a "these N keys will stop working" \
        warning; echo that token back as `confirm_token` to apply.
      - The Control Plane enforces the confirm_token guard server-side — a raw \
        curl is bound by the same guard this tool surfaces.

      **When to use this tool:**
      - API-key role-string drift detected (keys silently 403)
      - Suspected key compromise

      **Common patterns:**
      - Preview (get confirm_token): `tenant_id: "t-123"`
      - Apply: `tenant_id: "t-123", dry_run: false, confirm_token: "<token-from-dry-run>", reason: "role drift"`
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "tenant_id" => %{type: "string", description: "ID of the tenant whose keys to rotate"},
          "dry_run" => %{
            type: "boolean",
            description: "preview only; default true. Pass false (with confirm_token) to mutate."
          },
          "confirm_token" => %{
            type: "string",
            description: "echo the token returned by the preceding dry-run to execute"
          },
          "reason" => %{type: "string", description: "Reason (recorded in the Core audit event)"}
        },
        required: ["tenant_id"]
      }
    }
  end

  defp tool_recovery_reprovision do
    %{
      name: "recovery_reprovision",
      description: """
      Re-provision a tenant's metadata (Destructive recovery). Remediates \
      corrupt/incomplete tenant metadata or a failed onboarding.

      **ADMIN ONLY** — Requires admin role; gated behind system-admin mode \
      (ALLSOURCE_SYSTEM_ADMIN) on this server.

      **SAFETY WARNING — DESTRUCTIVE:**
      - Re-provisioning REWRITES tenant metadata. A wrong id would clobber a \
        healthy paying tenant.
      - Dry-run runs by default; you must pass confirmation to mutate. To \
        execute you MUST type the exact tenant id into `confirm_tenant_id` \
        (it must equal `tenant_id`). The dry-run returns the metadata diff.
      - The Control Plane HARD-REJECTS reprovision on a tenant that is `active` \
        and has ingested in the last 24h (use recovery_resync instead). Max \
        blast radius is one tenant.
      - All guards are enforced server-side by the Control Plane.

      **When to use this tool:**
      - A tenant's metadata is corrupt/incomplete and recovery_resync won't fix it
      - A failed onboarding left the tenant in a half-provisioned state

      **Common patterns:**
      - Preview (see the diff): `tenant_id: "t-123", reason: "broken onboarding"`
      - Apply: `tenant_id: "t-123", dry_run: false, confirm_tenant_id: "t-123", reason: "broken onboarding"`
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "tenant_id" => %{type: "string", description: "ID of the tenant to re-provision"},
          "dry_run" => %{
            type: "boolean",
            description:
              "preview only; default true. Pass false (with confirm_tenant_id) to mutate."
          },
          "confirm_tenant_id" => %{
            type: "string",
            description: "must exactly equal tenant_id to execute (omit for dry-run)"
          },
          "reason" => %{type: "string", description: "Reason (recorded in the Core audit event)"}
        },
        required: ["tenant_id", "reason"]
      }
    }
  end

  defp tool_recovery_restore do
    %{
      name: "recovery_restore",
      description: """
      Restore a tenant from a snapshot / replay (Destructive recovery). \
      Remediates data corruption or a bad projection. Wraps the existing Core \
      ops (replay + backup) in a tenant-scoped guarded action.

      **ADMIN ONLY** — Requires admin role; gated behind system-admin mode \
      (ALLSOURCE_SYSTEM_ADMIN) on this server.

      **SAFETY WARNING — DESTRUCTIVE:**
      - A replay/restore can OVERWRITE newer events. You must see exactly which \
        snapshot and how far back before applying.
      - Dry-run runs by default; you must pass confirmation to mutate. The \
        dry-run returns the snapshot id, age, and event count to be replayed. To \
        execute, type the exact tenant id into `confirm_tenant_id` and name the \
        `snapshot_id`.
      - All guards are enforced server-side by the Control Plane.

      **When to use this tool:**
      - Confirmed data corruption or a bad projection for a single tenant
      - Targeted restore from a known-good snapshot

      **Common patterns:**
      - Preview: `tenant_id: "t-123", snapshot_id: "snap-9", reason: "bad projection"`
      - Apply: `tenant_id: "t-123", dry_run: false, confirm_tenant_id: "t-123", snapshot_id: "snap-9", reason: "bad projection"`
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "tenant_id" => %{type: "string", description: "ID of the tenant to restore"},
          "snapshot_id" => %{
            type: "string",
            description: "Snapshot/backup id to restore from (required to apply)"
          },
          "dry_run" => %{
            type: "boolean",
            description:
              "preview only; default true. Pass false (with confirm_tenant_id) to mutate."
          },
          "confirm_tenant_id" => %{
            type: "string",
            description: "must exactly equal tenant_id to execute (omit for dry-run)"
          },
          "reason" => %{type: "string", description: "Reason (recorded in the Core audit event)"}
        },
        required: ["tenant_id", "reason"]
      }
    }
  end

  defp tool_recovery_batch do
    %{
      name: "recovery_batch",
      description: """
      Apply ONE Guarded recovery action across many tenants \
      (Destructive-bounded). E.g. reconcile_subscription across all retired-tier \
      tenants (the runbook "retired-tier backfill").

      **ADMIN ONLY** — Requires admin role; gated behind system-admin mode \
      (ALLSOURCE_SYSTEM_ADMIN) on this server.

      **SAFETY WARNING — DESTRUCTIVE (widest blast radius):**
      - A batch is the single most dangerous surface — an unbounded "recover \
        everything" could mutate the whole customer base.
      - Only Guarded actions are allowed in batch: `resync`, \
        `reconcile_subscription`, `resolve_dunning`. The Destructive \
        single-tenant actions (reprovision / restore / rotate_keys) are \
        FORBIDDEN here.
      - Hard `max_tenants` cap (default 25, absolute ceiling 100), enforced \
        server-side by the Control Plane.
      - Dry-run runs by default; you must pass confirmation to mutate. The \
        dry-run returns the full affected-tenant list + per-tenant preview + a \
        `confirm_token` that echoes the exact count. To apply, echo that token \
        as `confirm_token`.
      - All guards are enforced server-side by the Control Plane.

      **Common patterns:**
      - Preview a backfill: `filter: {tier: "free"}, action: "reconcile_subscription"`
      - Apply: `filter: {tier: "free"}, action: "reconcile_subscription", dry_run: false, confirm_token: "<token>", max_tenants: 50`
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "filter" => %{
            type: "object",
            description: "status/tier/health-tier selector for the affected tenants"
          },
          "action" => %{
            type: "string",
            enum: ["resync", "reconcile_subscription", "resolve_dunning"],
            description: "the Guarded action to apply (Destructive actions forbidden in batch)"
          },
          "max_tenants" => %{
            type: "integer",
            description: "hard cap ≤ 100, default 25"
          },
          "dry_run" => %{
            type: "boolean",
            description: "preview only; default true. Pass false (with confirm_token) to mutate."
          },
          "confirm_token" => %{
            type: "string",
            description:
              "echo the token from the dry-run (it encodes the exact tenant count) to execute"
          },
          "reason" => %{type: "string", description: "Reason (recorded in the Core audit event)"}
        },
        required: ["filter", "action"]
      }
    }
  end

  defp tool_recovery_diagnose_edition do
    %{
      name: "recovery_diagnose_edition",
      description: """
      Detect the edition=community trap (Safe — read/diagnose-only). The \
      ALLSOURCE_EDITION=community setting on the Query Service pins EVERY \
      request to the `community` tenant → dashboards look empty despite synced \
      data (runbook §5 #5).

      **ADMIN ONLY** — Reaches the Control Plane \
      /api/v1/admin/recovery/diagnose/edition endpoint, which requires an admin JWT.

      **This tool does NOT mutate anything.** The edition is a single fleet-wide \
      switch that re-routes every request's tenant, so flipping it is \
      operator-executed, not done blindly by the API. The tool DETECTS the trap \
      (QS edition `community` + ≥2 non-`community` tenants with data) and returns \
      the exact remediation: a copy-paste `ALLSOURCE_EDITION=enterprise` command \
      for `allsource-query` plus a post-change verification probe. It also \
      reminds you that a `fly deploy` creating no new release is a no-op.

      **When to use this tool:**
      - "No data anywhere" symptom across the fleet despite confirmed sync
      - Before assuming data loss — the runbook's first edition check

      **Common patterns:**
      - Diagnose: (no args)
      """,
      inputSchema: %{
        type: "object",
        properties: %{},
        required: []
      }
    }
  end

  # ============================================================================
  # Admin Tenant-360 + Comms Tool Definitions (ADMIN_TENANT_POWER_TOOL §9 Phase 8)
  #
  # MCP parity for the admin power tool. These live HERE in mcp-server-elixir,
  # NEVER in prime-mcp: prime-mcp is single-tenant by design, so a per-tenant 360
  # and a cross-tenant cohort notice cross a tenant boundary it does not have.
  # Both are thin pass-throughs to the Control Plane /api/v1/admin/* group:
  # tenant_overview composes three EXISTING reads; tenant_notice forwards
  # dry_run + confirm_token to POST /api/v1/admin/notices and the CP enforces the
  # cohort blast-radius / confirm-token / opt-out / audit guards. No scoring or
  # guard decisions happen here.
  # ============================================================================

  defp tool_tenant_overview do
    %{
      name: "tenant_overview",
      description: """
      Per-tenant 360 — one read that composes the tenant's whole world: identity \
      + members + API keys + subscription, health signals/tier, and usage. \
      Read-only. The single answer to "show me everything about this tenant".

      **ADMIN ONLY** — Reaches the Control Plane /api/v1/admin/* group (tenant \
      detail + fleet health + usage), which requires an admin JWT.

      **When to use this tool:**
      - A customer emails "my dashboard is empty / I want to upgrade" and you need \
        their whole world in one shot
      - Triage before a lifecycle/recovery/comms action on a tenant
      - Confirm identity + entitlements + health together

      **What it returns** (one payload, three sections):
      - `identity` ← GET /api/v1/admin/tenants/:id (id, name, status, plan, \
        members, API keys, subscription)
      - `health`   ← GET /api/v1/admin/fleet/health/:id (every signal + the tier \
        it triggered)
      - `usage`    ← GET /api/v1/admin/tenants/:id/usage (events/queries over time)

      Each section is fetched independently — if one read fails it surfaces an \
      `{error: ...}` marker for that section instead of failing the whole 360 \
      (empty data is a read-path/identity symptom, NEVER data loss).

      **Note:** This tool computes nothing — it composes existing CP reads. Drill \
      into the health detail with tenant_health_assessment; send a notice with \
      tenant_notice.
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "tenant_id" => %{
            type: "string",
            description: "ID of the tenant to assemble the 360 for"
          },
          "period" => %{
            type: "string",
            description:
              "Optional usage window passed straight to GET …/tenants/:id/usage " <>
                "(e.g. 'day' | 'week' | 'month'); omit for the CP default"
          }
        },
        required: ["tenant_id"]
      }
    }
  end

  defp tool_tenant_notice do
    %{
      name: "tenant_notice",
      description: """
      Send an in-app notice to ONE tenant or a COHORT (Guarded — comms/mutation). \
      Posts to the Control Plane /api/v1/admin/notices endpoint (event-sourced; \
      the tenant dashboard renders it as a banner).

      **ADMIN ONLY** — Requires admin role; gated behind system-admin mode \
      (ALLSOURCE_SYSTEM_ADMIN) on this server, same bar as the Destructive \
      recovery tools.

      **SAFETY WARNING:**
      - This writes a durable notice. A COHORT send (audience by tier / plan / \
        health_tier) fans out to many tenants, so it runs dry-run by default; \
        you must pass `dry_run: false` to actually send.
      - On a cohort dry-run the Control Plane returns a `would` preview \
        (recipient_tenant_ids + count) AND a `confirm_token`. Review it, then \
        re-run with `dry_run: false` + that `confirm_token` to apply.
      - The Control Plane enforces every guard server-side (blast-radius cap, \
        confirm-token echo, opt-out, per-category rate-limit, Core audit event). \
        This tool only forwards your audience + `dry_run` + `confirm_token`.

      **Audience (exactly one form):**
      - Single tenant: `audience: {tenant_id: "t-123"}`
      - Cohort: `audience: {tier: "indie"}` | `{plan: "studio"}` | \
        `{health_tier: "at_risk"}`

      **Common patterns:**
      - One tenant: `audience: {tenant_id: "t-123"}, title: "Heads up", \
        body: "...", severity: "info"` (single sends apply directly)
      - Cohort preview: `audience: {health_tier: "critical"}, title: "...", \
        body: "...", severity: "warning"` (dry-run is the default → returns \
        recipients + confirm_token)
      - Cohort apply: same + `dry_run: false, confirm_token: "<echoed>"`
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "audience" => %{
            type: "object",
            description:
              "Who the notice targets. Exactly one of `tenant_id` (single) OR a " <>
                "cohort selector (`tier` | `plan` | `health_tier`).",
            properties: %{
              "tenant_id" => %{type: "string", description: "Single-tenant target"},
              "tier" => %{
                type: "string",
                description: "Cohort: billing tier (e.g. indie/studio/scale)"
              },
              "plan" => %{
                type: "string",
                description: "Cohort: alias for tier (admin filter vocabulary)"
              },
              "health_tier" => %{
                type: "string",
                enum: ["critical", "at_risk", "degraded", "healthy"],
                description: "Cohort: health-signals tier"
              }
            }
          },
          "title" => %{type: "string", description: "Notice title"},
          "body" => %{type: "string", description: "Notice body"},
          "severity" => %{
            type: "string",
            enum: ["info", "warning", "critical"],
            description: "Notice severity"
          },
          "expires_at" => %{
            type: "string",
            description: "Optional ISO-8601 expiry; omit for a non-expiring notice"
          },
          "dry_run" => %{
            type: "boolean",
            description:
              "Preview only (cohort blast-radius). Default true. Pass false " <>
                "(with the echoed confirm_token for a cohort) to actually send."
          },
          "confirm_token" => %{
            type: "string",
            description:
              "For a cohort apply: echo the confirm_token returned by the prior " <>
                "dry-run. The Control Plane validates it server-side."
          }
        },
        required: ["audience", "title", "body", "severity"]
      }
    }
  end

  # ============================================================================
  # Fleet Health & Recovery Tool Handlers (P3)
  #
  # Thin pass-throughs: compose the new ControlPlaneClient fns and format via
  # ToonEncoder.format_response/2. They do NOT recompute tiers or decide
  # confirmations — guards are enforced by the Control Plane. Mutating handlers
  # forward dry_run + confirmation params and surface the `would` preview +
  # any confirm_token the Control Plane returns.
  # ============================================================================

  @doc false
  def handle_fleet_health_summary(args, state, format) do
    params =
      %{}
      |> maybe_put("tier", Map.get(args, "tier"))
      |> maybe_put("limit", Map.get(args, "limit"))

    case ControlPlaneClient.fleet_health(state.control_client, params) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        text = """
        🩺 Fleet Health Summary
        (tiers + worst-N scored by the Control Plane; this tool computes nothing)

        #{formatted_data}

        💡 Drill into a tenant with tenant_health_assessment
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to get fleet health: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_tenant_health_assessment(args, state, format) do
    tenant_id = Map.fetch!(args, "tenant_id")

    case ControlPlaneClient.fleet_tenant_health(state.control_client, tenant_id) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        text = """
        🩺 Tenant Health Assessment
        🆔 Tenant ID: #{tenant_id}
        (signals + tier scored by the Control Plane; empty-data is a read-path/identity symptom, not data loss)

        #{formatted_data}
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to assess tenant health: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_recovery_resync(args, state, format) do
    tenant_id = Map.fetch!(args, "tenant_id")

    params =
      %{}
      |> maybe_put("dry_run", recovery_dry_run(args))
      |> maybe_put("reason", Map.get(args, "reason"))

    run_recovery(
      "force_resync",
      tenant_id,
      params,
      fn -> ControlPlaneClient.recovery_resync(state.control_client, tenant_id, params) end,
      format
    )
  end

  @doc false
  def handle_recovery_reconcile_subscription(args, state, format) do
    tenant_id = Map.fetch!(args, "tenant_id")

    params =
      %{}
      |> maybe_put("dry_run", recovery_dry_run(args))
      |> maybe_put("reason", Map.get(args, "reason"))

    run_recovery(
      "reconcile_subscription",
      tenant_id,
      params,
      fn ->
        ControlPlaneClient.recovery_reconcile_subscription(state.control_client, tenant_id, params)
      end,
      format
    )
  end

  @doc false
  def handle_recovery_resolve_dunning(args, state, format) do
    tenant_id = Map.fetch!(args, "tenant_id")

    params =
      %{}
      |> maybe_put("dry_run", recovery_dry_run(args))
      |> maybe_put("reason", Map.get(args, "reason"))

    run_recovery(
      "resolve_dunning",
      tenant_id,
      params,
      fn ->
        ControlPlaneClient.recovery_resolve_dunning(state.control_client, tenant_id, params)
      end,
      format
    )
  end

  @doc false
  def handle_recovery_rotate_keys(args, state, format) do
    tenant_id = Map.fetch!(args, "tenant_id")

    params =
      %{}
      |> maybe_put("dry_run", recovery_dry_run(args))
      |> maybe_put("confirm_token", Map.get(args, "confirm_token"))
      |> maybe_put("reason", Map.get(args, "reason"))

    run_recovery(
      "rotate_keys",
      tenant_id,
      params,
      fn -> ControlPlaneClient.recovery_rotate_keys(state.control_client, tenant_id, params) end,
      format
    )
  end

  @doc false
  def handle_recovery_reprovision(args, state, format) do
    tenant_id = Map.fetch!(args, "tenant_id")
    params = reprovision_recovery_params(args)

    run_recovery(
      "reprovision",
      tenant_id,
      params,
      fn -> ControlPlaneClient.recovery_reprovision(state.control_client, tenant_id, params) end,
      format
    )
  end

  @doc false
  def handle_recovery_restore(args, state, format) do
    tenant_id = Map.fetch!(args, "tenant_id")

    params =
      %{}
      |> maybe_put("dry_run", recovery_dry_run(args))
      |> maybe_put("confirm_tenant_id", Map.get(args, "confirm_tenant_id"))
      |> maybe_put("snapshot_id", Map.get(args, "snapshot_id"))
      |> maybe_put("reason", Map.get(args, "reason"))

    run_recovery(
      "restore",
      tenant_id,
      params,
      fn -> ControlPlaneClient.recovery_restore(state.control_client, tenant_id, params) end,
      format
    )
  end

  @doc false
  def handle_recovery_batch(args, state, format) do
    params =
      %{}
      |> maybe_put("filter", Map.fetch!(args, "filter"))
      |> maybe_put("action", Map.fetch!(args, "action"))
      |> maybe_put("max_tenants", Map.get(args, "max_tenants"))
      |> maybe_put("dry_run", recovery_dry_run(args))
      |> maybe_put("confirm_token", Map.get(args, "confirm_token"))
      |> maybe_put("reason", Map.get(args, "reason"))

    run_recovery(
      "batch",
      "(fleet batch)",
      params,
      fn -> ControlPlaneClient.recovery_batch(state.control_client, params) end,
      format
    )
  end

  @doc false
  def handle_recovery_diagnose_edition(_args, state, format) do
    case ControlPlaneClient.recovery_diagnose_edition(state.control_client) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        text = """
        🔎 Edition Trap Diagnosis (read-only — no mutation)

        #{formatted_data}

        ⚠️  Flipping ALLSOURCE_EDITION is operator-executed: run the command above on
            allsource-query and confirm a NEW Fly release (a deploy that creates no
            release is a no-op).
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to diagnose edition trap: #{inspect(reason)}"}
    end
  end

  # ============================================================================
  # Admin Tenant-360 + Comms Tool Handlers (ADMIN_TENANT_POWER_TOOL §9 Phase 8)
  #
  # tenant_overview: pure READ — composes the three existing admin reads via
  # ControlPlaneClient.tenant_overview/3, formats one 360 payload, scores nothing.
  # tenant_notice: forwards audience + dry_run + confirm_token straight through to
  # POST /api/v1/admin/notices; the Control Plane enforces the cohort blast-radius
  # / confirm-token / opt-out / audit guards. Mirrors run_recovery's dry-run/
  # confirm-token surfacing.
  # ============================================================================

  @doc false
  def handle_tenant_overview(args, state, format) do
    tenant_id = Map.fetch!(args, "tenant_id")
    params = maybe_put(%{}, "period", Map.get(args, "period"))

    # tenant_overview/3 always returns {:ok, payload}: each sub-read is normalised
    # into its own 360 section (a failed read becomes an `{error: ...}` marker
    # INSIDE the payload), so a missing section never fails the whole 360.
    {:ok, data} = ControlPlaneClient.tenant_overview(state.control_client, tenant_id, params)
    formatted_data = ToonEncoder.format_response(data, format)

    text = """
    🪟 Tenant 360 Overview
    🆔 Tenant ID: #{tenant_id}
    (identity + health + usage composed from the Control Plane admin reads; this tool computes nothing)

    #{formatted_data}

    💡 Drill the health detail with tenant_health_assessment; send a notice with tenant_notice
    """

    {:ok, %{content: [%{type: "text", text: text}]}}
  end

  @doc false
  def handle_tenant_notice(args, state, format) do
    params = tenant_notice_params(args)
    cohort? = cohort_audience?(Map.get(args, "audience"))

    run_notice(
      params,
      cohort?,
      fn -> ControlPlaneClient.admin_create_notice(state.control_client, params) end,
      format
    )
  end

  @doc false
  # Builds the exact body forwarded to POST /api/v1/admin/notices. Public so the
  # dry_run / confirm_token pass-through can be asserted without a live CP:
  # - cohort + omit dry_run → "dry_run" => true (default-safe; no implicit blast)
  # - single-tenant + omit dry_run → "dry_run" => false (single sends apply)
  # - confirm_token forwarded verbatim when supplied (CP validates it server-side)
  # No scoring or guard decisions happen here — the CP enforces every guard.
  def tenant_notice_params(args) do
    audience = Map.get(args, "audience")

    %{}
    |> maybe_put("audience", audience)
    |> maybe_put("title", Map.get(args, "title"))
    |> maybe_put("body", Map.get(args, "body"))
    |> maybe_put("severity", Map.get(args, "severity"))
    |> maybe_put("expires_at", Map.get(args, "expires_at"))
    |> maybe_put("dry_run", notice_dry_run(args, audience))
    |> maybe_put("confirm_token", Map.get(args, "confirm_token"))
  end

  @doc false
  # Default-safe dry_run for notices: a COHORT send (tier/plan/health_tier) fans
  # out, so dry_run defaults ON unless the caller explicitly sets it. A SINGLE
  # tenant send has no blast radius, so it defaults OFF (applies directly), which
  # matches the CP convention. An explicit dry_run always wins.
  def notice_dry_run(args, audience) do
    case Map.get(args, "dry_run") do
      nil -> cohort_audience?(audience)
      explicit -> explicit
    end
  end

  @doc false
  # A cohort audience is any selector other than a single tenant_id. Mirrors the
  # Control Plane's NoticeAudience.isCohort (tier | plan | health_tier set, no
  # tenant_id). Used only to pick the default dry_run + the surfaced mode line.
  def cohort_audience?(audience) when is_map(audience) do
    tenant_id = Map.get(audience, "tenant_id") || Map.get(audience, :tenant_id)

    cohort =
      [
        Map.get(audience, "tier") || Map.get(audience, :tier),
        Map.get(audience, "plan") || Map.get(audience, :plan),
        Map.get(audience, "health_tier") || Map.get(audience, :health_tier)
      ]
      |> Enum.any?(&present?/1)

    is_nil_or_blank(tenant_id) and cohort
  end

  def cohort_audience?(_audience), do: false

  defp present?(nil), do: false
  defp present?(""), do: false
  defp present?(_), do: true

  defp is_nil_or_blank(nil), do: true
  defp is_nil_or_blank(""), do: true
  defp is_nil_or_blank(_), do: false

  # Shared formatter for tenant_notice. Surfaces whether the send was a dry-run
  # (cohort), the Control Plane's `would` recipient preview, and any confirm_token
  # it returned (echo to apply). Recomputes/decides nothing — the CP owns guards.
  defp run_notice(params, cohort?, fun, format) do
    dry_run? = Map.get(params, "dry_run", cohort?)

    case fun.() do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)
        confirm_token = if is_map(data), do: Map.get(data, "confirm_token"), else: nil

        mode_line =
          cond do
            dry_run? ->
              "🧪 DRY-RUN — no notice was sent. Review the `would` recipients below, then re-run with dry_run: false" <>
                if cohort?, do: " + the confirm_token to apply.", else: " to apply."

            true ->
              "✅ SENT — the Control Plane created the notice (and wrote a Core audit event)."
          end

        token_line =
          if confirm_token,
            do:
              "\n🔑 confirm_token: #{confirm_token}\n   Echo this token back (with dry_run: false) to apply the cohort send.",
            else: ""

        text = """
        📣 Tenant Notice#{if cohort?, do: " (cohort)", else: ""}
        #{mode_line}#{token_line}

        #{formatted_data}
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to send tenant notice: #{inspect(reason)}"}
    end
  end

  # Default-safe dry_run: Destructive recovery tools default dry_run ON. If the
  # caller omits dry_run we send true; we never mutate implicitly. This is the
  # only local default — the Control Plane still enforces every guard server-side.
  @doc false
  def recovery_dry_run(args), do: Map.get(args, "dry_run", true)

  @doc false
  # Builds the exact param map forwarded to the Control Plane for a reprovision.
  # Public so the dry-run pass-through can be asserted without a live CP:
  # - omit dry_run → "dry_run" => true (default-safe; no implicit mutation)
  # - omit confirm_tenant_id → key absent (no confirmation supplied = no mutate)
  # No tier scoring or guard decisions happen here — the CP enforces those.
  def reprovision_recovery_params(args) do
    %{}
    |> maybe_put("dry_run", recovery_dry_run(args))
    |> maybe_put("confirm_tenant_id", Map.get(args, "confirm_tenant_id"))
    |> maybe_put("reason", Map.get(args, "reason"))
  end

  # Shared formatter for the mutating recovery handlers. Surfaces whether the
  # call was a dry-run, the Control Plane's `would` preview, and any confirm_token
  # it returned. Recomputes/decides nothing.
  defp run_recovery(action, target, params, fun, format) do
    dry_run? = Map.get(params, "dry_run", true)

    case fun.() do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)
        confirm_token = if is_map(data), do: Map.get(data, "confirm_token"), else: nil

        mode_line =
          if dry_run? do
            "🧪 DRY-RUN — no changes were made. Review the `would` preview below, then re-run with dry_run: false + confirmation to apply."
          else
            "✅ APPLIED — the Control Plane executed this action (and wrote a Core audit event)."
          end

        token_line =
          if confirm_token,
            do: "\n🔑 confirm_token: #{confirm_token}\n   Echo this token back to apply the change.",
            else: ""

        text = """
        🛠️  Recovery: #{action}
        🎯 Target: #{target}
        #{mode_line}#{token_line}

        #{formatted_data}
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Recovery #{action} failed: #{inspect(reason)}"}
    end
  end

  # ============================================================================
  # Schema & Validation Tool Definitions
  # ============================================================================

  defp tool_register_schema do
    %{
      name: "register_schema",
      description: """
      Register a JSON Schema for an event type (subject). Schemas enable \
      validation of event payloads before ingestion, ensuring data quality.

      **When to use this tool:**
      - Defining the expected structure for a new event type
      - Enforcing data contracts between producers and consumers
      - Setting up schema governance before going to production
      - Adding validation to existing event types retroactively

      **Common patterns:**
      - Register basic schema: `subject: "user.created", definition: {"type": "object", ...}`
      - With description: `subject: "order.placed", definition: {...}, description: "Order placement event"`
      - With compatibility: `subject: "payment.processed", definition: {...}, compatibility: "backward"`

      **Performance tips:**
      - Schema registration is a lightweight metadata operation
      - Schemas are cached in Core for fast validation
      - Use list_schemas to verify registration

      **Decision guide:**
      - "Define event structure?" → register_schema
      - "Check if data matches schema?" → validate_schema
      - "Compare schema versions?" → schema_diff
      - "Auto-generate from samples?" → infer_schema
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "subject" => %{
            type: "string",
            description: "Schema subject name, typically the event type (e.g., 'user.created')"
          },
          "definition" => %{
            type: "object",
            description: "JSON Schema definition (draft-07 compatible)"
          },
          "description" => %{
            type: "string",
            description: "Human-readable description of what this schema represents"
          },
          "compatibility" => %{
            type: "string",
            enum: ["none", "backward", "forward", "full"],
            description:
              "Compatibility mode for schema evolution (default: backward). " <>
                "backward = new schema can read old data, " <>
                "forward = old schema can read new data, " <>
                "full = both directions"
          },
          "tags" => %{
            type: "array",
            items: %{type: "string"},
            description: "Tags for categorizing and filtering schemas"
          }
        },
        required: ["subject", "definition"]
      }
    }
  end

  defp tool_validate_schema do
    %{
      name: "validate_schema",
      description: """
      Validate an event payload against a registered schema. Returns validation \
      result with detailed error messages for any violations.

      **When to use this tool:**
      - Testing event payloads before ingestion
      - Debugging schema validation failures
      - Verifying data quality in CI/CD pipelines
      - Checking if existing events conform to updated schemas

      **Common patterns:**
      - Validate payload: `subject: "user.created", payload: {"name": "Alice", ...}`
      - Validate against specific version: `subject: "user.created", version: 2, payload: {...}`

      **Performance tips:**
      - Validation is fast (sub-millisecond for typical schemas)
      - Use this before ingest_event to catch errors early
      - Batch validation is not supported — validate one payload at a time

      **Decision guide:**
      - "Does this payload match the schema?" → validate_schema
      - "What schemas exist?" → list_schemas
      - "Register a new schema?" → register_schema
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "subject" => %{
            type: "string",
            description: "Schema subject to validate against"
          },
          "payload" => %{
            type: "object",
            description: "Event payload to validate"
          },
          "version" => %{
            type: "number",
            description: "Specific schema version to validate against (default: latest)"
          }
        },
        required: ["subject", "payload"]
      }
    }
  end

  defp tool_migrate_schema do
    %{
      name: "migrate_schema",
      description: """
      Register a new schema version with optional transformation metadata. \
      Uses Core's schema registry API for storage, with client-side diff \
      computation for dry-run previews.

      **Important:** This tool does NOT transform existing events. It registers \
      a new schema version in Core and records transformation metadata. The diff \
      preview (dry_run mode) is computed client-side by comparing JSON Schema \
      properties — it is a structural comparison, not a full compatibility check.

      **When to use this tool:**
      - Registering a new schema version for an evolving event type
      - Previewing what changed before registering (dry_run: true)
      - Recording transformation intent as metadata alongside new versions

      **Common patterns:**
      - Add field: `subject: "user.created", new_definition: {...}, transformations: [{"op": "add", "path": "/email_verified", "value": false}]`
      - Rename field: `subject: "order.placed", new_definition: {...}, transformations: [{"op": "rename", "from": "/amount", "to": "/total_amount"}]`
      - Dry run: `..., dry_run: true` to preview structural diff

      **How it works:**
      - **dry_run: true** → Fetches current schema from Core, computes client-side diff (field additions/removals/modifications)
      - **dry_run: false** → Calls Core's `POST /api/v1/schemas` to register the new version
      - Transformation rules are stored as metadata — they do not auto-apply to existing events

      **Decision guide:**
      - "Evolve schema safely?" → migrate_schema
      - "Compare two versions?" → schema_diff
      - "Check compatibility?" → migrate_schema with dry_run: true
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "subject" => %{
            type: "string",
            description: "Schema subject to migrate"
          },
          "new_definition" => %{
            type: "object",
            description: "New JSON Schema definition"
          },
          "transformations" => %{
            type: "array",
            items: %{
              type: "object",
              properties: %{
                "op" => %{type: "string", description: "Operation: add, remove, rename, modify"},
                "path" => %{type: "string", description: "JSON pointer path"},
                "from" => %{type: "string", description: "Source path (for rename)"},
                "to" => %{type: "string", description: "Target path (for rename)"},
                "value" => %{description: "Default value (for add)"}
              },
              required: ["op"]
            },
            description: "Transformation rules for migrating data between versions"
          },
          "description" => %{
            type: "string",
            description: "Description of what changed in this version"
          },
          "dry_run" => %{
            type: "boolean",
            description: "Check compatibility without registering (default: false)"
          }
        },
        required: ["subject", "new_definition"]
      }
    }
  end

  defp tool_list_schemas do
    %{
      name: "list_schemas",
      description: """
      List all registered schema subjects with their metadata. Provides an \
      overview of the schema registry for governance and discovery.

      **When to use this tool:**
      - Discovering what event types have schemas defined
      - Auditing schema coverage across event types
      - Finding schemas by tags or description
      - Getting an overview of schema governance status

      **Common patterns:**
      - List all: no parameters needed
      - Filter by tag: `tag: "billing"` to find billing-related schemas
      - Include versions: `include_versions: true` to see version counts

      **Performance tips:**
      - Lightweight metadata read operation
      - Results are cached — use refresh: true for real-time data
      - For detailed version info, use schema_diff with a specific subject

      **Decision guide:**
      - "What schemas exist?" → list_schemas
      - "Get specific schema?" → use subject name with register_schema or schema_diff
      - "Validate data against schema?" → validate_schema
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "tag" => %{
            type: "string",
            description: "Filter schemas by tag"
          },
          "include_versions" => %{
            type: "boolean",
            description: "Include version count per subject (default: false)"
          },
          "refresh" => %{
            type: "boolean",
            description: "Force refresh cached results (default: false)"
          }
        }
      }
    }
  end

  defp tool_infer_schema do
    %{
      name: "infer_schema",
      description: """
      Auto-generate a JSON Schema from sample events. Fetches events from \
      Core, then performs client-side type inference on the payloads.

      **How it works:** Queries Core for recent events of the given type, then \
      analyzes their `data` fields client-side to infer JSON Schema properties, \
      types, and required fields. The inference is basic: it collects value types \
      per field and marks fields present in all samples as required.

      **When to use this tool:**
      - Bootstrapping schemas for existing event types without definitions
      - Understanding the structure of unfamiliar event data
      - Generating a starting point schema to refine manually

      **Common patterns:**
      - Infer from event type: `event_type: "user.created"` (samples recent events)
      - With sample size: `event_type: "order.placed", sample_size: 100`
      - From specific entity: `entity_id: "user-123", event_type: "user.updated"`

      **Limitations:**
      - Inference is client-side — no Core endpoint involved beyond event fetching
      - Type detection is basic (string/number/boolean/array/object/null)
      - Does not detect patterns (email, UUID, date formats) or enum values
      - Inferred schemas are suggestions — always review before registering

      **Decision guide:**
      - "No schema exists, want to create one?" → infer_schema then register_schema
      - "Schema exists, want to validate data?" → validate_schema
      - "Want to compare inferred vs registered?" → infer_schema then schema_diff
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "event_type" => %{
            type: "string",
            description: "Event type to infer schema from"
          },
          "entity_id" => %{
            type: "string",
            description: "Optionally limit to events from a specific entity"
          },
          "sample_size" => %{
            type: "number",
            description: "Number of events to sample for inference (default: 50, max: 1000)"
          }
        },
        required: ["event_type"]
      }
    }
  end

  defp tool_schema_diff do
    %{
      name: "schema_diff",
      description: """
      Compare two schema versions to see what changed. Fetches schema \
      definitions from Core, then computes a structural diff client-side.

      **How it works:** Fetches schema definitions from Core's registry API, \
      then compares JSON Schema properties client-side. Reports added/removed/ \
      modified fields, required field changes, and flags breaking changes \
      (removed fields or newly-required fields).

      **Important:** The diff is a structural property comparison, not a full \
      JSON Schema compatibility check. It compares top-level properties and \
      required arrays — it does not analyze nested schemas, `allOf`/`anyOf` \
      compositions, or semantic compatibility modes.

      **Common patterns:**
      - Compare versions: `subject: "user.created", version_a: 1, version_b: 2`
      - Compare latest with previous: `subject: "user.created"` (defaults to last two versions)
      - Compare with proposed: `subject: "user.created", version_a: 2, proposed: {...}`

      **Decision guide:**
      - "What changed between versions?" → schema_diff
      - "Is this change safe?" → schema_diff (check breaking_changes field)
      - "Evolve schema?" → migrate_schema
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "subject" => %{
            type: "string",
            description: "Schema subject to compare versions for"
          },
          "version_a" => %{
            type: "number",
            description: "First version to compare (default: latest - 1)"
          },
          "version_b" => %{
            type: "number",
            description: "Second version to compare (default: latest)"
          },
          "proposed" => %{
            type: "object",
            description: "Proposed schema to compare against version_a (instead of version_b)"
          }
        },
        required: ["subject"]
      }
    }
  end

  # ============================================================================
  # Advanced Analytics Tool Definitions
  # ============================================================================

  defp tool_cohort_analysis do
    %{
      name: "cohort_analysis",
      description: """
      Get event frequency data grouped by time period. Returns time-bucketed \
      event counts from Core's frequency analytics endpoint.

      **Important:** This is a basic frequency analysis tool, not a true cohort \
      retention analysis. It returns event counts per time bucket — it does NOT \
      group entities by their first event, does NOT track retention rates, and \
      does NOT compute cohort-over-cohort comparisons. For actual cohort analysis, \
      query events directly and compute retention in your application.

      **When to use this tool:**
      - Getting event volume over time for a specific event type
      - Understanding activity trends across time periods
      - Exploring whether event volume is growing or declining

      **What it actually returns:**
      - Time-bucketed event counts from Core's `/api/v1/analytics/frequency` endpoint
      - Buckets at the requested granularity (day/week/month)
      - Total event count across all buckets

      **Common patterns:**
      - Weekly activity: `event_type: "user.active", granularity: "week"`
      - Daily volume: `event_type: "session.start", granularity: "day"`

      **Recommended workflow:** Call `list_schemas` to discover event types available, \
      then call `quick_stats` to understand data volume before running analysis.
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "event_type" => %{
            type: "string",
            description: "Event type to track for activity (e.g., 'session.start', 'purchase')"
          },
          "cohort_event_type" => %{
            type: "string",
            description:
              "Event type that defines cohort membership (default: same as event_type). " <>
                "E.g., 'user.created' to cohort by signup date"
          },
          "granularity" => %{
            type: "string",
            enum: ["day", "week", "month"],
            description: "Time granularity for cohort periods (default: week)"
          },
          "periods" => %{
            type: "number",
            description: "Number of periods to track after cohort formation (default: 8)"
          },
          "since" => %{
            type: "string",
            description: "Start date for cohort analysis (ISO 8601)"
          },
          "until" => %{
            type: "string",
            description: "End date for cohort analysis (ISO 8601)"
          },
          "tenant_id" => %{
            type: "string",
            description: "Filter to specific tenant"
          }
        },
        required: ["event_type"]
      }
    }
  end

  defp tool_correlation_analysis do
    %{
      name: "correlation_analysis",
      description: """
      Analyze temporal co-occurrence between two event types using Core's \
      correlation analytics endpoint. Measures how often events of type B occur \
      near events of type A for the same entity within a configurable time window.

      **When to use this tool:**
      - Discovering which events frequently occur together for the same entity
      - Checking if event A tends to be followed by event B within a time window
      - Understanding temporal proximity between event types

      **Methodology:**
      - Queries both event types from Core, groups by entity
      - Counts pairs where event B occurs within `time_window` seconds of event A (for same entity)
      - Returns: total counts, correlated pair count, correlation percentage, average time between, and up to 5 examples
      - Note: this is temporal co-occurrence, not a statistical correlation coefficient

      **Common patterns:**
      - Basic correlation: `event_type_a: "page.viewed", event_type_b: "purchase.completed"`
      - With time window: `..., time_window: "1h"` (events within 1 hour)
      - Multi-type: `event_type_a: "signup", event_type_b: "first_purchase"`

      **Performance tips:**
      - Narrow time ranges improve performance significantly
      - Use tenant_id filter on multi-tenant datasets
      - Large time windows (>24h) with high-volume events may be slow

      **Decision guide:**
      - "Do these events correlate?" → correlation_analysis
      - "What patterns exist?" → find_patterns (broader analysis)
      - "What's the user journey?" → path_analysis
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "event_type_a" => %{
            type: "string",
            description: "First event type to correlate"
          },
          "event_type_b" => %{
            type: "string",
            description: "Second event type to correlate"
          },
          "time_window" => %{
            type: "string",
            description:
              "Maximum time between correlated events (e.g., '1h', '24h', '7d'). Default: '1h'"
          },
          "since" => %{
            type: "string",
            description: "Start of analysis period (ISO 8601)"
          },
          "until" => %{
            type: "string",
            description: "End of analysis period (ISO 8601)"
          },
          "tenant_id" => %{
            type: "string",
            description: "Filter to specific tenant"
          }
        },
        required: ["event_type_a", "event_type_b"]
      }
    }
  end

  defp tool_forecast_events do
    %{
      name: "forecast_events",
      description: """
      Basic linear trend forecast of future event volumes. Fetches historical \
      frequency data from Core and applies simple linear regression to project ahead.

      **Important:** This uses basic linear regression (y = mx + b), not sophisticated \
      time series methods. It does NOT detect seasonality, does NOT use ARIMA/Prophet, \
      and works best for roughly linear trends. Treat results as rough estimates.

      **When to use this tool:**
      - Getting a rough directional forecast of event volume trends
      - Quick capacity planning estimates
      - Understanding if volume is trending up or down

      **What it actually computes:**
      - Fetches event counts per time bucket from Core's frequency endpoint
      - Fits a linear regression (slope + intercept) to historical bucket counts
      - Projects forward with 95% confidence intervals based on standard deviation
      - Returns: trend direction, slope, historical mean, and per-period predictions

      **Common patterns:**
      - Volume forecast: `event_type: "user.created", horizon: 30, granularity: "day"`
      - Weekly forecast: `event_type: "order.placed", horizon: 12, granularity: "week"`

      **Limitations:**
      - Needs at least 2 historical data points; more data = better estimates
      - Assumes linear trend — poor for seasonal or cyclical patterns
      - Confidence intervals are based on overall variance, not prediction-specific
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "event_type" => %{
            type: "string",
            description: "Event type to forecast (or omit for total volume)"
          },
          "granularity" => %{
            type: "string",
            enum: ["hour", "day", "week", "month"],
            description: "Forecast granularity (default: day)"
          },
          "horizon" => %{
            type: "number",
            description: "Number of periods to forecast ahead (default: 14)"
          },
          "history_periods" => %{
            type: "number",
            description: "Number of historical periods to use (default: 3x horizon)"
          },
          "tenant_id" => %{
            type: "string",
            description: "Filter to specific tenant"
          }
        }
      }
    }
  end

  defp tool_segment_analysis do
    %{
      name: "segment_analysis",
      description: """
      Get summary statistics for an event type. Returns aggregate counts, top \
      entities, and top event types from Core's summary analytics endpoint.

      **Important:** This is a basic summary statistics tool, not a behavioral \
      segmentation engine. It does NOT group entities into segments, does NOT \
      compute RFM scores, and does NOT apply clustering algorithms. The `metric` \
      and `segments` parameters are accepted but not used in computation. \
      For actual segmentation, query events directly and compute segments in \
      your application.

      **When to use this tool:**
      - Getting an overview of event activity for a specific event type
      - Seeing which entities are most active (top entities by event count)
      - Understanding event type distribution in your data

      **What it actually returns:**
      - Total event count, unique entities, unique event types
      - Top 10 event types by count (with percentages)
      - Top 10 entities by event count (with percentages)
      - Time range and events-per-day rate

      **Recommended workflow:** Call `list_schemas` to discover event types available, \
      then call `quick_stats` to understand data volume before running analysis.
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "event_type" => %{
            type: "string",
            description: "Event type to segment by"
          },
          "metric" => %{
            type: "string",
            enum: ["frequency", "recency", "monetary_value", "rfm"],
            description:
              "Segmentation metric (default: frequency). " <>
                "rfm = Recency-Frequency-Monetary combined scoring"
          },
          "segments" => %{
            type: "number",
            description: "Number of segments to create (default: 5)"
          },
          "since" => %{
            type: "string",
            description: "Start of analysis period (ISO 8601)"
          },
          "until" => %{
            type: "string",
            description: "End of analysis period (ISO 8601)"
          },
          "tenant_id" => %{
            type: "string",
            description: "Filter to specific tenant"
          }
        },
        required: ["event_type"]
      }
    }
  end

  defp tool_path_analysis do
    %{
      name: "path_analysis",
      description: """
      Get summary statistics for an event type, presented as path context. \
      Returns aggregate statistics from Core's summary analytics endpoint.

      **Important:** This is NOT a real funnel or path analysis tool. It does NOT \
      track event sequences per entity, does NOT compute conversion rates between \
      steps, does NOT identify drop-off points, and does NOT discover common paths. \
      The `steps`, `depth`, and `max_duration` parameters are accepted but not used \
      in computation. For actual funnel analysis, query events per entity and compute \
      step-to-step conversion in your application.

      **When to use this tool:**
      - Getting summary statistics for an event type as context for path investigation
      - Understanding overall event volume before manual path analysis

      **What it actually returns:**
      - Same as segment_analysis: total event count, top entities, top event types, \
        time range, and events-per-day rate from Core's summary endpoint

      **For real path analysis, consider:**
      - Use `query_events` with entity_id to trace a specific entity's journey
      - Use `correlation_analysis` to check if two event types co-occur temporally
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "steps" => %{
            type: "array",
            items: %{type: "string"},
            description:
              "Ordered list of event types defining the funnel/path. " <>
                "E.g., [\"signup\", \"activation\", \"purchase\"]"
          },
          "start_event" => %{
            type: "string",
            description: "Starting event type for open path discovery (alternative to steps)"
          },
          "depth" => %{
            type: "number",
            description: "Max path depth for open path discovery (default: 5)"
          },
          "max_duration" => %{
            type: "string",
            description: "Max time for entire path (e.g., '24h', '7d'). Default: no limit"
          },
          "since" => %{
            type: "string",
            description: "Start of analysis period (ISO 8601)"
          },
          "until" => %{
            type: "string",
            description: "End of analysis period (ISO 8601)"
          },
          "tenant_id" => %{
            type: "string",
            description: "Filter to specific tenant"
          }
        }
      }
    }
  end

  defp tool_attribution_analysis do
    %{
      name: "attribution_analysis",
      description: """
      Get summary statistics for a target event type, presented as attribution \
      context. Returns aggregate statistics from Core's summary analytics endpoint.

      **Important:** This is NOT a real attribution modeling tool. It does NOT \
      track touchpoints preceding conversions, does NOT apply attribution models \
      (first-touch, last-touch, linear, time-decay), and does NOT distribute credit \
      across events. The `model`, `touchpoint_types`, and `lookback` parameters are \
      accepted but not used in computation. For actual attribution, query events \
      per entity and compute attribution in your application.

      **When to use this tool:**
      - Getting summary statistics for a conversion event type
      - Understanding event volume context before manual attribution analysis

      **What it actually returns:**
      - Total event count, top entities, top event types, time range, and \
        events-per-day rate from Core's summary endpoint

      **For real attribution analysis, consider:**
      - Use `query_events` per entity to trace touchpoints before conversion
      - Use `correlation_analysis` to check if specific event types precede conversions
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "target_event" => %{
            type: "string",
            description: "Conversion event type to attribute (e.g., 'purchase', 'signup')"
          },
          "model" => %{
            type: "string",
            enum: ["first_touch", "last_touch", "linear", "time_decay"],
            description: "Attribution model to apply (default: linear)"
          },
          "touchpoint_types" => %{
            type: "array",
            items: %{type: "string"},
            description:
              "Event types to consider as touchpoints (default: all events before target)"
          },
          "lookback" => %{
            type: "string",
            description: "How far back to look for touchpoints (e.g., '7d', '30d'). Default: '30d'"
          },
          "since" => %{
            type: "string",
            description: "Start of analysis period (ISO 8601)"
          },
          "until" => %{
            type: "string",
            description: "End of analysis period (ISO 8601)"
          },
          "tenant_id" => %{
            type: "string",
            description: "Filter to specific tenant"
          }
        },
        required: ["target_event"]
      }
    }
  end

  defp tool_churn_prediction do
    %{
      name: "churn_prediction",
      description: """
      Get summary statistics for an activity event type, presented as churn \
      context. Returns aggregate statistics from Core's summary analytics endpoint.

      **Important:** This is NOT a real churn prediction tool. It does NOT compute \
      churn risk scores (0.0-1.0), does NOT compare recent vs historical activity, \
      does NOT identify at-risk entities, and does NOT analyze frequency decline. \
      The `lookback`, `risk_threshold`, `include_factors`, and `limit` parameters \
      are accepted but not used in computation. For actual churn analysis, query \
      events per entity and compute activity trends in your application.

      **When to use this tool:**
      - Getting summary statistics for an activity event type
      - Understanding overall activity volume as context for churn investigation

      **What it actually returns:**
      - Total event count, top entities, top event types, time range, and \
        events-per-day rate from Core's summary endpoint

      **For real churn analysis, consider:**
      - Use `query_events` per entity to check recent activity gaps
      - Use `segment_analysis` for basic summary of most/least active entities
      - Use `correlation_analysis` to check if disengagement events precede churn
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "activity_event" => %{
            type: "string",
            description: "Event type that indicates activity (e.g., 'session.start', 'api.call')"
          },
          "lookback" => %{
            type: "string",
            description: "Historical period to analyze (e.g., '30d', '90d'). Default: '30d'"
          },
          "risk_threshold" => %{
            type: "number",
            description: "Minimum churn risk score to include in results (0.0-1.0, default: 0.5)"
          },
          "include_factors" => %{
            type: "boolean",
            description: "Include contributing factors for each entity's score (default: false)"
          },
          "limit" => %{
            type: "number",
            description: "Maximum entities to return (default: 100)"
          },
          "tenant_id" => %{
            type: "string",
            description: "Filter to specific tenant"
          }
        },
        required: ["activity_event"]
      }
    }
  end

  defp tool_ltv_calculation do
    %{
      name: "ltv_calculation",
      description: """
      Get summary statistics for a value event type, presented as LTV context. \
      Returns aggregate statistics from Core's summary analytics endpoint.

      **Important:** This is NOT a real LTV calculation tool. It does NOT sum \
      monetary values per entity, does NOT use the `value_field` parameter to \
      extract amounts from event data, does NOT project future value, and does \
      NOT group by entity attributes. The `value_field`, `projection_months`, and \
      `group_by` parameters are accepted but not used in computation. For actual \
      LTV calculation, query events per entity and sum values in your application.

      **When to use this tool:**
      - Getting summary statistics for a value/revenue event type
      - Understanding event volume context before manual LTV analysis

      **What it actually returns:**
      - Total event count, top entities, top event types, time range, and \
        events-per-day rate from Core's summary endpoint

      **For real LTV calculation, consider:**
      - Use `query_events` with entity_id to get all value events for an entity
      - Sum the `value_field` from event data to compute historical LTV
      - Use `segment_analysis` to see which entities generate the most events
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "value_event" => %{
            type: "string",
            description: "Event type representing value (e.g., 'purchase', 'payment.received')"
          },
          "value_field" => %{
            type: "string",
            description: "Field in event data containing monetary value (e.g., 'amount', 'total')"
          },
          "projection_months" => %{
            type: "number",
            description: "Months to project forward (default: 0 = historical only)"
          },
          "group_by" => %{
            type: "string",
            description: "Group LTV by entity attribute (e.g., 'plan', 'channel', 'region')"
          },
          "since" => %{
            type: "string",
            description: "Start of analysis period (ISO 8601)"
          },
          "until" => %{
            type: "string",
            description: "End of analysis period (ISO 8601)"
          },
          "tenant_id" => %{
            type: "string",
            description: "Filter to specific tenant"
          }
        },
        required: ["value_event", "value_field"]
      }
    }
  end

  # ============================================================================
  # Developer Experience Tool Definitions
  # ============================================================================

  defp tool_generate_client do
    %{
      name: "generate_client",
      description: """
      Generate SDK code for interacting with AllSource Core. Produces \
      type-safe client code in multiple languages based on Core's API.

      **When to use this tool:**
      - Bootstrapping integration code for a new project
      - Generating type-safe API clients for your language
      - Getting code examples for specific Core operations
      - Creating TypeScript types from registered schemas

      **Common patterns:**
      - TypeScript client: `language: "typescript", operations: ["ingest", "query"]`
      - Python client: `language: "python", operations: ["ingest", "query", "search"]`
      - With schemas: `language: "typescript", include_schemas: true` (generates types from registered schemas)
      - Specific operation: `language: "go", operations: ["ingest"]`

      **Performance tips:**
      - Generation is a pure computation, no Core API calls needed
      - include_schemas fetches current schemas from registry
      - Generated code includes error handling and retry logic
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "language" => %{
            type: "string",
            enum: ["typescript", "python", "go", "rust", "elixir"],
            description: "Target programming language"
          },
          "operations" => %{
            type: "array",
            items: %{type: "string"},
            description:
              "Operations to generate code for. Options: ingest, query, search, " <>
                "snapshot, reconstruct, schema, analytics. Default: all"
          },
          "include_schemas" => %{
            type: "boolean",
            description: "Generate type definitions from registered schemas (default: false)"
          },
          "style" => %{
            type: "string",
            enum: ["minimal", "full", "example"],
            description:
              "Code style: minimal (just the essentials), full (with docs and error " <>
                "handling), example (with usage examples). Default: full"
          }
        },
        required: ["language"]
      }
    }
  end

  defp tool_mock_events do
    %{
      name: "mock_events",
      description: """
      Generate realistic test/mock events for development and testing. Creates \
      synthetic events that match registered schemas or common patterns.

      **When to use this tool:**
      - Creating test data for development environments
      - Populating staging with realistic event data
      - Testing query performance with representative data volumes
      - Generating sample data for demos and documentation

      **Common patterns:**
      - From schema: `event_type: "user.created", count: 100` (uses registered schema)
      - Custom template: `event_type: "purchase", count: 50, template: {"amount": "random:10-500"}`
      - Time series: `event_type: "session.start", count: 1000, time_range: {"since": "2024-01-01", "until": "2024-12-31"}`
      - Multi-entity: `event_type: "page.viewed", count: 500, entity_count: 50`

      **SAFETY WARNING:**
      - Generated events are REAL events that will be ingested into the store
      - Use a test tenant_id to avoid mixing with production data
      - Consider using dry_run: true first to preview generated events

      **Performance tips:**
      - Batch sizes are automatically optimized
      - Large counts (>10000) are streamed in batches
      - Schema-aware generation produces more realistic data
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "event_type" => %{
            type: "string",
            description: "Event type to generate"
          },
          "count" => %{
            type: "number",
            description: "Number of events to generate (default: 10, max: 100000)"
          },
          "entity_count" => %{
            type: "number",
            description: "Number of unique entities to distribute events across (default: count/5)"
          },
          "template" => %{
            type: "object",
            description:
              "Event data template with value generators. Use 'random:min-max' for numbers, " <>
                "'choice:a,b,c' for enums, 'uuid' for IDs, 'timestamp' for dates"
          },
          "time_range" => %{
            type: "object",
            properties: %{
              "since" => %{type: "string", description: "Start of time range (ISO 8601)"},
              "until" => %{type: "string", description: "End of time range (ISO 8601)"}
            },
            description: "Distribute events across this time range (default: last 30 days)"
          },
          "tenant_id" => %{
            type: "string",
            description: "Tenant ID for generated events (recommended: use test tenant)"
          },
          "dry_run" => %{
            type: "boolean",
            description: "Preview generated events without ingesting (default: false)"
          }
        },
        required: ["event_type"]
      }
    }
  end

  defp tool_debug_query do
    %{
      name: "debug_query",
      description: """
      Get query execution plan and optimization hints. Shows how Core would \
      execute a query, including index usage, partition scanning, and estimated cost.

      **When to use this tool:**
      - Understanding why a query is slow
      - Optimizing query filters for better performance
      - Learning which indexes are being used
      - Comparing query strategies before execution

      **Common patterns:**
      - Debug existing query: `entity_id: "user-123", event_type: "purchase"`
      - Time range query: `since: "2024-01-01", until: "2024-06-01", event_type: "order.placed"`
      - Search query: `semantic_query: "failed payments", threshold: 0.8`

      **Output includes:**
      - Execution plan steps (scan, filter, sort, limit)
      - Index usage (which indexes would be hit)
      - Estimated cost (relative measure of work)
      - Optimization suggestions (missing indexes, better filters)
      - Estimated result count

      **Performance tips:**
      - This is a dry-run operation — no actual query execution
      - Use this before running expensive queries on large datasets
      - Optimization suggestions are actionable recommendations
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "entity_id" => %{
            type: "string",
            description: "Entity ID filter to debug"
          },
          "event_type" => %{
            type: "string",
            description: "Event type filter to debug"
          },
          "since" => %{
            type: "string",
            description: "Time range start (ISO 8601)"
          },
          "until" => %{
            type: "string",
            description: "Time range end (ISO 8601)"
          },
          "semantic_query" => %{
            type: "string",
            description: "Semantic search query to debug"
          },
          "limit" => %{
            type: "number",
            description: "Result limit to factor into plan"
          }
        }
      }
    }
  end

  defp tool_benchmark_query do
    %{
      name: "benchmark_query",
      description: """
      Run performance benchmarks on queries. Executes a query multiple times \
      and reports latency statistics (p50, p95, p99, mean, min, max).

      **When to use this tool:**
      - Measuring actual query performance under realistic conditions
      - Comparing query strategies by latency
      - Establishing performance baselines before and after changes
      - Validating SLA compliance for specific query patterns

      **Common patterns:**
      - Basic benchmark: `query: {"entity_id": "user-123"}, iterations: 50`
      - Time range query: `query: {"event_type": "purchase", "since": "2024-01-01"}, iterations: 100`
      - Warm cache: `query: {...}, iterations: 100, warmup: 10`

      **SAFETY WARNING:**
      - Benchmark queries execute REAL queries against the store
      - High iteration counts on production can impact performance
      - Use warmup iterations for more accurate results
      - Consider running during off-peak hours

      **Output includes:**
      - Latency percentiles: p50, p95, p99
      - Mean, min, max latency
      - Throughput (queries per second)
      - Result count consistency check

      **Performance tips:**
      - 50-100 iterations typically sufficient for stable percentiles
      - Use warmup: 5-10 to prime caches before measuring
      - Results may vary under different load conditions
      """,
      inputSchema: %{
        type: "object",
        properties: %{
          "query" => %{
            type: "object",
            description:
              "Query parameters to benchmark (same as query_events parameters: " <>
                "entity_id, event_type, since, until, limit)"
          },
          "iterations" => %{
            type: "number",
            description: "Number of query executions (default: 50, max: 1000)"
          },
          "warmup" => %{
            type: "number",
            description: "Warmup iterations before measuring (default: 5)"
          }
        },
        required: ["query"]
      }
    }
  end

  # ============================================================================
  # Schema & Validation Handlers
  # ============================================================================

  @doc false
  def handle_register_schema(args, state, format) do
    params =
      %{"subject" => Map.fetch!(args, "subject"), "definition" => Map.fetch!(args, "definition")}
      |> maybe_put("description", Map.get(args, "description"))
      |> maybe_put("compatibility", Map.get(args, "compatibility"))
      |> maybe_put("tags", Map.get(args, "tags"))

    case state.backend.register_schema(params) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        text = """
        📋 Schema Registered
        📛 Subject: #{Map.fetch!(args, "subject")}

        #{formatted_data}

        💡 Use validate_schema to test payloads against this schema
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to register schema: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_validate_schema(args, state, format) do
    params =
      %{"subject" => Map.fetch!(args, "subject"), "payload" => Map.fetch!(args, "payload")}
      |> maybe_put("version", Map.get(args, "version"))

    case state.backend.validate_schema(params) do
      {:ok, data} ->
        valid = Map.get(data, "valid", false)
        icon = if valid, do: "✅", else: "❌"
        formatted_data = ToonEncoder.format_response(data, format)

        text = """
        #{icon} Schema Validation: #{if valid, do: "PASSED", else: "FAILED"}
        📛 Subject: #{Map.fetch!(args, "subject")}

        #{formatted_data}
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to validate schema: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_migrate_schema(args, state, format) do
    subject = Map.fetch!(args, "subject")
    dry_run = Map.get(args, "dry_run", false)

    if dry_run do
      # Fetch current schema and compare with proposed
      case state.backend.get_schema(subject) do
        {:ok, current} ->
          new_def = Map.fetch!(args, "new_definition")

          diff = compute_schema_diff(Map.get(current, "definition", %{}), new_def)
          transformations = Map.get(args, "transformations", [])

          text = """
          🔍 Schema Migration Preview (DRY RUN)
          📛 Subject: #{subject}

          📊 Changes detected:
          #{format_diff_summary(diff)}

          🔄 Transformations: #{length(transformations)} rule(s) defined

          💡 Remove dry_run to register the new version
          """

          {:ok, %{content: [%{type: "text", text: text}]}}

        {:error, reason} ->
          {:error, "Failed to fetch current schema for comparison: #{inspect(reason)}"}
      end
    else
      params =
        %{
          "subject" => subject,
          "definition" => Map.fetch!(args, "new_definition")
        }
        |> maybe_put("description", Map.get(args, "description"))
        |> maybe_put("transformations", Map.get(args, "transformations"))

      case state.backend.register_schema(params) do
        {:ok, data} ->
          formatted_data = ToonEncoder.format_response(data, format)

          text = """
          🔄 Schema Migrated
          📛 Subject: #{subject}

          #{formatted_data}

          💡 Use schema_diff to review what changed
          """

          {:ok, %{content: [%{type: "text", text: text}]}}

        {:error, reason} ->
          {:error, "Failed to migrate schema: #{inspect(reason)}"}
      end
    end
  end

  @doc false
  def handle_list_schemas(args, state, format) do
    params =
      %{}
      |> maybe_put("tag", Map.get(args, "tag"))
      |> maybe_put("include_versions", Map.get(args, "include_versions"))
      |> maybe_put("refresh", Map.get(args, "refresh"))

    case state.backend.list_schemas(params) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        schemas = if is_list(data), do: data, else: Map.get(data, "subjects", data)
        count = if is_list(schemas), do: length(schemas), else: "N/A"

        text = """
        📋 Schema Registry
        📊 Subjects: #{count}

        #{formatted_data}
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to list schemas: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_infer_schema(args, state, format) do
    event_type = Map.fetch!(args, "event_type")
    sample_size = Map.get(args, "sample_size", 50)

    # Fetch sample events to infer schema from
    query_params =
      %{"event_type" => event_type, "limit" => sample_size}
      |> maybe_put("entity_id", Map.get(args, "entity_id"))

    case state.backend.query_events(query_params) do
      {:ok, data} ->
        events = Map.get(data, "events", [])

        if events == [] do
          text = """
          ⚠️ No events found for type "#{event_type}"
          Cannot infer schema without sample data.

          💡 Try a different event_type or check with list_schemas for existing schemas
          """

          {:ok, %{content: [%{type: "text", text: text}]}}
        else
          schema = infer_schema_from_events(events)
          formatted_schema = ToonEncoder.format_response(schema, format)

          text = """
          🔬 Inferred Schema
          📛 Event type: #{event_type}
          📊 Samples analyzed: #{length(events)}

          #{formatted_schema}

          💡 Review and refine, then use register_schema to register
          """

          {:ok, %{content: [%{type: "text", text: text}]}}
        end

      {:error, reason} ->
        {:error, "Failed to fetch events for schema inference: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_schema_diff(args, state, format) do
    subject = Map.fetch!(args, "subject")

    case Map.get(args, "proposed") do
      nil ->
        # Compare two registered versions
        version_a = Map.get(args, "version_a")
        version_b = Map.get(args, "version_b")

        with {:ok, versions} <- state.backend.list_schema_versions(subject) do
          version_list = if is_list(versions), do: versions, else: Map.get(versions, "versions", [])

          v_a = version_a || max(length(version_list) - 1, 1)
          v_b = version_b || length(version_list)

          with {:ok, schema_a} <-
                 state.backend.get_schema(subject, %{"version" => v_a}),
               {:ok, schema_b} <-
                 state.backend.get_schema(subject, %{"version" => v_b}) do
            def_a = Map.get(schema_a, "definition", %{})
            def_b = Map.get(schema_b, "definition", %{})

            diff = compute_schema_diff(def_a, def_b)
            formatted_diff = ToonEncoder.format_response(diff, format)

            text = """
            🔍 Schema Diff
            📛 Subject: #{subject}
            📊 Comparing v#{v_a} → v#{v_b}

            #{formatted_diff}
            """

            {:ok, %{content: [%{type: "text", text: text}]}}
          end
        else
          {:error, reason} ->
            {:error, "Failed to compare schema versions: #{inspect(reason)}"}
        end

      proposed ->
        # Compare registered version with proposed schema
        version_a = Map.get(args, "version_a")
        params = if version_a, do: %{"version" => version_a}, else: %{}

        case state.backend.get_schema(subject, params) do
          {:ok, schema_a} ->
            def_a = Map.get(schema_a, "definition", %{})
            diff = compute_schema_diff(def_a, proposed)
            formatted_diff = ToonEncoder.format_response(diff, format)

            v_label = if version_a, do: "v#{version_a}", else: "latest"

            text = """
            🔍 Schema Diff (vs Proposed)
            📛 Subject: #{subject}
            📊 Comparing #{v_label} → proposed

            #{formatted_diff}

            💡 Use migrate_schema to register the proposed version
            """

            {:ok, %{content: [%{type: "text", text: text}]}}

          {:error, reason} ->
            {:error, "Failed to fetch schema for diff: #{inspect(reason)}"}
        end
    end
  end

  # ============================================================================
  # Advanced Analytics Handlers
  # ============================================================================

  @doc false
  def handle_cohort_analysis(args, state, format) do
    event_type = Map.fetch!(args, "event_type")
    granularity = Map.get(args, "granularity", "week")
    periods = Map.get(args, "periods", 8)

    # Use frequency analytics to build cohort data
    params =
      %{"event_type" => event_type, "window" => granularity}
      |> maybe_put("since", Map.get(args, "since"))
      |> maybe_put("until", Map.get(args, "until"))
      |> maybe_put("tenant_id", Map.get(args, "tenant_id"))

    case state.backend.analytics_frequency(params) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        cohort_event = Map.get(args, "cohort_event_type", event_type)

        text = """
        👥 Cohort Analysis
        📊 Activity event: #{event_type}
        🏷️ Cohort event: #{cohort_event}
        📅 Granularity: #{granularity} | Periods: #{periods}

        #{formatted_data}

        💡 Use segment_analysis for behavioral segmentation
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to perform cohort analysis: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_correlation_analysis(args, state, format) do
    params =
      %{
        "event_type_a" => Map.fetch!(args, "event_type_a"),
        "event_type_b" => Map.fetch!(args, "event_type_b")
      }
      |> maybe_put("time_window", Map.get(args, "time_window"))
      |> maybe_put("since", Map.get(args, "since"))
      |> maybe_put("until", Map.get(args, "until"))
      |> maybe_put("tenant_id", Map.get(args, "tenant_id"))

    case state.backend.analytics_correlation(params) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        text = """
        🔗 Correlation Analysis
        📊 Event A: #{Map.fetch!(args, "event_type_a")}
        📊 Event B: #{Map.fetch!(args, "event_type_b")}

        #{formatted_data}

        💡 Use path_analysis to see the full journey between these events
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to perform correlation analysis: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_forecast_events(args, state, format) do
    granularity = Map.get(args, "granularity", "day")
    horizon = Map.get(args, "horizon", 14)
    history_periods = Map.get(args, "history_periods", horizon * 3)

    # Fetch historical frequency data
    params =
      %{"window" => granularity}
      |> maybe_put("event_type", Map.get(args, "event_type"))
      |> maybe_put("tenant_id", Map.get(args, "tenant_id"))

    case state.backend.analytics_frequency(params) do
      {:ok, data} ->
        buckets = if is_list(data), do: data, else: Map.get(data, "buckets", [])

        # Simple trend-based forecast from historical data
        forecast = compute_forecast(buckets, horizon, history_periods)
        formatted_data = ToonEncoder.format_response(forecast, format)

        event_label = Map.get(args, "event_type", "all events")

        text = """
        📈 Event Forecast
        🏷️ Event type: #{event_label}
        📅 Granularity: #{granularity} | Horizon: #{horizon} periods
        📊 History used: #{history_periods} periods

        #{formatted_data}

        ⚠️ Forecasts are approximations — use confidence intervals for planning
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to generate forecast: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_segment_analysis(args, state, format) do
    event_type = Map.fetch!(args, "event_type")
    metric = Map.get(args, "metric", "frequency")

    params =
      %{"event_type" => event_type}
      |> maybe_put("since", Map.get(args, "since"))
      |> maybe_put("until", Map.get(args, "until"))
      |> maybe_put("tenant_id", Map.get(args, "tenant_id"))

    case state.backend.analytics_summary(params) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        segments = Map.get(args, "segments", 5)

        text = """
        📊 Segment Analysis
        🏷️ Event type: #{event_type}
        📏 Metric: #{metric} | Segments: #{segments}

        #{formatted_data}

        💡 Use churn_prediction for at-risk entity identification
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to perform segment analysis: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_path_analysis(args, state, format) do
    steps = Map.get(args, "steps")
    start_event = Map.get(args, "start_event")

    unless steps || start_event do
      {:error,
       "Either 'steps' (funnel definition) or 'start_event' (open path discovery) is required"}
    else
      # Use summary analytics to analyze path data
      params =
        %{}
        |> maybe_put("event_type", start_event || List.first(steps || []))
        |> maybe_put("since", Map.get(args, "since"))
        |> maybe_put("until", Map.get(args, "until"))
        |> maybe_put("tenant_id", Map.get(args, "tenant_id"))

      case state.backend.analytics_summary(params) do
        {:ok, data} ->
          formatted_data = ToonEncoder.format_response(data, format)

          path_label =
            if steps,
              do: "Funnel: #{Enum.join(steps, " → ")}",
              else: "Open path from: #{start_event}"

          depth = Map.get(args, "depth", 5)
          max_duration = Map.get(args, "max_duration", "no limit")

          text = """
          🛤️ Path Analysis
          #{path_label}
          ⏱️ Max duration: #{max_duration} | Depth: #{depth}

          #{formatted_data}

          💡 Use correlation_analysis for pairwise event relationships
          """

          {:ok, %{content: [%{type: "text", text: text}]}}

        {:error, reason} ->
          {:error, "Failed to perform path analysis: #{inspect(reason)}"}
      end
    end
  end

  @doc false
  def handle_attribution_analysis(args, state, format) do
    target_event = Map.fetch!(args, "target_event")
    model = Map.get(args, "model", "linear")

    params =
      %{"event_type" => target_event}
      |> maybe_put("since", Map.get(args, "since"))
      |> maybe_put("until", Map.get(args, "until"))
      |> maybe_put("tenant_id", Map.get(args, "tenant_id"))

    case state.backend.analytics_summary(params) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        touchpoints = Map.get(args, "touchpoint_types", ["all events"])
        lookback = Map.get(args, "lookback", "30d")

        text = """
        🎯 Attribution Analysis
        🏷️ Target event: #{target_event}
        📊 Model: #{model}
        🔙 Lookback: #{lookback}
        📋 Touchpoints: #{Enum.join(touchpoints, ", ")}

        #{formatted_data}

        💡 Compare models by running with different model parameter values
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to perform attribution analysis: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_churn_prediction(args, state, format) do
    activity_event = Map.fetch!(args, "activity_event")

    params =
      %{"event_type" => activity_event}
      |> maybe_put("tenant_id", Map.get(args, "tenant_id"))

    case state.backend.analytics_summary(params) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        lookback = Map.get(args, "lookback", "30d")
        threshold = Map.get(args, "risk_threshold", 0.5)

        text = """
        ⚠️ Churn Prediction
        🏷️ Activity event: #{activity_event}
        🔙 Lookback: #{lookback}
        📊 Risk threshold: #{threshold}

        #{formatted_data}

        💡 Use segment_analysis for broader behavioral segmentation
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to perform churn prediction: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_ltv_calculation(args, state, format) do
    value_event = Map.fetch!(args, "value_event")
    value_field = Map.fetch!(args, "value_field")

    params =
      %{"event_type" => value_event}
      |> maybe_put("since", Map.get(args, "since"))
      |> maybe_put("until", Map.get(args, "until"))
      |> maybe_put("tenant_id", Map.get(args, "tenant_id"))

    case state.backend.analytics_summary(params) do
      {:ok, data} ->
        formatted_data = ToonEncoder.format_response(data, format)

        projection = Map.get(args, "projection_months", 0)
        group = Map.get(args, "group_by", "none")

        text = """
        💰 Lifetime Value Calculation
        🏷️ Value event: #{value_event}
        📊 Value field: #{value_field}
        📈 Projection: #{if projection > 0, do: "#{projection} months", else: "historical only"}
        📋 Group by: #{group}

        #{formatted_data}

        💡 Use churn_prediction to identify at-risk high-value entities
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to calculate LTV: #{inspect(reason)}"}
    end
  end

  # ============================================================================
  # Developer Experience Handlers
  # ============================================================================

  @doc false
  def handle_generate_client(args, _state, _format) do
    language = Map.fetch!(args, "language")
    operations = Map.get(args, "operations", ["ingest", "query", "search"])
    style = Map.get(args, "style", "full")

    code = generate_client_code(language, operations, style)

    text = """
    🔧 Generated #{String.capitalize(language)} Client
    📋 Operations: #{Enum.join(operations, ", ")}
    🎨 Style: #{style}

    ```#{language}
    #{code}
    ```

    💡 Customize the base_url and add authentication headers for your deployment
    """

    {:ok, %{content: [%{type: "text", text: text}]}}
  end

  @doc false
  def handle_mock_events(args, state, format) do
    event_type = Map.fetch!(args, "event_type")
    count = min(Map.get(args, "count", 10), 100_000)
    dry_run = Map.get(args, "dry_run", false)
    entity_count = Map.get(args, "entity_count", max(div(count, 5), 1))
    template = Map.get(args, "template", %{})

    events = generate_mock_events(event_type, count, entity_count, template, args)

    if dry_run do
      preview = Enum.take(events, 3)
      formatted_preview = ToonEncoder.format_response(preview, format)

      text = """
      🔍 Mock Events Preview (DRY RUN)
      🏷️ Event type: #{event_type}
      📊 Would generate: #{count} events across #{entity_count} entities

      Sample events:
      #{formatted_preview}

      💡 Remove dry_run to ingest these events
      """

      {:ok, %{content: [%{type: "text", text: text}]}}
    else
      # Ingest events in batches
      batch_size = 100
      batches = Enum.chunk_every(events, batch_size)

      results =
        Enum.map(batches, fn batch ->
          Enum.map(batch, fn event ->
            state.backend.ingest_event(event)
          end)
        end)

      success_count = results |> List.flatten() |> Enum.count(fn r -> match?({:ok, _}, r) end)
      error_count = count - success_count

      text = """
      ✅ Mock Events Generated
      🏷️ Event type: #{event_type}
      📊 Created: #{success_count}/#{count} events across #{entity_count} entities
      #{if error_count > 0, do: "❌ Errors: #{error_count}", else: ""}

      💡 Use query_events with event_type: "#{event_type}" to verify
      """

      {:ok, %{content: [%{type: "text", text: text}]}}
    end
  end

  @doc false
  def handle_debug_query(args, state, format) do
    # Build query params from args
    params =
      %{}
      |> maybe_put("entity_id", Map.get(args, "entity_id"))
      |> maybe_put("event_type", Map.get(args, "event_type"))
      |> maybe_put("since", Map.get(args, "since"))
      |> maybe_put("until", Map.get(args, "until"))
      |> maybe_put("limit", Map.get(args, "limit"))

    # Get stats to estimate query cost
    case state.backend.get_stats() do
      {:ok, stats} ->
        plan = build_query_plan(params, stats)
        formatted_plan = ToonEncoder.format_response(plan, format)

        text = """
        🔍 Query Debug Plan

        #{formatted_plan}
        """

        {:ok, %{content: [%{type: "text", text: text}]}}

      {:error, reason} ->
        {:error, "Failed to generate query plan: #{inspect(reason)}"}
    end
  end

  @doc false
  def handle_benchmark_query(args, state, format) do
    query = Map.fetch!(args, "query")
    iterations = min(Map.get(args, "iterations", 50), 1000)
    warmup = Map.get(args, "warmup", 5)

    # Run warmup iterations (discard results)
    for _i <- 1..warmup do
      state.backend.query_events(query)
    end

    # Run measured iterations
    timings =
      for _i <- 1..iterations do
        start = System.monotonic_time(:microsecond)
        result = state.backend.query_events(query)
        elapsed = System.monotonic_time(:microsecond) - start
        {elapsed, result}
      end

    latencies = Enum.map(timings, fn {elapsed, _} -> elapsed end) |> Enum.sort()
    success_count = Enum.count(timings, fn {_, r} -> match?({:ok, _}, r) end)

    stats = compute_latency_stats(latencies)
    formatted_stats = ToonEncoder.format_response(stats, format)

    text = """
    ⏱️ Benchmark Results
    📊 Iterations: #{iterations} (warmup: #{warmup})
    ✅ Success rate: #{success_count}/#{iterations}

    #{formatted_stats}

    💡 Use debug_query to understand the query execution plan
    """

    {:ok, %{content: [%{type: "text", text: text}]}}
  end

  # ============================================================================
  # Schema Helper Functions
  # ============================================================================

  defp compute_schema_diff(schema_a, schema_b) do
    props_a = Map.get(schema_a, "properties", %{})
    props_b = Map.get(schema_b, "properties", %{})

    keys_a = Map.keys(props_a) |> MapSet.new()
    keys_b = Map.keys(props_b) |> MapSet.new()

    added = MapSet.difference(keys_b, keys_a) |> MapSet.to_list()
    removed = MapSet.difference(keys_a, keys_b) |> MapSet.to_list()
    common = MapSet.intersection(keys_a, keys_b) |> MapSet.to_list()

    modified =
      Enum.filter(common, fn key ->
        Map.get(props_a, key) != Map.get(props_b, key)
      end)

    required_a = Map.get(schema_a, "required", []) |> MapSet.new()
    required_b = Map.get(schema_b, "required", []) |> MapSet.new()

    %{
      "added_fields" => added,
      "removed_fields" => removed,
      "modified_fields" => modified,
      "unchanged_fields" => common -- modified,
      "required_added" => MapSet.difference(required_b, required_a) |> MapSet.to_list(),
      "required_removed" => MapSet.difference(required_a, required_b) |> MapSet.to_list(),
      "breaking_changes" =>
        length(removed) > 0 ||
          MapSet.difference(required_b, required_a) |> MapSet.size() > 0,
      "summary" =>
        "#{length(added)} added, #{length(removed)} removed, #{length(modified)} modified"
    }
  end

  defp format_diff_summary(diff) do
    lines = []

    lines =
      if length(Map.get(diff, "added_fields", [])) > 0,
        do:
          lines ++
            ["  + Added: #{Enum.join(Map.get(diff, "added_fields", []), ", ")}"],
        else: lines

    lines =
      if length(Map.get(diff, "removed_fields", [])) > 0,
        do:
          lines ++
            ["  - Removed: #{Enum.join(Map.get(diff, "removed_fields", []), ", ")}"],
        else: lines

    lines =
      if length(Map.get(diff, "modified_fields", [])) > 0,
        do:
          lines ++
            ["  ~ Modified: #{Enum.join(Map.get(diff, "modified_fields", []), ", ")}"],
        else: lines

    lines =
      if Map.get(diff, "breaking_changes", false),
        do: lines ++ ["  ⚠️ BREAKING CHANGES detected"],
        else: lines ++ ["  ✅ No breaking changes"]

    Enum.join(lines, "\n")
  end

  defp infer_schema_from_events(events) do
    # Extract data fields from events and infer types
    data_fields =
      events
      |> Enum.map(fn event -> Map.get(event, "data", %{}) end)
      |> Enum.filter(&is_map/1)

    if data_fields == [] do
      %{"type" => "object", "properties" => %{}}
    else
      # Collect all keys and their value types
      field_types =
        Enum.reduce(data_fields, %{}, fn data, acc ->
          Enum.reduce(data, acc, fn {key, value}, inner_acc ->
            type = infer_json_type(value)
            existing = Map.get(inner_acc, key, MapSet.new())
            Map.put(inner_acc, key, MapSet.put(existing, type))
          end)
        end)

      # Determine which fields appear in all events (required)
      total = length(data_fields)

      field_counts =
        Enum.reduce(data_fields, %{}, fn data, acc ->
          Enum.reduce(Map.keys(data), acc, fn key, inner_acc ->
            Map.update(inner_acc, key, 1, &(&1 + 1))
          end)
        end)

      required =
        field_counts
        |> Enum.filter(fn {_key, count} -> count == total end)
        |> Enum.map(fn {key, _} -> key end)
        |> Enum.sort()

      properties =
        Enum.reduce(field_types, %{}, fn {key, types}, acc ->
          type =
            case MapSet.to_list(types) do
              [single] -> single
              multiple -> %{"oneOf" => Enum.map(multiple, fn t -> %{"type" => t} end)}
            end

          prop =
            if is_binary(type),
              do: %{"type" => type},
              else: type

          Map.put(acc, key, prop)
        end)

      result = %{
        "type" => "object",
        "properties" => properties
      }

      if required != [], do: Map.put(result, "required", required), else: result
    end
  end

  defp infer_json_type(value) when is_binary(value), do: "string"
  defp infer_json_type(value) when is_integer(value), do: "integer"
  defp infer_json_type(value) when is_float(value), do: "number"
  defp infer_json_type(value) when is_boolean(value), do: "boolean"
  defp infer_json_type(value) when is_list(value), do: "array"
  defp infer_json_type(value) when is_map(value), do: "object"
  defp infer_json_type(nil), do: "null"

  # ============================================================================
  # Analytics Helper Functions
  # ============================================================================

  defp compute_forecast(buckets, horizon, _history_periods) do
    values = Enum.map(buckets, fn b -> Map.get(b, "count", 0) end)

    if length(values) < 2 do
      %{
        "forecast" => [],
        "note" => "Insufficient historical data for forecasting (need at least 2 data points)"
      }
    else
      # Simple linear trend forecast
      n = length(values)
      mean = Enum.sum(values) / n

      # Compute trend (slope)
      indexed = Enum.with_index(values)

      sum_xy = Enum.reduce(indexed, 0, fn {v, i}, acc -> acc + v * i end)
      sum_x = Enum.reduce(0..(n - 1), 0, &(&1 + &2))
      sum_x2 = Enum.reduce(0..(n - 1), 0, fn i, acc -> acc + i * i end)

      slope =
        if n * sum_x2 - sum_x * sum_x != 0,
          do: (n * sum_xy - sum_x * Enum.sum(values)) / (n * sum_x2 - sum_x * sum_x),
          else: 0

      intercept = mean - slope * (n - 1) / 2

      # Compute standard deviation for confidence intervals
      variance =
        Enum.reduce(values, 0, fn v, acc -> acc + (v - mean) * (v - mean) end) / max(n - 1, 1)

      std_dev = :math.sqrt(variance)

      forecast =
        for i <- n..(n + horizon - 1) do
          predicted = max(intercept + slope * i, 0)

          %{
            "period" => i - n + 1,
            "predicted" => round(predicted),
            "confidence_low" => round(max(predicted - 1.96 * std_dev, 0)),
            "confidence_high" => round(predicted + 1.96 * std_dev)
          }
        end

      %{
        "trend" =>
          if(slope > 0, do: "increasing", else: if(slope < 0, do: "decreasing", else: "flat")),
        "slope" => Float.round(slope * 1.0, 2),
        "historical_mean" => Float.round(mean * 1.0, 1),
        "forecast" => forecast
      }
    end
  end

  # ============================================================================
  # Developer Experience Helper Functions
  # ============================================================================

  defp generate_client_code("typescript", operations, style) do
    base = """
    // AllSource Core TypeScript Client
    // Generated by AllSource MCP Server

    const BASE_URL = process.env.CORE_URL || "http://localhost:3900";

    interface CoreClientOptions {
      baseUrl?: string;
      headers?: Record<string, string>;
      timeout?: number;
    }
    """

    ops =
      Enum.map(operations, fn op ->
        generate_ts_operation(op, style)
      end)
      |> Enum.join("\n\n")

    base <> "\n" <> ops
  end

  defp generate_client_code("python", operations, style) do
    base = """
    # AllSource Core Python Client
    # Generated by AllSource MCP Server

    import os
    import requests
    from typing import Any, Dict, List, Optional

    BASE_URL = os.getenv("CORE_URL", "http://localhost:3900")
    """

    ops =
      Enum.map(operations, fn op ->
        generate_python_operation(op, style)
      end)
      |> Enum.join("\n\n")

    base <> "\n" <> ops
  end

  defp generate_client_code("go", operations, style) do
    base = """
    // AllSource Core Go Client
    // Generated by AllSource MCP Server

    package allsource

    import (
    \t"encoding/json"
    \t"fmt"
    \t"net/http"
    \t"os"
    )

    var baseURL = getEnv("CORE_URL", "http://localhost:3900")

    func getEnv(key, fallback string) string {
    \tif v := os.Getenv(key); v != "" {
    \t\treturn v
    \t}
    \treturn fallback
    }
    """

    ops =
      Enum.map(operations, fn op ->
        generate_go_operation(op, style)
      end)
      |> Enum.join("\n\n")

    base <> "\n" <> ops
  end

  defp generate_client_code(language, _operations, _style) do
    "// Client generation for #{language} is not yet supported.\n// Supported: typescript, python, go"
  end

  defp generate_ts_operation("ingest", _style) do
    """
    async function ingestEvent(event: {
      entity_id: string;
      event_type: string;
      data: Record<string, any>;
      metadata?: Record<string, any>;
    }): Promise<any> {
      const res = await fetch(`${BASE_URL}/api/v1/events`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(event),
      });
      if (!res.ok) throw new Error(`Ingest failed: ${res.status}`);
      return res.json();
    }
    """
  end

  defp generate_ts_operation("query", _style) do
    """
    async function queryEvents(params: {
      entity_id?: string;
      event_type?: string;
      since?: string;
      until?: string;
      limit?: number;
    }): Promise<{ events: any[]; count: number }> {
      const query = new URLSearchParams(
        Object.entries(params).filter(([_, v]) => v != null).map(([k, v]) => [k, String(v)])
      );
      const res = await fetch(`${BASE_URL}/api/v1/events/query?${query}`);
      if (!res.ok) throw new Error(`Query failed: ${res.status}`);
      return res.json();
    }
    """
  end

  defp generate_ts_operation("search", _style) do
    """
    async function semanticSearch(params: {
      query: string;
      limit?: number;
      threshold?: number;
    }): Promise<any> {
      const query = new URLSearchParams(
        Object.entries(params).filter(([_, v]) => v != null).map(([k, v]) => [k, String(v)])
      );
      const res = await fetch(`${BASE_URL}/api/v1/search/semantic?${query}`);
      if (!res.ok) throw new Error(`Search failed: ${res.status}`);
      return res.json();
    }
    """
  end

  defp generate_ts_operation(op, _style) do
    "// TODO: Implement #{op} operation"
  end

  defp generate_python_operation("ingest", _style) do
    """
    def ingest_event(entity_id: str, event_type: str, data: Dict[str, Any],
                     metadata: Optional[Dict] = None) -> Dict:
        payload = {"entity_id": entity_id, "event_type": event_type, "data": data}
        if metadata:
            payload["metadata"] = metadata
        resp = requests.post(f"{BASE_URL}/api/v1/events", json=payload)
        resp.raise_for_status()
        return resp.json()
    """
  end

  defp generate_python_operation("query", _style) do
    """
    def query_events(entity_id: Optional[str] = None, event_type: Optional[str] = None,
                     since: Optional[str] = None, until: Optional[str] = None,
                     limit: Optional[int] = None) -> Dict:
        params = {k: v for k, v in locals().items() if v is not None}
        resp = requests.get(f"{BASE_URL}/api/v1/events/query", params=params)
        resp.raise_for_status()
        return resp.json()
    """
  end

  defp generate_python_operation("search", _style) do
    """
    def semantic_search(query: str, limit: int = 100, threshold: float = 0.7) -> Dict:
        params = {"query": query, "limit": limit, "threshold": threshold}
        resp = requests.get(f"{BASE_URL}/api/v1/search/semantic", params=params)
        resp.raise_for_status()
        return resp.json()
    """
  end

  defp generate_python_operation(op, _style) do
    "# TODO: Implement #{op} operation"
  end

  defp generate_go_operation("ingest", _style) do
    """
    func IngestEvent(entityID, eventType string, data map[string]interface{}) error {
    \tbody, _ := json.Marshal(map[string]interface{}{
    \t\t"entity_id":  entityID,
    \t\t"event_type": eventType,
    \t\t"data":       data,
    \t})
    \tresp, err := http.Post(baseURL+"/api/v1/events", "application/json",
    \t\tbytes.NewReader(body))
    \tif err != nil { return err }
    \tdefer resp.Body.Close()
    \tif resp.StatusCode != 201 { return fmt.Errorf("ingest failed: %d", resp.StatusCode) }
    \treturn nil
    }
    """
  end

  defp generate_go_operation("query", _style) do
    """
    func QueryEvents(params map[string]string) (map[string]interface{}, error) {
    \treq, _ := http.NewRequest("GET", baseURL+"/api/v1/events/query", nil)
    \tq := req.URL.Query()
    \tfor k, v := range params { q.Set(k, v) }
    \treq.URL.RawQuery = q.Encode()
    \tresp, err := http.DefaultClient.Do(req)
    \tif err != nil { return nil, err }
    \tdefer resp.Body.Close()
    \tvar result map[string]interface{}
    \tjson.NewDecoder(resp.Body).Decode(&result)
    \treturn result, nil
    }
    """
  end

  defp generate_go_operation(op, _style) do
    "// TODO: Implement #{op} operation"
  end

  defp generate_mock_events(event_type, count, entity_count, template, args) do
    entity_ids =
      for i <- 1..entity_count, do: "mock-entity-#{:rand.uniform(100_000)}-#{i}"

    time_range = Map.get(args, "time_range", %{})
    now = DateTime.utc_now()

    since =
      case Map.get(time_range, "since") do
        nil -> DateTime.add(now, -30 * 24 * 3600, :second)
        ts -> parse_datetime(ts)
      end

    until_dt =
      case Map.get(time_range, "until") do
        nil -> now
        ts -> parse_datetime(ts)
      end

    range_seconds = DateTime.diff(until_dt, since, :second)

    for _i <- 1..count do
      entity_id = Enum.random(entity_ids)
      offset = :rand.uniform(max(range_seconds, 1))
      timestamp = DateTime.add(since, offset, :second) |> DateTime.to_iso8601()

      data =
        if template != %{} do
          Enum.reduce(template, %{}, fn {key, spec}, acc ->
            Map.put(acc, key, generate_value(spec))
          end)
        else
          %{"mock" => true, "generated_at" => DateTime.to_iso8601(now)}
        end

      event =
        %{
          "entity_id" => entity_id,
          "event_type" => event_type,
          "data" => data,
          "timestamp" => timestamp
        }
        |> maybe_put("tenant_id", Map.get(args, "tenant_id"))

      event
    end
  end

  defp generate_value("uuid") do
    <<a::32, b::16, c::16, d::16, e::48>> = :crypto.strong_rand_bytes(16)

    :io_lib.format("~8.16.0b-~4.16.0b-~4.16.0b-~4.16.0b-~12.16.0b", [a, b, c, d, e])
    |> to_string()
  end

  defp generate_value("random:" <> range) do
    case String.split(range, "-") do
      [min_s, max_s] ->
        {min_v, _} = Integer.parse(min_s)
        {max_v, _} = Integer.parse(max_s)
        :rand.uniform(max_v - min_v + 1) + min_v - 1

      _ ->
        :rand.uniform(100)
    end
  end

  defp generate_value("choice:" <> choices) do
    choices |> String.split(",") |> Enum.random() |> String.trim()
  end

  defp generate_value("timestamp") do
    DateTime.utc_now() |> DateTime.to_iso8601()
  end

  defp generate_value(other) when is_binary(other), do: other
  defp generate_value(other), do: other

  defp parse_datetime(ts) when is_binary(ts) do
    case DateTime.from_iso8601(ts) do
      {:ok, dt, _} -> dt
      _ -> DateTime.utc_now()
    end
  end

  defp parse_datetime(_), do: DateTime.utc_now()

  defp build_query_plan(params, stats) do
    total_events = Map.get(stats, "total_events", 0)
    filters = classify_query_filters(params)
    limit = Map.get(params, "limit", 100)

    {scan_type, est_scan} = estimate_scan(filters, total_events)
    steps = build_plan_steps(scan_type, est_scan, filters, limit)

    %{
      "query" => params,
      "execution_plan" => steps,
      "estimated_cost" => estimate_cost(filters),
      "total_store_events" => total_events,
      "optimization_suggestions" => suggest_optimizations(filters, total_events)
    }
  end

  defp classify_query_filters(params) do
    %{
      has_entity: Map.has_key?(params, "entity_id") && params["entity_id"] != nil,
      has_type: Map.has_key?(params, "event_type") && params["event_type"] != nil,
      has_time: Map.has_key?(params, "since") || Map.has_key?(params, "until")
    }
  end

  defp estimate_scan(%{has_entity: true, has_type: true}, total),
    do: {"Index scan (entity_id + event_type)", total * 0.001}

  defp estimate_scan(%{has_entity: true}, total),
    do: {"Index scan (entity_id)", total * 0.01}

  defp estimate_scan(%{has_type: true}, total),
    do: {"Index scan (event_type)", total * 0.05}

  defp estimate_scan(_filters, total),
    do: {"Full scan", total * 1.0}

  defp build_plan_steps(scan_type, est_scan, filters, limit) do
    steps = [%{"step" => 1, "operation" => scan_type, "estimated_rows" => round(est_scan)}]

    {steps, final_rows} =
      if filters.has_time do
        filtered = round(est_scan * 0.3)
        step = %{"step" => 2, "operation" => "Time range filter", "estimated_rows" => filtered}
        {steps ++ [step], filtered}
      else
        {steps, round(est_scan)}
      end

    steps ++
      [
        %{
          "step" => length(steps) + 1,
          "operation" => "Limit #{limit}",
          "estimated_rows" => min(final_rows, limit)
        }
      ]
  end

  defp estimate_cost(%{has_entity: true, has_type: true}), do: "very low"
  defp estimate_cost(%{has_entity: true}), do: "low"
  defp estimate_cost(%{has_type: true}), do: "medium"
  defp estimate_cost(%{has_time: true}), do: "medium-high"
  defp estimate_cost(_), do: "high"

  defp suggest_optimizations(%{has_entity: false, has_type: false}, _total),
    do: ["Add entity_id or event_type filter to avoid full scan"]

  defp suggest_optimizations(%{has_time: false}, total) when total > 100_000,
    do: ["Add time range (since/until) to reduce scan scope"]

  defp suggest_optimizations(_filters, _total),
    do: ["Query is well-optimized"]

  defp compute_latency_stats(latencies) do
    n = length(latencies)

    if n == 0 do
      %{"error" => "No latency data"}
    else
      sorted = Enum.sort(latencies)
      sum = Enum.sum(sorted)

      %{
        "iterations" => n,
        "p50_us" => Enum.at(sorted, div(n, 2)),
        "p95_us" => Enum.at(sorted, round(n * 0.95) - 1),
        "p99_us" => Enum.at(sorted, round(n * 0.99) - 1),
        "mean_us" => round(sum / n),
        "min_us" => List.first(sorted),
        "max_us" => List.last(sorted),
        "p50_ms" => Float.round(Enum.at(sorted, div(n, 2)) / 1000, 2),
        "p95_ms" => Float.round(Enum.at(sorted, round(n * 0.95) - 1) / 1000, 2),
        "p99_ms" => Float.round(Enum.at(sorted, round(n * 0.99) - 1) / 1000, 2),
        "throughput_qps" => if(sum > 0, do: Float.round(n / (sum / 1_000_000), 1), else: 0)
      }
    end
  end
end
