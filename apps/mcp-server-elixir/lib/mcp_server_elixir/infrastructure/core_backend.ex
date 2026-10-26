defmodule McpServerElixir.Infrastructure.CoreBackend do
  @moduledoc """
  Behaviour defining the contract for all Core operations.

  Both `CoreClient` (HTTP/remote) and `CoreEmbedded` (Rustler NIF/local) implement
  this behaviour. Tool handlers dispatch through `state.backend.function()` so they
  work with any backend.
  """

  # ============================================================================
  # Query & State
  # ============================================================================

  @doc "Query events with flexible filters"
  @callback query_events(params :: map()) :: {:ok, map()} | {:error, term()}

  @doc "Reconstruct entity state at a point in time"
  @callback reconstruct_state(entity_id :: String.t(), as_of :: String.t() | nil) ::
              {:ok, map()} | {:error, term()}

  @doc "Get current snapshot of an entity"
  @callback get_snapshot(entity_id :: String.t()) :: {:ok, map()} | {:error, term()}

  @doc "Ingest a new event"
  @callback ingest_event(event_data :: map()) :: {:ok, map()} | {:error, term()}

  @doc "Get event store statistics"
  @callback get_stats() :: {:ok, map()} | {:error, term()}

  # ============================================================================
  # Views (latest-state read models)
  # ============================================================================

  @doc "Latest state of one view row"
  @callback get_latest_state(view :: String.t(), key :: String.t()) ::
              {:ok, map()} | {:error, term()}

  @doc "One page of a view's rows (`prefix`, `after`, `limit`)"
  @callback list_latest(view :: String.t(), params :: map()) :: {:ok, map()} | {:error, term()}

  @doc "View rows written after a revision (`after`, `limit`)"
  @callback list_changes(view :: String.t(), params :: map()) :: {:ok, map()} | {:error, term()}

  # Views are rows Core stores for the Query Service; the embedded NIF has no
  # route to them, so a backend may leave these out.
  @optional_callbacks get_latest_state: 2, list_latest: 2, list_changes: 2

  # ============================================================================
  # Search
  # ============================================================================

  @doc "Perform semantic (vector) search on events"
  @callback semantic_search(params :: map()) :: {:ok, map()} | {:error, term()}

  @doc "Perform hybrid search combining semantic and keyword search"
  @callback hybrid_search(params :: map()) :: {:ok, map()} | {:error, term()}

  # ============================================================================
  # Operational
  # ============================================================================

  @doc "Trigger manual storage compaction"
  @callback compact_storage(params :: map()) :: {:ok, map()} | {:error, term()}

  @doc "Get storage statistics"
  @callback storage_stats(params :: map()) :: {:ok, map()} | {:error, term()}

  @doc "Get partition health and distribution info"
  @callback partition_info(params :: map()) :: {:ok, map()} | {:error, term()}

  @doc "Get WAL/replication status"
  @callback wal_status() :: {:ok, map()} | {:error, term()}

  @doc "Create a backup snapshot"
  @callback backup_create(params :: map()) :: {:ok, map()} | {:error, term()}

  @doc "Restore from a backup snapshot"
  @callback backup_restore(params :: map()) :: {:ok, map()} | {:error, term()}

  @doc "List available backup snapshots"
  @callback backup_list(params :: map()) :: {:ok, map()} | {:error, term()}

  @doc "Deep health check"
  @callback health_deep() :: {:ok, map()} | {:error, term()}

  @doc "Get performance metrics report"
  @callback performance_report(params :: map()) :: {:ok, map()} | {:error, term()}

  @doc "Query the audit trail"
  @callback audit_log(params :: map()) :: {:ok, map()} | {:error, term()}

  @doc "Get cluster status"
  @callback get_cluster_status() :: {:ok, map()} | {:error, term()}

  @doc "Get durability status — compares in-memory, WAL, and Parquet layers"
  @callback durability_status() :: {:ok, map()} | {:error, term()}

  # ============================================================================
  # Schema
  # ============================================================================

  @doc "Register a new event type schema"
  @callback register_schema(params :: map()) :: {:ok, map()} | {:error, term()}

  @doc "Validate an event payload against a registered schema"
  @callback validate_schema(params :: map()) :: {:ok, map()} | {:error, term()}

  @doc "List all registered schema subjects"
  @callback list_schemas(params :: map()) :: {:ok, map()} | {:error, term()}

  @doc "Get a specific schema by subject"
  @callback get_schema(subject :: String.t(), params :: map()) ::
              {:ok, map()} | {:error, term()}

  @doc "List schema versions for a subject"
  @callback list_schema_versions(subject :: String.t()) :: {:ok, map()} | {:error, term()}

  @doc "Set compatibility mode for a schema subject"
  @callback set_compatibility(subject :: String.t(), params :: map()) ::
              {:ok, map()} | {:error, term()}

  # ============================================================================
  # Analytics
  # ============================================================================

  @doc "Get event frequency analytics"
  @callback analytics_frequency(params :: map()) :: {:ok, map()} | {:error, term()}

  @doc "Get analytics summary statistics"
  @callback analytics_summary(params :: map()) :: {:ok, map()} | {:error, term()}

  @doc "Get correlation analysis between event types"
  @callback analytics_correlation(params :: map()) :: {:ok, map()} | {:error, term()}
end
