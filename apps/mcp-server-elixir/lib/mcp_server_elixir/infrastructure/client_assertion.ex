defmodule McpServerElixir.Infrastructure.ClientAssertion do
  @moduledoc """
  ES256 client assertions for a gateway that holds this client's registered
  P-256 public key.

  Each assertion is a JWT with header `{alg: ES256, typ: JWT, kid}` and claims
  `iss` (client id), `sub` (tenant), `aud`, `iat`, `exp = iat + 60` and a random
  `jti`. The gateway takes role and scopes from the registration record, never
  from the token. A minted assertion is reused until 15 seconds before it
  expires.

  `new/1` refuses, up front, a missing identity value, a key that is not a P-256
  PKCS#8 PEM, and a `kid` that is not the key's RFC 7638 thumbprint. Started
  under the application supervisor, that refusal stops the release from booting.
  """

  use GenServer

  require Logger

  # The gateway refuses an assertion whose exp - iat exceeds 60s.
  @lifetime_secs 60
  @refresh_secs 15
  @secp256r1 {1, 2, 840, 10_045, 3, 1, 7}

  @enforce_keys [:private_key, :public_point, :client_id, :tenant_id, :kid, :audience]
  defstruct @enforce_keys

  @type t :: %__MODULE__{
          private_key: binary(),
          public_point: binary(),
          client_id: String.t(),
          tenant_id: String.t(),
          kid: String.t(),
          audience: String.t()
        }

  @required [
    client_id: "ALLSOURCE_CLIENT_ID",
    tenant_id: "ALLSOURCE_TENANT_ID",
    kid: "ALLSOURCE_CLIENT_KID",
    audience: "ALLSOURCE_ASSERTION_AUDIENCE"
  ]

  @unreadable_key "ALLSOURCE_CLIENT_SIGNING_KEY is neither a PEM nor a readable file"

  @doc """
  Build a signer from `signing_key` (an inline PEM or a path to one),
  `client_id`, `tenant_id`, `kid` and `audience`.
  """
  @spec new(keyword() | map()) :: {:ok, t()} | {:error, String.t()}
  def new(settings) do
    settings = Map.new(settings)

    with :ok <- require_identity(settings),
         {:ok, pem} <- read_signing_key(settings[:signing_key]),
         {:ok, private_key, public_point} <- decode_p256(pem),
         signer = %__MODULE__{
           private_key: private_key,
           public_point: public_point,
           client_id: settings.client_id,
           tenant_id: settings.tenant_id,
           kid: settings.kid,
           audience: settings.audience
         },
         :ok <- check_kid(signer),
         {:ok, _token} <- safe_mint(signer, System.os_time(:second)) do
      {:ok, signer}
    end
  end

  @doc "The RFC 7638 JWK thumbprint of an uncompressed P-256 public point."
  @spec thumbprint(binary()) :: String.t()
  def thumbprint(<<4, x::binary-size(32), y::binary-size(32)>>) do
    canonical = ~s({"crv":"P-256","kty":"EC","x":"#{b64(x)}","y":"#{b64(y)}"})
    b64(:crypto.hash(:sha256, canonical))
  end

  @doc false
  # Mints an uncached assertion issued at `iat` (Unix seconds), for fixtures
  # that pin the clock.
  @spec mint_at(t(), integer()) :: String.t()
  def mint_at(%__MODULE__{} = signer, iat) do
    header = %{"typ" => "JWT", "alg" => "ES256", "kid" => signer.kid}

    claims = %{
      "iss" => signer.client_id,
      "sub" => signer.tenant_id,
      "aud" => signer.audience,
      "iat" => iat,
      "exp" => iat + @lifetime_secs,
      "jti" => jti()
    }

    signing_input = b64(Jason.encode!(header)) <> "." <> b64(Jason.encode!(claims))
    der = :crypto.sign(:ecdsa, :sha256, signing_input, [signer.private_key, :secp256r1])
    {:"ECDSA-Sig-Value", r, s} = :public_key.der_decode(:"ECDSA-Sig-Value", der)
    signature = <<r::unsigned-big-integer-size(256), s::unsigned-big-integer-size(256)>>

    signing_input <> "." <> b64(signature)
  end

  @doc false
  # The cached `{token, exp}` while it has more than the refresh margin left,
  # otherwise a newly minted one.
  @spec fresh(t(), {String.t(), integer()} | nil, integer()) :: {String.t(), integer()}
  def fresh(_signer, {_token, exp} = cached, now) when now + @refresh_secs < exp, do: cached
  def fresh(signer, _cached, now), do: {mint_at(signer, now), now + @lifetime_secs}

  # Server

  def start_link(opts) do
    {name, settings} = Keyword.pop(opts, :name, __MODULE__)
    GenServer.start_link(__MODULE__, settings, name: name)
  end

  @doc "The current assertion, minted or reused."
  @spec bearer(GenServer.server()) :: {:ok, String.t()} | {:error, term()}
  def bearer(server \\ __MODULE__) do
    GenServer.call(server, :bearer)
  catch
    :exit, reason -> {:error, {:client_assertion_unavailable, reason}}
  end

  @impl true
  def init([]), do: init(Application.get_env(:mcp_server_elixir, :client_assertion, []))

  def init(settings) do
    case new(settings) do
      {:ok, signer} ->
        {:ok, %{signer: signer, cached: nil}}

      {:error, message} ->
        Logger.error("[ClientAssertion] #{message}")
        {:stop, message}
    end
  end

  @impl true
  def handle_call(:bearer, _from, state) do
    {token, _exp} = cached = fresh(state.signer, state.cached, System.os_time(:second))
    {:reply, {:ok, token}, %{state | cached: cached}}
  end

  # Keeps the private key out of crash reports and :sys.get_status/1.
  @impl true
  def format_status(status), do: Map.put(status, :state, :redacted)

  # Construction

  defp require_identity(settings) do
    case Enum.filter(@required, fn {key, _env} -> blank?(settings[key]) end) do
      [] ->
        :ok

      missing ->
        names = Enum.map_join(missing, ", ", fn {_key, env} -> env end)
        {:error, "ALLSOURCE_CLIENT_SIGNING_KEY is set but #{names} is missing"}
    end
  end

  defp blank?(value), do: not is_binary(value) or String.trim(value) == ""

  defp read_signing_key(source) when is_binary(source) do
    if String.contains?(source, "-----BEGIN"), do: {:ok, source}, else: read_key_file(source)
  end

  defp read_signing_key(_source), do: {:error, "ALLSOURCE_CLIENT_SIGNING_KEY is not set"}

  defp read_key_file(path) do
    case File.read(path) do
      {:ok, pem} -> {:ok, pem}
      {:error, _} -> {:error, @unreadable_key}
    end
  end

  defp decode_p256(pem) do
    with [{:PrivateKeyInfo, _der, :not_encrypted} = entry] <- :public_key.pem_decode(pem),
         key when elem(key, 0) == :ECPrivateKey <- :public_key.pem_entry_decode(entry),
         {:namedCurve, @secp256r1} <- elem(key, 3) do
      private_key = elem(key, 2)
      {public_point, _} = :crypto.generate_key(:ecdh, :secp256r1, private_key)
      {:ok, private_key, public_point}
    else
      _ -> not_p256()
    end
  rescue
    _ -> not_p256()
  catch
    _, _ -> not_p256()
  end

  defp not_p256, do: {:error, "ALLSOURCE_CLIENT_SIGNING_KEY is not a P-256 PKCS#8 PEM"}

  defp check_kid(signer) do
    case thumbprint(signer.public_point) do
      kid when kid == signer.kid ->
        :ok

      expected ->
        {:error,
         "ALLSOURCE_CLIENT_KID #{signer.kid} is not the signing key's RFC 7638 thumbprint " <>
           "(#{expected})"}
    end
  end

  defp safe_mint(signer, iat) do
    {:ok, mint_at(signer, iat)}
  rescue
    error -> {:error, "client assertion signing failed: #{Exception.message(error)}"}
  end

  defp jti do
    <<a::48, _::4, b::12, _::2, c::62>> = :crypto.strong_rand_bytes(16)

    <<p1::binary-8, p2::binary-4, p3::binary-4, p4::binary-4, p5::binary-12>> =
      Base.encode16(<<a::48, 4::4, b::12, 2::2, c::62>>, case: :lower)

    Enum.join([p1, p2, p3, p4, p5], "-")
  end

  defp b64(binary), do: Base.url_encode64(binary, padding: false)
end
