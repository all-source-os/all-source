import { CredentialError } from "./credentials";

/** Lifetime of each assertion. The gateway refuses anything longer than 60s. */
export const ASSERTION_LIFETIME_SECS = 60;
/** A cached assertion is replaced this long before it expires. */
export const ASSERTION_REFRESH_SECS = 15;

/** Identity a registered client key asserts. */
export interface Es256AssertionOptions {
  /** The registration's `client_id`; becomes `iss`. */
  clientId: string;
  /** The tenant the key is registered on; becomes `sub`. */
  tenant: string;
  /** The registered key's RFC 7638 thumbprint, as returned on registration. */
  kid: string;
  /** Must equal the gateway's `ALLSOURCE_ASSERTION_AUDIENCE` (default `https://api.all-source.xyz`). */
  audience: string;
  /**
   * WebCrypto implementation. Defaults to `globalThis.crypto` (Node 19+, Bun,
   * browsers). On Node 18 pass `require("node:crypto").webcrypto`.
   */
  crypto?: Crypto;
}

interface Minted {
  token: string;
  exp: number;
}

const encoder = new TextEncoder();

function toArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  const copy = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(copy).set(bytes);
  return copy;
}

function base64url(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function base64urlJson(value: unknown): string {
  return base64url(encoder.encode(JSON.stringify(value)));
}

function pkcs8Der(pem: string): ArrayBuffer {
  const lines = pem.trim().split(/\r?\n/);
  const begin = lines[0]?.trim();
  const end = lines[lines.length - 1]?.trim();
  if (begin === "-----BEGIN EC PRIVATE KEY-----") {
    throw new CredentialError(
      "client signing key is a SEC1 EC key; convert it with `openssl pkcs8 -topk8 -nocrypt`"
    );
  }
  if (begin !== "-----BEGIN PRIVATE KEY-----" || end !== "-----END PRIVATE KEY-----") {
    throw new CredentialError("client signing key is not a P-256 PKCS#8 PEM");
  }
  const body = lines.slice(1, -1).join("").replace(/\s+/g, "");
  let binary: string;
  try {
    binary = atob(body);
  } catch (error) {
    throw new CredentialError("client signing key is not a P-256 PKCS#8 PEM", error);
  }
  const der = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) der[i] = binary.charCodeAt(i);
  return toArrayBuffer(der);
}

function nowSecs(): number {
  return Math.floor(Date.now() / 1000);
}

/**
 * Mints a 60-second ES256 assertion signed with a P-256 private key and
 * reuses it until 15 seconds before it expires. Concurrent callers share one
 * mint. Pass it as the client's `credentials`.
 *
 * Uses WebCrypto only. Build it with `fromPkcs8Pem`, which refuses an unusable
 * key or an empty field up front rather than on the first request.
 */
export class Es256AssertionProvider {
  private cached: Minted | undefined;
  private inflight: Promise<Minted> | undefined;

  private constructor(
    private readonly key: CryptoKey,
    private readonly subtle: SubtleCrypto,
    private readonly randomUUID: () => string,
    private readonly clientId: string,
    private readonly tenant: string,
    private readonly kid: string,
    private readonly audience: string
  ) {}

  static async fromPkcs8Pem(
    pem: string,
    options: Es256AssertionOptions
  ): Promise<Es256AssertionProvider> {
    const { clientId, tenant, kid, audience } = options;
    if (!clientId || !tenant || !kid || !audience) {
      throw new CredentialError("clientId, tenant, kid and audience are all required");
    }
    const crypto = options.crypto ?? globalThis.crypto;
    if (!crypto?.subtle || typeof crypto.randomUUID !== "function") {
      throw new CredentialError(
        "WebCrypto is unavailable; pass `crypto` (Node 18: require('node:crypto').webcrypto)"
      );
    }
    let key: CryptoKey;
    try {
      key = await crypto.subtle.importKey(
        "pkcs8",
        pkcs8Der(pem),
        { name: "ECDSA", namedCurve: "P-256" },
        false,
        ["sign"]
      );
    } catch (error) {
      if (error instanceof CredentialError) throw error;
      const detail = error instanceof Error ? error.message : String(error);
      throw new CredentialError(`client signing key is not a P-256 PKCS#8 PEM: ${detail}`, error);
    }
    const provider = new Es256AssertionProvider(
      key,
      crypto.subtle,
      () => crypto.randomUUID(),
      clientId,
      tenant,
      kid,
      audience
    );
    provider.cached = await provider.mint(nowSecs());
    return provider;
  }

  /** The current assertion, minting a new one when the cached one is within 15s of `exp`. */
  async bearer(): Promise<string> {
    const now = nowSecs();
    if (this.cached && now + ASSERTION_REFRESH_SECS < this.cached.exp) {
      return this.cached.token;
    }
    if (!this.inflight) {
      this.inflight = this.mint(now)
        .then((minted) => {
          this.cached = minted;
          return minted;
        })
        .finally(() => {
          this.inflight = undefined;
        });
    }
    return (await this.inflight).token;
  }

  /** Mints an uncached assertion issued at `iat` (Unix seconds), for fixtures that pin the clock. */
  async mintAt(iat: number): Promise<string> {
    return (await this.mint(iat)).token;
  }

  private async mint(iat: number): Promise<Minted> {
    const exp = iat + ASSERTION_LIFETIME_SECS;
    const header = { typ: "JWT", alg: "ES256", kid: this.kid };
    const claims = {
      iss: this.clientId,
      sub: this.tenant,
      aud: this.audience,
      iat,
      exp,
      jti: this.randomUUID(),
    };
    const signingInput = `${base64urlJson(header)}.${base64urlJson(claims)}`;
    let signature: ArrayBuffer;
    try {
      // WebCrypto's ECDSA output is raw r||s (IEEE P1363), which is the JWS ES256 encoding.
      signature = await this.subtle.sign(
        { name: "ECDSA", hash: "SHA-256" },
        this.key,
        toArrayBuffer(encoder.encode(signingInput))
      );
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      throw new CredentialError(`client assertion signing failed: ${detail}`, error);
    }
    return { token: `${signingInput}.${base64url(new Uint8Array(signature))}`, exp };
  }
}
