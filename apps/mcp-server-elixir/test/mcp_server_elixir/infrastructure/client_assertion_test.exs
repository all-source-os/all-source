defmodule McpServerElixir.Infrastructure.ClientAssertionTest do
  @moduledoc """
  The ES256 signer mints what the gateway verifies.

  The interop fixtures are shared with the Rust SDK
  (`sdks/rust/tests/es256_assertion.rs`) and the gateway
  (`apps/control-plane/asserted_keys_interop_test.go`), with the same client,
  tenant, audience and issue time.
  """
  use ExUnit.Case, async: true

  import ExUnit.CaptureLog

  alias McpServerElixir.Infrastructure.ClientAssertion

  @fixtures Path.expand("../../../../../sdks/rust/tests/fixtures", __DIR__)
  @key_path Path.join(@fixtures, "es256_interop_test_key.pem")
  @token_path Path.join(@fixtures, "es256_interop_token.jwt")
  @external_resource @key_path
  @external_resource @token_path

  @audience "https://api.all-source.xyz"
  @client "longhand-control-plane"
  @tenant "asserted-tenant"
  @iat 1_794_009_600
  @fixture_kid "q_OqpJGBOYfCf5q-_NvXB8sEV7Zo9CN_EUHRewTCSSQ"

  defp settings(overrides \\ []) do
    Keyword.merge(
      [
        signing_key: File.read!(@key_path),
        client_id: @client,
        tenant_id: @tenant,
        kid: @fixture_kid,
        audience: @audience
      ],
      overrides
    )
  end

  defp signer(overrides \\ []) do
    {:ok, signer} = ClientAssertion.new(settings(overrides))
    signer
  end

  defp segment(token, index) do
    token
    |> String.split(".")
    |> Enum.at(index)
    |> Base.url_decode64!(padding: false)
    |> Jason.decode!()
  end

  # JOSE carries ES256 as raw r||s; :crypto verifies a DER ECDSA-Sig-Value.
  defp verifies?(token, public_point) do
    [header, claims, signature] = String.split(token, ".")

    <<r::unsigned-big-integer-size(256), s::unsigned-big-integer-size(256)>> =
      Base.url_decode64!(signature, padding: false)

    der = :public_key.der_encode(:"ECDSA-Sig-Value", {:"ECDSA-Sig-Value", r, s})
    :crypto.verify(:ecdsa, :sha256, header <> "." <> claims, der, [public_point, :secp256r1])
  end

  defp without_jti(claims) do
    {jti, rest} = Map.pop(claims, "jti")
    assert is_binary(jti) and jti != ""
    rest
  end

  describe "interop fixtures" do
    test "the fixture key's RFC 7638 thumbprint is the kid the fixture token names" do
      assert ClientAssertion.thumbprint(signer().public_point) == @fixture_kid
      assert segment(File.read!(@token_path), 0)["kid"] == @fixture_kid
    end

    test "the committed token verifies with the fixture key's public half" do
      assert verifies?(String.trim(File.read!(@token_path)), signer().public_point)
    end

    test "a token minted here has the committed token's header and claims" do
      committed = String.trim(File.read!(@token_path))
      minted = ClientAssertion.mint_at(signer(), @iat)

      assert segment(minted, 0) == segment(committed, 0)
      assert without_jti(segment(minted, 1)) == without_jti(segment(committed, 1))
      assert verifies?(minted, signer().public_point)
    end
  end

  describe "assertions" do
    test "carry the registered identity and a 60s lifetime, and nothing else" do
      signer = signer()
      token = ClientAssertion.mint_at(signer, System.os_time(:second))

      assert segment(token, 0) == %{"alg" => "ES256", "typ" => "JWT", "kid" => @fixture_kid}

      claims = segment(token, 1)
      assert claims["iss"] == @client
      assert claims["sub"] == @tenant
      assert claims["aud"] == @audience
      assert claims["exp"] - claims["iat"] == 60
      assert Enum.sort(Map.keys(claims)) == ~w(aud exp iat iss jti sub)

      assert claims["jti"] =~
               ~r/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/

      assert verifies?(token, signer.public_point)
    end

    test "each mint has its own jti" do
      signer = signer()
      first = segment(ClientAssertion.mint_at(signer, @iat), 1)["jti"]
      second = segment(ClientAssertion.mint_at(signer, @iat), 1)["jti"]
      refute first == second
    end

    test "a cached assertion is reused until 15s before it expires" do
      signer = signer()
      now = System.os_time(:second)

      fresh = {ClientAssertion.mint_at(signer, now - 40), now + 20}
      assert ClientAssertion.fresh(signer, fresh, now) == fresh

      expiring = {ClientAssertion.mint_at(signer, now - 45), now + 15}
      {token, exp} = ClientAssertion.fresh(signer, expiring, now)
      refute token == elem(expiring, 0)
      assert exp == now + 60
      assert segment(token, 1)["iat"] == now

      assert {_token, _exp} = ClientAssertion.fresh(signer, nil, now)
    end

    test "the server mints once and serves the cached assertion" do
      name = :"client_assertion_#{System.unique_integer([:positive])}"
      start_supervised!({ClientAssertion, Keyword.put(settings(), :name, name)})

      {:ok, token} = ClientAssertion.bearer(name)
      assert {:ok, ^token} = ClientAssertion.bearer(name)
      assert verifies?(token, signer().public_point)
    end

    test "the server's status never carries the private key" do
      name = :"client_assertion_#{System.unique_integer([:positive])}"
      start_supervised!({ClientAssertion, Keyword.put(settings(), :name, name)})
      {:ok, _token} = ClientAssertion.bearer(name)

      status = inspect(:sys.get_status(name), limit: :infinity, printable_limit: :infinity)
      refute status =~ inspect(signer().private_key, limit: :infinity)
      assert status =~ ":redacted"
    end
  end

  describe "settings refused up front" do
    test "a signing key path is read from disk" do
      assert {:ok, _} = ClientAssertion.new(settings(signing_key: @key_path))
    end

    test "a missing identity value names its variable" do
      assert {:error, message} = ClientAssertion.new(settings(client_id: nil))
      assert message =~ "ALLSOURCE_CLIENT_ID"

      assert {:error, message} = ClientAssertion.new(settings(tenant_id: "", kid: nil))
      assert message =~ "ALLSOURCE_TENANT_ID"
      assert message =~ "ALLSOURCE_CLIENT_KID"

      assert {:error, message} = ClientAssertion.new(settings(audience: ""))
      assert message =~ "ALLSOURCE_ASSERTION_AUDIENCE"
    end

    test "a key that is not a PKCS#8 PEM is refused" do
      bad = "-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n"
      assert {:error, message} = ClientAssertion.new(settings(signing_key: bad))
      assert message =~ "P-256 PKCS#8"
    end

    test "an unreadable path is refused" do
      missing = Path.join(System.tmp_dir!(), "no-such-key-#{System.unique_integer()}.pem")
      assert {:error, message} = ClientAssertion.new(settings(signing_key: missing))
      assert message =~ "neither a PEM nor a readable file"
    end

    test "a key on another curve is refused" do
      p384 = :public_key.generate_key({:namedCurve, {1, 3, 132, 0, 34}})
      pem = :public_key.pem_encode([:public_key.pem_entry_encode(:PrivateKeyInfo, p384)])

      assert {:error, message} = ClientAssertion.new(settings(signing_key: pem))
      assert message =~ "P-256 PKCS#8"
    end

    test "a kid that is not the key's thumbprint is refused" do
      assert {:error, message} = ClientAssertion.new(settings(kid: "kid-1"))
      assert message =~ "RFC 7638 thumbprint"
      assert message =~ @fixture_kid
    end

    test "the server refuses to start on bad settings" do
      Process.flag(:trap_exit, true)

      capture_log(fn ->
        send(self(), ClientAssertion.start_link(settings(kid: "kid-1", name: nil)))
      end)

      assert_received {:error, message}
      assert message =~ "RFC 7638 thumbprint"
    end
  end
end
