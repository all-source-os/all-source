if System.get_env("CORE_MODE") == "embedded" do
  defmodule McpServerElixir.Infrastructure.CoreEmbedded do
    @moduledoc """
    CoreBackend implementation backed by the embedded Rust Core via Rustler NIF.

    Delegates all operations to `CoreNif.nif_*` functions, passing a resource
    reference held in `CoreEmbedded.Supervisor`.
    """

    @behaviour McpServerElixir.Infrastructure.CoreBackend

    alias McpServerElixir.Infrastructure.CoreNif

    defp resource do
      McpServerElixir.Infrastructure.CoreEmbedded.Supervisor.resource()
    end

    @impl true
    def unsupported_tools do
      ["get_cluster_status", "partition_info", "performance_report", "audit_log"]
    end

    # ============================================================================
    # Query & State
    # ============================================================================

    @impl true
    def query_events(params) do
      CoreNif.nif_query(resource(), params)
    end

    @impl true
    def reconstruct_state(entity_id, as_of) do
      CoreNif.nif_reconstruct_state(resource(), entity_id, %{"as_of" => as_of})
    end

    @impl true
    def get_snapshot(entity_id) do
      case CoreNif.nif_get_snapshot(resource(), entity_id) do
        {:ok, json_str} when is_binary(json_str) ->
          case Jason.decode(json_str) do
            {:ok, map} -> {:ok, map}
            {:error, _} -> {:ok, %{"raw" => json_str}}
          end

        {:error, :not_found} ->
          {:error, :not_found}

        other ->
          other
      end
    end

    @impl true
    def ingest_event(event_data) do
      CoreNif.nif_ingest(resource(), event_data)
    end

    @impl true
    def get_stats do
      CoreNif.nif_get_stats(resource())
    end

    # ============================================================================
    # Search
    # ============================================================================

    @impl true
    def semantic_search(params) do
      CoreNif.nif_semantic_search(resource(), params)
    end

    @impl true
    def hybrid_search(params) do
      CoreNif.nif_hybrid_search(resource(), params)
    end

    # ============================================================================
    # Operational
    # ============================================================================

    @impl true
    def compact_storage(_params) do
      CoreNif.nif_compact_storage(resource())
    end

    @impl true
    def storage_stats(_params) do
      CoreNif.nif_storage_stats(resource())
    end

    @impl true
    def partition_info(_params) do
      {:error, "not supported in embedded mode"}
    end

    @impl true
    def wal_status do
      CoreNif.nif_wal_status(resource())
    end

    @impl true
    def health_deep do
      CoreNif.nif_health_deep(resource())
    end

    @impl true
    def performance_report(_params) do
      {:error, "not supported in embedded mode"}
    end

    @impl true
    def audit_log(_params) do
      {:error, "not supported in embedded mode"}
    end

    @impl true
    def get_cluster_status do
      {:error, "not supported in embedded mode"}
    end

    @impl true
    def durability_status do
      CoreNif.nif_durability_status(resource())
    end

    # ============================================================================
    # Schema
    # ============================================================================

    @impl true
    def register_schema(params) do
      CoreNif.nif_register_schema(resource(), params)
    end

    @impl true
    def validate_schema(params) do
      CoreNif.nif_validate_schema(resource(), params)
    end

    @impl true
    def list_schemas(_params) do
      CoreNif.nif_list_schemas(resource())
    end

    @impl true
    def get_schema(subject, _params) do
      CoreNif.nif_get_schema(resource(), subject)
    end

    @impl true
    def list_schema_versions(_subject) do
      {:error, "not supported in embedded mode"}
    end

    @impl true
    def set_compatibility(_subject, _params) do
      {:error, "not supported in embedded mode"}
    end

    # ============================================================================
    # Analytics
    # ============================================================================

    @impl true
    def analytics_frequency(params) do
      CoreNif.nif_analytics_frequency(resource(), params)
    end

    @impl true
    def analytics_summary(params) do
      CoreNif.nif_analytics_summary(resource(), params)
    end

    @impl true
    def analytics_correlation(params) do
      CoreNif.nif_analytics_correlation(resource(), params)
    end
  end

  defmodule McpServerElixir.Infrastructure.CoreEmbedded.Supervisor do
    @moduledoc """
    GenServer that manages the lifecycle of the embedded Core NIF resource.

    Calls `CoreNif.nif_open(config)` on init and `CoreNif.nif_shutdown(ref)` on terminate.
    The NIF resource reference is stored in the GenServer state and accessed via `resource/0`.
    """

    use GenServer

    alias McpServerElixir.Infrastructure.CoreNif

    def start_link(opts \\ []) do
      GenServer.start_link(__MODULE__, opts, name: __MODULE__)
    end

    def resource do
      GenServer.call(__MODULE__, :get_resource)
    end

    @impl true
    def init(_opts) do
      config = Application.get_env(:mcp_server_elixir, :embedded_config, %{})

      nif_config = %{
        "data_dir" => Map.get(config, :data_dir, ""),
        "node_id" => Map.get(config, :node_id, 1),
        "wal_fsync_interval_ms" => Map.get(config, :wal_fsync_interval_ms, 100),
        "parquet_flush_interval_secs" => Map.get(config, :parquet_flush_interval_secs, 300)
      }

      case CoreNif.nif_open(nif_config) do
        {:ok, resource} ->
          {:ok, %{resource: resource}}

        {:error, reason} ->
          {:stop, {:nif_open_failed, reason}}
      end
    end

    @impl true
    def handle_call(:get_resource, _from, %{resource: resource} = state) do
      {:reply, resource, state}
    end

    @impl true
    def terminate(_reason, %{resource: resource}) do
      CoreNif.nif_shutdown(resource)
      :ok
    end

    def terminate(_reason, _state), do: :ok
  end
end

# if CORE_MODE == "embedded"
