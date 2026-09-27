// @vitest-environment node
import { NextRequest } from "next/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DELETE, GET, POST, PUT } from "@/app/api/team/[...path]/route";

function session(extra = {}) {
  return `header.${Buffer.from(JSON.stringify({ sub: "oauth:google:one", tenant_id: "old", exp: Math.floor(Date.now() / 1000) + 300, ...extra })).toString("base64url")}.signature`;
}
const origin = "https://www.all-source.xyz";
function request(path: string, method = "POST", headers = {}, body = "{}") {
  return new NextRequest(`${origin}/api/team/${path}`, {
    method,
    headers: {
      cookie: `auth_token=${session()}; unrelated=private`,
      origin,
      "content-type": "application/json",
      ...headers,
    },
    ...(method === "GET" || method === "DELETE" ? {} : { body }),
  });
}
function context(path: string[]) {
  return { params: Promise.resolve({ path }) };
}

describe("team browser session boundary", () => {
  beforeEach(() => {
    vi.stubEnv("NEXT_PUBLIC_APP_URL", origin);
    vi.stubEnv("CONTROL_PLANE_INTERNAL_URL", "http://cp.test");
  });
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.unstubAllGlobals();
  });
  it("sets joined session HttpOnly and strips credential from browser JSON", async () => {
    const next = session({ tenant_id: "new" });
    const fetcher = vi.fn().mockResolvedValue(Response.json({ token: next, tenant_id: "new" }));
    vi.stubGlobal("fetch", fetcher);
    const req = request("join");
    const response = await POST(req, context(["join"]));
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({ joined: true, tenant_id: "new" });
    expect(response.headers.get("set-cookie")).toContain("HttpOnly");
    expect(response.headers.get("cache-control")).toBe("no-store");
    expect(fetcher.mock.calls[0]?.[0]).toBe("http://cp.test/api/v1/teams/join");
    expect(fetcher.mock.calls[0]?.[1]).toMatchObject({
      redirect: "error",
      cache: "no-store",
      headers: { authorization: `Bearer ${req.cookies.get("auth_token")?.value}` },
    });
    expect(fetcher.mock.calls[0]?.[1].headers.cookie).toBeUndefined();
  });
  it.each([
    { origin: "https://evil.test" },
    { origin: "" },
    { "sec-fetch-site": "cross-site" },
    { authorization: "Bearer other" },
    { cookie: "" },
    { cookie: "auth_token=header.bnVsbA.signature" },
    { cookie: `auth_token=${session({ is_api_key: true })}` },
    { cookie: `auth_token=${session({ view_as: true })}` },
    { cookie: `auth_token=${session({ is_demo: true })}` },
  ])("rejects unauthorized browser mutation before upstream call: %j", async (headers) => {
    const fetcher = vi.fn();
    vi.stubGlobal("fetch", fetcher);
    const response = await POST(request("join", "POST", headers), context(["join"]));
    expect([401, 403]).toContain(response.status);
    expect(fetcher).not.toHaveBeenCalled();
  });
  it("rejects oversized request and disallowed path", async () => {
    const fetcher = vi.fn();
    vi.stubGlobal("fetch", fetcher);
    expect(
      (await POST(request("join", "POST", {}, "x".repeat(4097)), context(["join"]))).status
    ).toBe(413);
    expect(
      (await PUT(request("members/../role", "PUT"), context(["members", "..", "role"]))).status
    ).toBe(404);
    expect(fetcher).not.toHaveBeenCalled();
  });
  it("maps member role route safely and returns bodyless 204 for removal", async () => {
    const fetcher = vi.fn().mockResolvedValue(new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetcher);
    const response = await DELETE(
      request("members/oauth:google:one", "DELETE"),
      context(["members", "oauth:google:one"])
    );
    expect(response.status).toBe(204);
    expect(await response.text()).toBe("");
    expect(fetcher.mock.calls[0]?.[0]).toBe(
      "http://cp.test/api/v1/teams/members/oauth%3Agoogle%3Aone"
    );
  });
  it("denies a different subject or expired replacement session", async () => {
    for (const extra of [{ sub: "outsider" }, { exp: 1 }]) {
      vi.stubGlobal(
        "fetch",
        vi
          .fn()
          .mockResolvedValue(
            Response.json({ token: session({ tenant_id: "new", ...extra }), tenant_id: "new" })
          )
      );
      const response = await POST(request("join"), context(["join"]));
      expect(response.status).toBe(502);
      expect(response.headers.get("set-cookie")).toBeNull();
    }
  });
  it("fails closed for upstream outage and bounded oversized response", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("private upstream detail")));
    const unavailable = await GET(request("members", "GET"), context(["members"]));
    expect(unavailable.status).toBe(502);
    expect(await unavailable.text()).not.toContain("private upstream detail");
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("x".repeat(262145))));
    expect((await GET(request("members", "GET"), context(["members"]))).status).toBe(502);
  });
});
