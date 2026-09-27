defmodule QueryServiceEx.Domain.CustomerAgent.AuthorizationCodePort do
  @moduledoc "Purpose-separated, short-lived encryption for PKCE authorization codes."
  @callback available?() :: boolean()
  @callback seal(map(), integer()) :: {:ok, String.t()} | {:error, atom()}
  @callback open(term()) :: {:ok, map()} | {:error, atom()}
end
