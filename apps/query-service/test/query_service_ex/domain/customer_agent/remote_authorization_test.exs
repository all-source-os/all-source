defmodule QueryServiceEx.Domain.CustomerAgent.RemoteAuthorizationTest do
  use ExUnit.Case, async: true
  alias QueryServiceEx.Domain.CustomerAgent.RemoteAuthorization

  @resource "https://api.example.test/mcp"
  # RFC 7636 Appendix B, independent known SHA-256 vector.
  @verifier "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
  @request %{
    "client_id" => "claude-ai",
    "redirect_uri" => "https://claude.ai/api/mcp/auth_callback",
    "response_type" => "code",
    "resource" => @resource,
    "scope" => "allsource.review",
    "state" => "synthetic-client-state",
    "code_challenge_method" => "S256",
    "code_challenge" => "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
  }

  test "known S256 vector succeeds only for the exact client, resource and redirect" do
    assert {:ok, @request} = RemoteAuthorization.validate_request(@request, @resource)
    assert :ok = RemoteAuthorization.validate_exchange(exchange(), payload(), @resource, 1_000)

    for {key, value} <- [
          {"client_id", "claude-code"},
          {"redirect_uri", "https://claude.ai/api/mcp/auth_callback/"},
          {"resource", @resource <> "/"},
          {"grant_type", "refresh_token"},
          {"code_verifier", String.duplicate("x", 43)},
          {"code_verifier", @verifier <> "="},
          {"code_verifier", "short"},
          {"code_verifier", String.duplicate("x", 129)},
          {"scope", "admin"}
        ] do
      assert {:error, :invalid_grant} =
               RemoteAuthorization.validate_exchange(
                 Map.put(exchange(), key, value),
                 payload(),
                 @resource,
                 1_000
               )
    end
  end

  test "redirect matching rejects suffixes, case changes, encoded paths, queries and fragments" do
    for redirect <- [
          "https://claude.ai.evil.test/api/mcp/auth_callback",
          "https://claude.ai@evil.test/api/mcp/auth_callback",
          "https://CLAUDE.ai/api/mcp/auth_callback",
          "https://claude.ai:443/api/mcp/auth_callback",
          "https://claude.ai/api/mcp/%61uth_callback",
          "https://claude.ai/api/mcp/auth_callback?next=evil",
          "https://claude.ai/api/mcp/auth_callback#fragment",
          "http://127.0.0.1:3118/callback"
        ] do
      assert {:error, :invalid_request} =
               RemoteAuthorization.validate_request(
                 Map.put(@request, "redirect_uri", redirect),
                 @resource
               )
    end
  end

  test "plain PKCE, noncanonical challenges, extra scopes and malformed state fail closed" do
    for {key, value} <- [
          {"code_challenge_method", "plain"},
          {"code_challenge", @request["code_challenge"] <> "="},
          {"code_challenge", String.duplicate("!", 43)},
          {"code_challenge", String.duplicate("x", 43)},
          {"scope", "allsource.review offline_access"},
          {"response_type", "token"},
          {"state", ""},
          {"state", "abc\n"},
          {"state", String.duplicate("x", 513)},
          {"tenant_id", "forged"}
        ] do
      assert {:error, :invalid_request} =
               RemoteAuthorization.validate_request(Map.put(@request, key, value), @resource)
    end

    assert {:error, :invalid_request} = RemoteAuthorization.validate_request(nil, @resource)

    assert {:error, :invalid_grant} =
             RemoteAuthorization.validate_exchange(nil, payload(), @resource, 1_000)
  end

  test "exact expiration and clock rollback deny exchange" do
    assert :ok = RemoteAuthorization.validate_exchange(exchange(), payload(), @resource, 1_299)

    for now <- [999, 1_300, 2_000] do
      assert {:error, :invalid_grant} =
               RemoteAuthorization.validate_exchange(exchange(), payload(), @resource, now)
    end
  end

  defp payload, do: %{"request" => @request, "created_at" => 1_000}

  defp exchange do
    %{
      "client_id" => "claude-ai",
      "redirect_uri" => @request["redirect_uri"],
      "resource" => @resource,
      "grant_type" => "authorization_code",
      "code_verifier" => @verifier,
      "code" => "synthetic-code"
    }
  end
end
