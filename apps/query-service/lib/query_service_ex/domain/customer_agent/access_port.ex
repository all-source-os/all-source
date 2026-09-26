defmodule QueryServiceEx.Domain.CustomerAgent.AccessPort do
  @moduledoc """
  Current credential and eligibility reads for the customer review boundary.

  The application must not substitute cached sessions or user-supplied records
  for these authoritative reads. Implementations return fixed errors only.
  """

  @callback verify_credential(term(), term(), term(), term()) ::
              {:ok, map()} | {:error, atom()}
  @callback tenant(String.t()) :: {:ok, map()} | {:error, atom()}
  @callback members(String.t()) :: {:ok, list()} | {:error, atom()}
end
