defmodule QueryServiceEx.Infrastructure.Adapters.CustomerAuthorizationCodeTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Infrastructure.Adapters.CustomerAuthorizationCode

  setup do
    previous = System.get_env("JWT_SECRET")
    System.put_env("JWT_SECRET", "synthetic-only-envelope-test-secret-2026")

    on_exit(fn ->
      if previous,
        do: System.put_env("JWT_SECRET", previous),
        else: System.delete_env("JWT_SECRET")
    end)

    %{
      payload: %{
        "version" => 1,
        "binding" => %{},
        "request" => %{},
        "created_at" => System.system_time(:second),
        "token" => String.duplicate("s", 109)
      }
    }
  end

  test "authenticated encryption rejects tampering, wrong-purpose tokens and secret rotation", %{
    payload: payload
  } do
    assert {:ok, code} = CustomerAuthorizationCode.seal(payload, payload["created_at"])
    assert {:ok, ^payload} = CustomerAuthorizationCode.open(code)
    refute code =~ payload["token"]
    assert {:error, :invalid_grant} = CustomerAuthorizationCode.open(code <> "x")

    wrong_purpose =
      Plug.Crypto.encrypt(System.fetch_env!("JWT_SECRET"), "another-purpose", payload)

    assert {:error, :invalid_grant} = CustomerAuthorizationCode.open(wrong_purpose)
    System.put_env("JWT_SECRET", "synthetic-only-rotated-envelope-secret-2026")
    assert {:error, :invalid_grant} = CustomerAuthorizationCode.open(code)
  end

  test "missing or weak secrets, expired codes, oversized codes and malformed payloads deny", %{
    payload: payload
  } do
    assert {:ok, expired} = CustomerAuthorizationCode.seal(payload, payload["created_at"] - 301)
    assert {:error, :invalid_grant} = CustomerAuthorizationCode.open(expired)

    for code <- [nil, %{}, "", String.duplicate("x", 4_097)] do
      assert {:error, :invalid_grant} = CustomerAuthorizationCode.open(code)
    end

    assert {:error, :storage_unavailable} =
             CustomerAuthorizationCode.seal(
               Map.put(payload, "extra", true),
               payload["created_at"]
             )

    System.put_env("JWT_SECRET", "too-short")
    refute CustomerAuthorizationCode.available?()

    assert {:error, :storage_unavailable} =
             CustomerAuthorizationCode.seal(payload, payload["created_at"])

    System.delete_env("JWT_SECRET")
    refute CustomerAuthorizationCode.available?()
    assert {:error, :invalid_grant} = CustomerAuthorizationCode.open(expired)
  end
end
