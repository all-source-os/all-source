defmodule QueryServiceExWeb.CustomerHumanSession do
  @moduledoc "Verified product session only; never generic API-key, demo or impersonation authority."
  import Plug.Conn
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionGrant

  def actor(conn) do
    with ["Bearer " <> token] <- get_req_header(conn, "authorization"),
         true <- byte_size(token) in 1..8_192,
         secret <- System.get_env("JWT_SECRET"),
         true <- is_binary(secret) and byte_size(secret) >= 32,
         {true, %JOSE.JWT{fields: claims}, _} <-
           JOSE.JWT.verify_strict(JOSE.JWK.from_oct(secret), ["HS256"], token),
         true <- valid_session?(claims) do
      {:ok, %{"tenant_id" => claims["tenant_id"], "subject_id" => claims["sub"]}}
    else
      _ -> {:error, :access_denied}
    end
  rescue
    _ -> {:error, :access_denied}
  end

  defp valid_session?(claims) do
    ConnectionGrant.valid_id?(claims["tenant_id"]) and
      ConnectionGrant.valid_subject?(claims["sub"]) and
      claims["provider"] in ~w(google github email) and claims["email_verified"] == true and
      Enum.all?(~w(is_api_key is_demo view_as), &(claims[&1] in [nil, false])) and
      Enum.all?(~w(api_key core_api_key act_as), &(claims[&1] in [nil, ""])) and
      valid_lifetime?(claims, System.system_time(:second))
  end

  defp valid_lifetime?(claims, now) do
    is_integer(claims["exp"]) and claims["exp"] > now and
      is_integer(claims["iat"]) and claims["iat"] >= 0 and claims["iat"] <= now and
      (is_nil(claims["nbf"]) or (is_integer(claims["nbf"]) and claims["nbf"] <= now))
  end
end
