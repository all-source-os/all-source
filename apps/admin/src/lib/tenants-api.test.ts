import { afterEach, describe, expect, test } from "bun:test";
import {
  CLIENT_KEY_SCOPES,
  type ClientKey,
  clientKeyState,
  fetchClientKeys,
  parsePublicJwk,
  registerClientKey,
  revokeClientKey,
} from "./tenants-api";

const X = "f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU";
const Y = "x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0";
const PUBLIC_JWK = { kty: "EC", crv: "P-256", x: X, y: Y };

interface Call {
  url: string;
  init?: RequestInit;
}

const realFetch = globalThis.fetch;
let calls: Call[] = [];

function stubFetch(status: number, body: unknown) {
  calls = [];
  globalThis.fetch = (async (url: string | URL | Request, init?: RequestInit) => {
    calls.push({ url: String(url), init });
    return new Response(typeof body === "string" ? body : JSON.stringify(body), {
      status,
      headers: { "Content-Type": "application/json" },
    });
  }) as typeof fetch;
}

afterEach(() => {
  globalThis.fetch = realFetch;
});

describe("parsePublicJwk", () => {
  test("keeps only the public members of a P-256 key", () => {
    const jwk = parsePublicJwk(JSON.stringify({ ...PUBLIC_JWK, kid: "ignored", use: "sig" }));
    expect(jwk).toEqual({ kty: "EC", crv: "P-256", x: X, y: Y });
  });

  test("refuses a private JWK before it leaves the browser", () => {
    expect(() => parsePublicJwk(JSON.stringify({ ...PUBLIC_JWK, d: "secret" }))).toThrow(
      "private JWK"
    );
  });

  test("refuses non-JSON, non-objects, other curves and malformed coordinates", () => {
    expect(() => parsePublicJwk("not json")).toThrow("not valid JSON");
    expect(() => parsePublicJwk("[]")).toThrow("JSON object");
    expect(() => parsePublicJwk(JSON.stringify({ ...PUBLIC_JWK, crv: "P-384" }))).toThrow("P-256");
    expect(() => parsePublicJwk(JSON.stringify({ ...PUBLIC_JWK, kty: "RSA" }))).toThrow("P-256");
    expect(() => parsePublicJwk(JSON.stringify({ ...PUBLIC_JWK, x: `${X}=` }))).toThrow("32 bytes");
    expect(() => parsePublicJwk(JSON.stringify({ ...PUBLIC_JWK, y: 42 }))).toThrow("32 bytes");
  });
});

describe("clientKeyState", () => {
  const base: ClientKey = { kid: "k", revoked: false, invalid: false, active: true };
  test("revoked wins over invalid, invalid over active", () => {
    expect(clientKeyState(base)).toBe("active");
    expect(clientKeyState({ ...base, active: false })).toBe("inactive");
    expect(clientKeyState({ ...base, invalid: true, active: false })).toBe("invalid");
    expect(clientKeyState({ ...base, revoked: true, invalid: true, active: false })).toBe(
      "revoked"
    );
  });
});

describe("client key fetchers", () => {
  test("lists the tenant's keys from the client_keys wrapper", async () => {
    stubFetch(200, {
      tenant_id: "acme",
      client_keys: [{ kid: "abc", revoked: false, invalid: false, active: true }],
    });
    const keys = await fetchClientKeys("acme");
    expect(keys).toHaveLength(1);
    expect(keys[0]?.kid).toBe("abc");
    expect(calls[0]?.url).toEndWith("/api/v1/admin/tenants/acme/client-keys");
  });

  test("a null list body resolves to an empty array", async () => {
    stubFetch(200, { tenant_id: "acme", client_keys: null });
    expect(await fetchClientKeys("acme")).toEqual([]);
  });

  test("register posts exactly {client_id, jwk, scopes}", async () => {
    stubFetch(201, { kid: "abc", tenant_id: "acme", client_id: "worker" });
    const result = await registerClientKey("acme", {
      client_id: "worker",
      jwk: parsePublicJwk(JSON.stringify(PUBLIC_JWK)),
      scopes: ["events:read"],
    });
    expect(result.kid).toBe("abc");
    expect(calls[0]?.init?.method).toBe("POST");
    expect(JSON.parse(String(calls[0]?.init?.body))).toEqual({
      client_id: "worker",
      jwk: PUBLIC_JWK,
      scopes: ["events:read"],
    });
  });

  test("register surfaces the Control Plane's message", async () => {
    stubFetch(409, {
      error: "kid_revoked",
      kid: "abc",
      message: "this key was revoked; generate a new keypair",
    });
    await expect(
      registerClientKey("acme", {
        client_id: "worker",
        jwk: parsePublicJwk(JSON.stringify(PUBLIC_JWK)),
        scopes: ["events:read"],
      })
    ).rejects.toThrow(
      "Failed to register client key: this key was revoked; generate a new keypair"
    );
  });

  test("revoke sends DELETE to the encoded kid", async () => {
    stubFetch(200, { kid: "a/b", tenant_id: "acme", revoked_at: "2026-11-07T00:00:00Z" });
    await revokeClientKey("acme", "a/b");
    expect(calls[0]?.init?.method).toBe("DELETE");
    expect(calls[0]?.url).toEndWith("/api/v1/admin/tenants/acme/client-keys/a%2Fb");
  });

  test("a non-JSON error body falls back to the status", async () => {
    stubFetch(503, "upstream down");
    await expect(revokeClientKey("acme", "abc")).rejects.toThrow(
      "Failed to revoke client key: 503"
    );
  });

  test("the scope list matches the Control Plane's managed scopes", () => {
    expect([...CLIENT_KEY_SCOPES].sort()).toEqual([
      "events:read",
      "events:write",
      "projections:read",
      "projections:write",
      "queries:execute",
      "schemas:read",
      "schemas:write",
    ]);
  });
});
