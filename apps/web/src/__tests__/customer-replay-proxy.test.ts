// @vitest-environment node
import { createHash, createHmac } from "node:crypto";
import { NextRequest } from "next/server";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { POST } from "@/app/api/customer-agent/replay/[operation]/route";

const origin = "https://www.all-source.xyz";
const secret = "synthetic-only-product-relay-secret-2026";
const token = `header.${Buffer.from(JSON.stringify({ sub: "oauth:google:one", tenant_id: "synthetic", provider: "google", email_verified: true })).toString("base64url")}.signature`;
const context = { params: Promise.resolve({ operation: "approve" }) };
const body = {
  connection_id: "synthetic",
  input: { version: 1, id: "review", digest: "a".repeat(64) },
};
const hash = (value: string) => createHash("sha256").update(value).digest("hex");
function request(headers = {}, value = JSON.stringify(body)) {
  return new NextRequest(`${origin}/api/customer-agent/replay/approve`, {
    method: "POST",
    body: value,
    headers: {
      origin,
      cookie: `auth_token=${token}; unrelated=private`,
      "content-type": "application/json",
      ...headers,
    },
  });
}
beforeEach(() => {
  for (const key of [
    "CUSTOMER_REPLAY_ENABLED",
    "CUSTOMER_EVIDENCE_ENABLED",
    "CUSTOMER_CONNECTIONS_ENABLED",
  ])
    vi.stubEnv(key, "true");
  vi.stubEnv("CUSTOMER_HUMAN_ACTION_SECRET", secret);
  vi.stubEnv("NEXT_PUBLIC_APP_URL", origin);
  vi.stubEnv("QUERY_SERVICE_URL", "http://query.test");
});
afterEach(() => {
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
});
it("binds server proof to canonical body, operation and session without returning the proof", async () => {
  const fetcher = vi.fn().mockResolvedValue(Response.json({ data: { state: "approved" } }));
  vi.stubGlobal("fetch", fetcher);
  const response = await POST(request(), context);
  expect(response.status).toBe(200);
  const [url, init] = fetcher.mock.calls[0]!;
  expect(String(url)).toBe("http://query.test/api/customer-agent/replay/approve");
  expect(init.headers.cookie).toBeUndefined();
  const [encoded, signature] = init.headers["x-allsource-product-action"].split(".");
  expect(signature).toBe(createHmac("sha256", secret).update(encoded).digest("base64url"));
  const claims = JSON.parse(Buffer.from(encoded, "base64url").toString());
  // Fixed independent vector matching ReviewOwner's sorted object-pair encoding.
  const canonical = [
    ["connection_id", "synthetic"],
    [
      "input",
      [
        ["digest", "a".repeat(64)],
        ["id", "review"],
        ["version", 1],
      ],
    ],
  ];
  expect(claims).toMatchObject({
    v: 1,
    aud: "allsource-product-action",
    op: "approve",
    session_sha256: hash(token),
    body_sha256: hash(JSON.stringify(canonical)),
  });
  expect(claims.exp - claims.iat).toBe(30);
  expect(init).toMatchObject({ cache: "no-store", redirect: "error" });
  expect(await response.text()).not.toContain(encoded);
  expect(response.headers.get("cache-control")).toBe("no-store");
});
it.each([
  { origin: "https://evil.test" },
  { origin: "" },
  { authorization: "Bearer agent" },
  { "x-allsource-product-action": "forged" },
  { "sec-fetch-site": "cross-site" },
  { cookie: "" },
])("denies browser authority %j before upstream", async (headers) => {
  const fetcher = vi.fn();
  vi.stubGlobal("fetch", fetcher);
  expect([401, 403]).toContain((await POST(request(headers), context)).status);
  expect(fetcher).not.toHaveBeenCalled();
});
it("fails closed on missing flags or relay secret and bounds both sides", async () => {
  const fetcher = vi.fn();
  vi.stubGlobal("fetch", fetcher);
  vi.stubEnv("CUSTOMER_REPLAY_ENABLED", "false");
  expect((await POST(request(), context)).status).toBe(404);
  vi.stubEnv("CUSTOMER_REPLAY_ENABLED", "true");
  vi.stubEnv("CUSTOMER_HUMAN_ACTION_SECRET", "");
  expect((await POST(request(), context)).status).toBe(502);
  expect(fetcher).not.toHaveBeenCalled();
  expect((await POST(request({}, "x".repeat(16_385)), context)).status).toBe(400);
  vi.stubEnv("CUSTOMER_HUMAN_ACTION_SECRET", secret);
  fetcher.mockResolvedValue(new Response("x".repeat(65_537)));
  expect((await POST(request(), context)).status).toBe(502);
});
