defmodule QueryServiceEx.Domain.CustomerAgent.ReviewState do
  @moduledoc """
  Effective review status with explicit time and evidence-version inputs.

  This pure type is separate from existing replay execution status. It creates
  only pending records and has no approve or execute operation. The application
  must authenticate any persisted human decision before constructing this value;
  status/3 is not an authenticator and must never authorise a rebuild by itself.
  """

  alias QueryServiceEx.Domain.CustomerAgent.ConnectionGrant

  @enforce_keys [:owner_tenant, :owner_subject, :digest, :created_at, :expires_at]
  defstruct [
    :owner_tenant,
    :owner_subject,
    :digest,
    :created_at,
    :expires_at,
    authority_version: "allsource-operator-review-v1",
    decision: :pending
  ]

  @type decision :: :pending | :rejected | :approved
  @type status :: decision() | :expired | :superseded | :unavailable
  @type t :: %__MODULE__{
          owner_tenant: String.t(),
          owner_subject: String.t(),
          digest: String.t(),
          created_at: non_neg_integer(),
          expires_at: pos_integer(),
          authority_version: String.t(),
          decision: decision()
        }

  @doc "Create pending state using server-authenticated owner IDs and Unix seconds."
  @spec pending(term(), term(), term(), term(), term()) :: {:ok, t()} | {:error, :invalid_review}
  def pending(tenant, subject, digest, now, ttl) do
    if ConnectionGrant.valid_id?(tenant) and ConnectionGrant.valid_subject?(subject) and
         valid_digest?(digest) and
         is_integer(now) and now >= 0 and is_integer(ttl) and ttl in 1..86_400 do
      {:ok,
       %__MODULE__{
         owner_tenant: tenant,
         owner_subject: subject,
         digest: digest,
         created_at: now,
         expires_at: now + ttl
       }}
    else
      {:error, :invalid_review}
    end
  end

  @doc "Interpret an authenticated stored decision; unavailable data fails closed."
  @spec status(term(), term(), term()) :: status()
  def status(%__MODULE__{authority_version: "allsource-operator-review-v1"} = state, digest, now) do
    cond do
      not valid_record?(state, now) -> :unavailable
      state.decision == :rejected -> :rejected
      now >= state.expires_at -> :expired
      not valid_digest?(digest) -> :unavailable
      state.digest != digest -> :superseded
      true -> state.decision
    end
  end

  def status(_, _, _), do: :unavailable

  defp valid_record?(state, now) do
    state.decision in [:pending, :rejected, :approved] and is_integer(now) and
      ConnectionGrant.valid_id?(state.owner_tenant) and
      ConnectionGrant.valid_subject?(state.owner_subject) and
      valid_digest?(state.digest) and valid_interval?(state, now)
  end

  defp valid_interval?(state, now) do
    is_integer(state.created_at) and state.created_at >= 0 and is_integer(state.expires_at) and
      (state.expires_at - state.created_at) in 1..86_400 and now >= state.created_at
  end

  defp valid_digest?(value) when is_binary(value) and byte_size(value) == 64,
    do: Regex.match?(~r/\A[0-9a-f]{64}\z/, value)

  defp valid_digest?(_), do: false
end
