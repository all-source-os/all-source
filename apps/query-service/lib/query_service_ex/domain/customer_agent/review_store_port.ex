defmodule QueryServiceEx.Domain.CustomerAgent.ReviewStorePort do
  @moduledoc "Conditional durable review metadata and independent deny markers. No actor authority."
  @callback load(String.t()) :: {:ok, map(), String.t() | nil} | {:error, atom()}
  @callback replace(String.t(), map(), String.t() | nil) :: :ok | {:error, atom()}
  @callback revoke(String.t(), String.t(), String.t()) :: :ok | {:error, atom()}
  @callback active?(String.t(), String.t(), String.t()) :: :ok | {:error, atom()}
end
