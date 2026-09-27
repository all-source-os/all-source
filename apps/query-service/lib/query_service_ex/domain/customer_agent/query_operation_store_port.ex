defmodule QueryServiceEx.Domain.CustomerAgent.QueryOperationStorePort do
  @moduledoc "Conditional storage for original metering requests in Core; no process-local fallback or second counter."
  @callback load(String.t()) :: {:ok, map(), String.t() | nil} | {:error, atom()}
  @callback replace(String.t(), map(), String.t() | nil) :: :ok | {:error, atom()}
end
