defmodule QueryServiceEx.Application.Services.AgentRunRecorder do
  @moduledoc """
  Internal typed capture, without an executor. Callers must establish current
  tenant write authority and metering before invocation. No public/MCP binding.
  Every request makes at most one append; a lost response requires read recovery.
  """
  alias QueryServiceEx.Domain.AgentRun.AppendCommand

  def record(tenant, run_id, input) do
    with {:ok, command} <- AppendCommand.new(tenant, run_id, input),
         {:ok, events} <- source().events(tenant, run_id) do
      case AppendCommand.prepare(command, events) do
        {:existing, receipt} -> {:ok, receipt}
        {:append, request} -> append(command, request)
        error -> error
      end
    end
  end

  defp append(command, request) do
    writer = Application.fetch_env!(:query_service_ex, :agent_run_writer)

    case writer.append(request) do
      {:ok, acknowledgement} -> confirm(command, acknowledgement)
      {:error, :version_conflict} -> recover_conflict(command)
      _ -> {:error, :append_uncertain}
    end
  end

  defp confirm(command, acknowledgement) do
    with {:ok, events} <- source().events(command.tenant, command.run_id),
         {:existing, receipt} <- AppendCommand.prepare(command, events),
         true <- receipt.event_id == acknowledgement["event_id"],
         true <- receipt.version === acknowledgement["version"],
         true <- receipt.timestamp == acknowledgement["timestamp"] do
      {:ok, %{receipt | disposition: "recorded"}}
    else
      _ -> {:error, :append_uncertain}
    end
  end

  defp recover_conflict(command) do
    case source().events(command.tenant, command.run_id) do
      {:ok, events} ->
        case AppendCommand.prepare(command, events) do
          {:existing, receipt} -> {:ok, receipt}
          {:error, :operation_conflict} = error -> error
          {:error, :stale_revision} = error -> error
          _ -> {:error, :append_uncertain}
        end

      _ ->
        {:error, :append_uncertain}
    end
  end

  defp source, do: Application.fetch_env!(:query_service_ex, :agent_run_source)
end
