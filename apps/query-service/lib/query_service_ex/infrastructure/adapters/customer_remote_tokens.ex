defmodule QueryServiceEx.Infrastructure.Adapters.CustomerRemoteTokens do
  @moduledoc "Purpose-separated request cookies and opaque remote access envelopes."
  @behaviour QueryServiceEx.Domain.CustomerAgent.RemoteTokensPort
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionGrant
  alias QueryServiceEx.Domain.CustomerAgent.RemoteAuthorization

  @impl true
  def seal_request(request, resource, now) do
    with {:ok, _} <- RemoteAuthorization.validate_request(request, resource) do
      seal("request", %{"request" => request, "created_at" => now}, now, 600)
    end
  end

  @impl true
  def open_request(token, resource, now) do
    with {:ok, %{"request" => request, "created_at" => created} = payload} <-
           open("request", token, 600),
         true <- map_size(payload) == 2 and is_integer(created),
         true <- now >= created and now < created + 600,
         {:ok, _} <- RemoteAuthorization.validate_request(request, resource) do
      {:ok, request}
    else
      _ -> {:error, :invalid_request}
    end
  end

  @impl true
  def seal_access(%{token: token, binding: binding, expires_at: expires}, now) do
    payload = %{
      "token" => token,
      "binding" => binding,
      "created_at" => now,
      "expires_at" => expires
    }

    if valid_access?(payload, binding["resource"], now),
      do: seal("access", payload, now, expires - now),
      else: {:error, :storage_unavailable}
  end

  @impl true
  def open_access(token, resource, now) do
    with {:ok, payload} <- open("access", token, 3_600),
         true <- valid_access?(payload, resource, now) do
      {:ok, payload}
    else
      _ -> {:error, :access_denied}
    end
  end

  defp valid_access?(payload, resource, now) when is_map(payload) do
    binding = payload["binding"]

    Enum.sort(Map.keys(payload)) == ~w(binding created_at expires_at token) and
      ConnectionGrant.valid_binding?(binding) and binding["client_id"] == "claude-ai" and
      binding["resource"] == resource and valid_interval?(payload, now) and
      is_binary(payload["token"]) and
      Regex.match?(~r/\Aasreview_v2_[0-9a-f]{32}\.[0-9a-f]{64}\z/, payload["token"])
  end

  defp valid_access?(_, _, _), do: false

  defp valid_interval?(%{"created_at" => created, "expires_at" => expires}, now),
    do:
      is_integer(created) and is_integer(expires) and is_integer(now) and
        now >= created and now < expires and (expires - created) in 1..3_600

  defp seal(purpose, payload, now, ttl) do
    with {:ok, key} <- key(),
         token = Plug.Crypto.encrypt(key, salt(purpose), payload, signed_at: now, max_age: ttl),
         true <- byte_size(token) <= 3_800 do
      {:ok, token}
    else
      _ -> {:error, :storage_unavailable}
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  defp open(purpose, token, ttl) when is_binary(token) and byte_size(token) in 1..3_800 do
    with {:ok, key} <- key(),
         {:ok, payload} <- Plug.Crypto.decrypt(key, salt(purpose), token, max_age: ttl) do
      {:ok, payload}
    else
      _ -> {:error, :invalid_token}
    end
  rescue
    _ -> {:error, :invalid_token}
  end

  defp open(_, _, _), do: {:error, :invalid_token}
  defp salt(purpose), do: "allsource-customer-remote-" <> purpose <> "-v1"

  defp key do
    case System.get_env("JWT_SECRET") do
      key when is_binary(key) and byte_size(key) >= 32 -> {:ok, key}
      _ -> {:error, :storage_unavailable}
    end
  end
end
