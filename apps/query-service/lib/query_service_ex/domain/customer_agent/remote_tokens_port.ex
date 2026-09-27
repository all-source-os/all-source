defmodule QueryServiceEx.Domain.CustomerAgent.RemoteTokensPort do
  @moduledoc "Request and bearer envelopes remain distinct from human sessions and OAuth codes."
  @callback seal_request(map(), String.t(), integer()) :: {:ok, String.t()} | {:error, atom()}
  @callback open_request(term(), String.t(), integer()) :: {:ok, map()} | {:error, atom()}
  @callback seal_access(map(), integer()) :: {:ok, String.t()} | {:error, atom()}
  @callback open_access(term(), String.t(), integer()) :: {:ok, map()} | {:error, atom()}
end
