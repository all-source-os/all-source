/**
 * The ES256 assertion provider mints what the gateway verifies. The interop
 * fixtures are the Rust SDK's; the gateway's asserted_keys_interop_test.go
 * verifies the same pair with the same client, tenant, audience and issue time.
 */

import { afterEach, describe, expect, setSystemTime, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
  ASSERTION_LIFETIME_SECS,
  ASSERTION_REFRESH_SECS,
  CredentialError,
  Es256AssertionProvider,
} from "../src";

const AUDIENCE = "https://api.all-source.xyz";
const FIXTURE_DIR = join(import.meta.dir, "..", "..", "rust", "tests", "fixtures");
const INTEROP_TEST_KEY = readFileSync(join(FIXTURE_DIR, "es256_interop_test_key.pem"), "utf8");
const INTEROP_TOKEN = readFileSync(join(FIXTURE_DIR, "es256_interop_token.jwt"), "utf8").trim();
const INTEROP_CLIENT = "longhand-control-plane";
const INTEROP_TENANT = "asserted-tenant";
const INTEROP_IAT = 1_794_009_600;

const subtle = globalThis.crypto.subtle;
const encoder = new TextEncoder();
const ECDSA_P256 = { name: "ECDSA", namedCurve: "P-256" } as const;
const ES256 = { name: "ECDSA", hash: "SHA-256" } as const;

function base64url(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function fromBase64url(text: string): Uint8Array {
  const standard = text.replace(/-/g, "+").replace(/_/g, "/");
  const binary = atob(standard.padEnd(Math.ceil(standard.length / 4) * 4, "="));
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

function segment(token: string, index: number): Record<string, unknown> {
  const part = token.split(".")[index];
  if (part === undefined) throw new Error("token has fewer than three segments");
  return JSON.parse(new TextDecoder().decode(fromBase64url(part)));
}

function pemFromPkcs8(der: ArrayBuffer): string {
  const body = btoa(String.fromCharCode(...new Uint8Array(der)));
  const lines = body.match(/.{1,64}/g) ?? [];
  return `-----BEGIN PRIVATE KEY-----\n${lines.join("\n")}\n-----END PRIVATE KEY-----\n`;
}

function pemDer(pem: string): Uint8Array {
  const body = pem
    .split(/\r?\n/)
    .filter((line) => line && !line.startsWith("-----"))
    .join("");
  const binary = atob(body);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/** The PEM key's RFC 7638 thumbprint and its public verifying key. */
async function publicHalf(pem: string): Promise<{ kid: string; verifier: CryptoKey }> {
  const privateKey = await subtle.importKey("pkcs8", pemDer(pem), ECDSA_P256, true, ["sign"]);
  const jwk = await subtle.exportKey("jwk", privateKey);
  const canonical = `{"crv":"P-256","kty":"EC","x":"${jwk.x}","y":"${jwk.y}"}`;
  const kid = base64url(new Uint8Array(await subtle.digest("SHA-256", encoder.encode(canonical))));
  const verifier = await subtle.importKey(
    "jwk",
    { kty: "EC", crv: "P-256", x: jwk.x, y: jwk.y },
    ECDSA_P256,
    false,
    ["verify"]
  );
  return { kid, verifier };
}

async function verifies(token: string, verifier: CryptoKey): Promise<boolean> {
  const [header, claims, signature] = token.split(".");
  if (header === undefined || claims === undefined || signature === undefined) return false;
  return subtle.verify(
    ES256,
    verifier,
    fromBase64url(signature),
    encoder.encode(`${header}.${claims}`)
  );
}

async function generatedPem(curve: "P-256" | "P-384" = "P-256"): Promise<{
  pem: string;
  verifier: CryptoKey;
}> {
  const pair = await subtle.generateKey({ name: "ECDSA", namedCurve: curve }, true, [
    "sign",
    "verify",
  ]);
  const pem = pemFromPkcs8(await subtle.exportKey("pkcs8", pair.privateKey));
  return { pem, verifier: pair.publicKey };
}

function withoutJti(claims: Record<string, unknown>): Record<string, unknown> {
  expect(typeof claims.jti).toBe("string");
  expect(claims.jti).not.toBe("");
  const { jti: _jti, ...rest } = claims;
  return rest;
}

afterEach(() => {
  setSystemTime();
});

function setNow(secs: number) {
  setSystemTime(new Date(secs * 1000));
}

describe("ES256 interop fixtures", () => {
  test("the committed token verifies with the fixture key's public half", async () => {
    const { kid, verifier } = await publicHalf(INTEROP_TEST_KEY);

    expect(segment(INTEROP_TOKEN, 0).kid).toBe(kid);
    expect(await verifies(INTEROP_TOKEN, verifier)).toBe(true);
  });

  test("the provider mints the committed header and claims at the pinned iat", async () => {
    const { kid, verifier } = await publicHalf(INTEROP_TEST_KEY);
    const provider = await Es256AssertionProvider.fromPkcs8Pem(INTEROP_TEST_KEY, {
      clientId: INTEROP_CLIENT,
      tenant: INTEROP_TENANT,
      kid,
      audience: AUDIENCE,
    });

    const minted = await provider.mintAt(INTEROP_IAT);

    expect(minted.split(".")[0]).toBe(INTEROP_TOKEN.split(".")[0]);
    expect(segment(minted, 0)).toEqual(segment(INTEROP_TOKEN, 0));
    expect(withoutJti(segment(minted, 1))).toEqual(withoutJti(segment(INTEROP_TOKEN, 1)));
    expect(await verifies(minted, verifier)).toBe(true);
  });
});

describe("Es256AssertionProvider", () => {
  test("the assertion carries the registered identity and a 60s lifetime", async () => {
    const { pem, verifier } = await generatedPem();
    const provider = await Es256AssertionProvider.fromPkcs8Pem(pem, {
      clientId: "longhand-control-plane",
      tenant: "onboard-tenant",
      kid: "kid-1",
      audience: AUDIENCE,
    });

    const token = await provider.bearer();

    expect(segment(token, 0)).toEqual({ typ: "JWT", alg: "ES256", kid: "kid-1" });
    const claims = segment(token, 1);
    expect(claims.iss).toBe("longhand-control-plane");
    expect(claims.sub).toBe("onboard-tenant");
    expect(claims.aud).toBe(AUDIENCE);
    expect((claims.exp as number) - (claims.iat as number)).toBe(ASSERTION_LIFETIME_SECS);
    expect(Math.abs((claims.iat as number) - Date.now() / 1000)).toBeLessThan(5);
    expect(claims.role).toBeUndefined();
    expect(claims.scope).toBeUndefined();
    withoutJti(claims);
    expect(await verifies(token, verifier)).toBe(true);
    expect(fromBase64url(token.split(".")[2] ?? "").length).toBe(64);
  });

  test("reuses a token until 15s before exp, then mints one shared by concurrent callers", async () => {
    const start = 1_800_000_000;
    setNow(start);
    const { pem } = await generatedPem();
    const provider = await Es256AssertionProvider.fromPkcs8Pem(pem, {
      clientId: "c",
      tenant: "t",
      kid: "k",
      audience: AUDIENCE,
    });

    const first = await provider.bearer();
    setNow(start + ASSERTION_LIFETIME_SECS - ASSERTION_REFRESH_SECS - 1);
    expect(await provider.bearer()).toBe(first);

    setNow(start + ASSERTION_LIFETIME_SECS - ASSERTION_REFRESH_SECS);
    const [a, b] = await Promise.all([provider.bearer(), provider.bearer()]);
    expect(a).not.toBe(first);
    expect(b).toBe(a);
    expect(segment(a, 1).iat).toBe(start + ASSERTION_LIFETIME_SECS - ASSERTION_REFRESH_SECS);
  });

  test("a key that is not a P-256 PKCS#8 PEM is refused up front", async () => {
    const identity = { clientId: "c", tenant: "t", kid: "k", audience: AUDIENCE };

    await expect(
      Es256AssertionProvider.fromPkcs8Pem(
        "-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n",
        identity
      )
    ).rejects.toBeInstanceOf(CredentialError);

    const { pem: p384 } = await generatedPem("P-384");
    await expect(Es256AssertionProvider.fromPkcs8Pem(p384, identity)).rejects.toBeInstanceOf(
      CredentialError
    );

    await expect(
      Es256AssertionProvider.fromPkcs8Pem(
        "-----BEGIN EC PRIVATE KEY-----\nAAAA\n-----END EC PRIVATE KEY-----\n",
        identity
      )
    ).rejects.toThrow("openssl pkcs8 -topk8 -nocrypt");
  });

  test("an empty identity field is refused up front", async () => {
    const { pem } = await generatedPem();
    await expect(
      Es256AssertionProvider.fromPkcs8Pem(pem, {
        clientId: "",
        tenant: "t",
        kid: "k",
        audience: AUDIENCE,
      })
    ).rejects.toThrow("clientId, tenant, kid and audience are all required");
  });
});
