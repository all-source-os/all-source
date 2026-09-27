defmodule QueryServiceEx.Domain.AgentRun.AppendPort do
  @moduledoc "One conditional append; transport uncertainty never becomes success or a retry."
  @callback append(map()) :: {:ok, map()} | {:error, :version_conflict | :append_uncertain}
end
