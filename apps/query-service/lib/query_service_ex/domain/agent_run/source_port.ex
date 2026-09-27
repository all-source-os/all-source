defmodule QueryServiceEx.Domain.AgentRun.SourcePort do
  @moduledoc "Bounded complete run reads; caller supplies an already-authorized tenant."
  @callback events(String.t(), String.t()) :: {:ok, list()} | {:error, atom()}
end
