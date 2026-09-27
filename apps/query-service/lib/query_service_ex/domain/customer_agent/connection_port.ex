defmodule QueryServiceEx.Domain.CustomerAgent.ConnectionPort do
  @moduledoc "Durable consent and credential operations; no human authentication fallback."
  @callback issue(map(), list(), map(), integer(), integer()) :: {:ok, map()} | {:error, atom()}
  @callback list(String.t(), String.t(), integer()) :: {:ok, list()} | {:error, atom()}
  @callback fetch(String.t(), String.t()) :: {:ok, map()} | {:error, atom()}
  @callback revoke(map(), String.t(), integer()) :: :ok | {:error, atom()}
  @callback activate_remote(String.t(), map(), integer()) :: :ok | {:error, atom()}
end
