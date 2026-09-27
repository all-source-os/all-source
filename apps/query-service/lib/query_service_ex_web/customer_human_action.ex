defmodule QueryServiceExWeb.CustomerHumanAction do
  @moduledoc "Product relay attestation bound to one verified session, operation and canonical request body."
  import Plug.Conn
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner, as: Owner
  alias QueryServiceExWeb.CustomerHumanSession

  def actor(conn, operation, params) do
    now = System.system_time(:second)

    with {:ok, actor} <- CustomerHumanSession.actor(conn),
         ["Bearer " <> session] <- get_req_header(conn, "authorization"),
         [proof] when byte_size(proof) <= 2_048 <-
           get_req_header(conn, "x-allsource-product-action"),
         [encoded, signature] <- String.split(proof, "."),
         {:ok, actual} <- Base.url_decode64(signature, padding: false),
         secret when is_binary(secret) and byte_size(secret) >= 32 <-
           System.get_env("CUSTOMER_HUMAN_ACTION_SECRET"),
         expected = :crypto.mac(:hmac, :sha256, secret, encoded),
         true <-
           byte_size(actual) == byte_size(expected) and
             Plug.Crypto.secure_compare(actual, expected),
         {:ok, json} <- Base.url_decode64(encoded, padding: false),
         {:ok, claims} <- Jason.decode(json),
         true <- valid?(claims, operation, params, session, now) do
      {:ok, actor}
    else
      _ -> {:error, :access_denied}
    end
  rescue
    _ -> {:error, :access_denied}
  end

  defp valid?(claims, operation, params, session, now) when is_map(claims) do
    Enum.sort(Map.keys(claims)) == ~w(aud body_sha256 exp iat op session_sha256 v) and
      claims["v"] === 1 and
      claims["aud"] == "allsource-product-action" and claims["op"] == operation and
      claims["body_sha256"] == Owner.digest(params) and claims["session_sha256"] == hash(session) and
      valid_time?(claims, now)
  end

  defp valid?(_, _, _, _, _), do: false

  defp valid_time?(claims, now),
    do:
      is_integer(claims["iat"]) and is_integer(claims["exp"]) and
        claims["iat"] <= now and now < claims["exp"] and (claims["exp"] - claims["iat"]) in 1..30

  defp hash(value), do: :crypto.hash(:sha256, value) |> Base.encode16(case: :lower)
end
