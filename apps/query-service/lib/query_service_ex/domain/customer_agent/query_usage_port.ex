defmodule QueryServiceEx.Domain.CustomerAgent.QueryUsagePort do
  @moduledoc """
  Authoritative query accounting through Core. Callers must durably retain the
  exact operation ID, fingerprint, count and period before admission, including
  after an uncertain response. Never mint a replacement retry or fall back to
  buffered usage. This port neither verifies access nor resets billing periods.
  """
  @callback snapshot(String.t()) :: {:ok, map()} | {:error, atom()}
  @callback admit(String.t(), map()) :: {:ok, map()} | {:error, atom()}
end
