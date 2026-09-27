defmodule QueryServiceEx.Application.Services.CustomerHumanEvidence do
  @moduledoc "Product-owned source selection and minimal saved work. Never authenticates an agent or approves an action."
  alias QueryServiceEx.Application.Services.AgentRunEvidence
  alias QueryServiceEx.Application.Services.CustomerConnections, as: Connections
  alias QueryServiceEx.Application.Services.CustomerQueryAdmission, as: Admission
  alias QueryServiceEx.Application.Services.CustomerReviewDeadline
  alias QueryServiceEx.Application.Services.CustomerReviewRecords, as: Records
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionConsent
  alias QueryServiceEx.Domain.CustomerAgent.EvidenceSource
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOperation, as: Operation
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner, as: Owner

  def inspect_run(actor, connection, input, now) do
    CustomerReviewDeadline.run(actor, fn ->
      with true <- is_map(input) and Enum.sort(Map.keys(input)) == ~w(request_id run_id),
           true <- Event.uuid?(input["run_id"]) and Operation.valid_at?(input["request_id"], now),
           {:ok, owner} <- Connections.evidence_owner(actor, connection, now),
           :ok <-
             Admission.admit(
               owner,
               "source.inspect",
               input["request_id"],
               input["run_id"],
               1,
               now
             ),
           {:ok, run} <- AgentRunEvidence.read(owner["tenant_id"], input["run_id"]),
           {:ok, _} <- Connections.evidence_owner(actor, connection, System.system_time(:second)) do
        {:ok,
         %{
           run_id: run.run_id,
           revision: run.revision,
           sha256: run.digest,
           changes: length(run.changes),
           attempts: length(run.attempts),
           completed: run.completed,
           unknowns: run.unknowns,
           shared: false
         }}
      else
        false -> {:error, :invalid_source_request}
        error -> error
      end
    end)
  end

  def workspace(actor, connection, now) do
    CustomerReviewDeadline.run(actor, fn ->
      with {:ok, owner, status} <- owned(actor, connection, now),
           {:ok, records, _} <- Records.snapshot(owner["tenant_id"]),
           {:ok, sources} <- summaries(records["sources"], owner, "sources", status, now),
           {:ok, reviews} <- summaries(records["reviews"], owner, "reviews", status, now),
           {:ok, _, ^status} <- owned(actor, connection, System.system_time(:second)) do
        {:ok, %{sources: sources, reviews: reviews, connection_status: status}}
      else
        {:error, _} = error -> error
        _ -> {:error, :access_denied}
      end
    end)
  end

  def revoke_source(actor, connection, id, now) do
    CustomerReviewDeadline.run(actor, fn ->
      with true <- Owner.id?(id),
           {:ok, owner, _} <- owned(actor, connection, now),
           {:ok, records, _} <- Records.snapshot(owner["tenant_id"]),
           source when is_map(source) <- records["sources"][id],
           true <- Owner.matches?(source, owner),
           :ok <- Records.revoke(owner["tenant_id"], "sources", id),
           {:ok, _, _} <- owned(actor, connection, System.system_time(:second)) do
        {:ok, %{revoked: true}}
      else
        {:error, _} = error -> error
        _ -> {:error, :access_denied}
      end
    end)
  end

  defp owned(actor, connection, now) do
    with true <- Owner.id?(connection),
         {:ok, %{connections: receipts}} <- Connections.list(actor, now),
         [receipt] <- Enum.filter(receipts, &(&1["id"] == connection)),
         true <- receipt["consent"]["version"] == ConnectionConsent.evidence_version(),
         binding = Connections.binding(actor, receipt["client_id"]),
         true <- receipt["resource"] == binding["resource"],
         owner = Map.put(binding, "grant_id", connection),
         true <- Owner.valid?(owner) do
      {:ok, owner, receipt["status"]}
    else
      {:error, _} = error -> error
      _ -> {:error, :access_denied}
    end
  end

  defp summaries(records, owner, kind, connection_status, now) do
    records
    |> Map.values()
    |> Enum.filter(&Owner.matches?(&1, owner))
    |> Enum.sort_by(&{&1["created_at"], &1["id"]}, :desc)
    |> Enum.reduce_while({:ok, []}, fn record, {:ok, summaries} ->
      case Records.active?(owner["tenant_id"], kind, record["id"]) do
        :ok ->
          {:cont,
           {:ok, [summary(record, kind, state(record, connection_status, now)) | summaries]}}

        {:error, :revoked} ->
          {:cont, {:ok, [summary(record, kind, "revoked") | summaries]}}

        _ ->
          {:halt, {:error, :storage_unavailable}}
      end
    end)
    |> case do
      {:ok, summaries} -> {:ok, Enum.reverse(summaries)}
      error -> error
    end
  end

  defp state(record, connection_status, now) do
    cond do
      record["expires_at"] <= now -> "expired"
      connection_status != "active" -> "unavailable"
      true -> "saved"
    end
  end

  defp summary(record, "sources", status),
    do: %{
      source: EvidenceSource.reference(record),
      run_id: record["locator"],
      expires_at: record["expires_at"],
      status: status
    }

  defp summary(record, "reviews", status),
    do: %{
      id: record["id"],
      version: record["version"],
      digest: record["digest"],
      expires_at: record["expires_at"],
      status: status,
      approved: false,
      execution: "none"
    }
end
