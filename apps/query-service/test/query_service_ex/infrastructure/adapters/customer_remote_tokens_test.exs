defmodule QueryServiceEx.Infrastructure.Adapters.CustomerRemoteTokensTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Infrastructure.Adapters.CustomerRemoteTokens
  alias QueryServiceEx.TestSupport.CustomerRemoteHTTP, as: Remote

  setup do
    previous = System.get_env("JWT_SECRET")
    System.put_env("JWT_SECRET", "synthetic-only-remote-envelope-secret-2026")

    on_exit(fn ->
      if previous,
        do: System.put_env("JWT_SECRET", previous),
        else: System.delete_env("JWT_SECRET")
    end)

    now = System.system_time(:second)
    resource = Remote.request()["resource"]

    %{
      now: now,
      resource: resource,
      issued: %{
        token: "asreview_v2_" <> String.duplicate("a", 32) <> "." <> String.duplicate("b", 64),
        binding: %{
          "tenant_id" => "synthetic",
          "subject_id" => "oauth:google:synthetic",
          "client_id" => "claude-ai",
          "resource" => resource
        },
        expires_at: now + 3_600
      }
    }
  end

  test "requests and access tokens have different purposes, lifetime and resource binding", %{
    now: now,
    resource: resource,
    issued: issued
  } do
    assert {:ok, request} = CustomerRemoteTokens.seal_request(Remote.request(), resource, now)
    assert {:ok, access} = CustomerRemoteTokens.seal_access(issued, now)
    assert {:ok, _} = CustomerRemoteTokens.open_request(request, resource, now + 599)

    assert {:error, :invalid_request} =
             CustomerRemoteTokens.open_request(request, resource, now + 600)

    assert {:ok, _} = CustomerRemoteTokens.open_access(access, resource, now + 3_599)

    assert {:error, :access_denied} =
             CustomerRemoteTokens.open_access(access, resource, now + 3_600)

    for value <- [request, access <> "x", issued.token, nil, String.duplicate("a", 3_801)] do
      assert {:error, :access_denied} = CustomerRemoteTokens.open_access(value, resource, now)
    end

    assert {:error, :invalid_request} = CustomerRemoteTokens.open_request(access, resource, now)

    assert {:error, :access_denied} =
             CustomerRemoteTokens.open_access(access, resource <> "/", now)

    assert {:error, :access_denied} = CustomerRemoteTokens.open_access(access, resource, now - 1)
    refute access =~ issued.token
    refute access =~ issued.binding["subject_id"]
    System.put_env("JWT_SECRET", "synthetic-rotated-remote-envelope-secret")
    assert {:error, :access_denied} = CustomerRemoteTokens.open_access(access, resource, now)
  end

  test "malformed or overlong lifetimes and missing encryption keys fail closed", %{
    now: now,
    issued: issued
  } do
    for bad <- [
          Map.put(issued, :expires_at, now),
          Map.put(issued, :expires_at, now + 3_601),
          Map.put(issued, :token, "human-session"),
          put_in(issued, [:binding, "client_id"], "claude-code")
        ] do
      assert {:error, :storage_unavailable} = CustomerRemoteTokens.seal_access(bad, now)
    end

    System.delete_env("JWT_SECRET")
    assert {:error, :storage_unavailable} = CustomerRemoteTokens.seal_access(issued, now)
  end
end
