import { NextRequest } from "next/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import { GET, POST } from "@/app/api/v1/auth/[...path]/route";
import { authRedirect } from "@/lib/auth-redirect";

describe("email auth product-session boundary", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
  });
  it.each(["register", "login"])("routes %s through CP and keeps token HttpOnly", async (path) => {
    vi.stubEnv("AUTH_SERVICE_URL", "http://auth.test");
    vi.stubEnv("CONTROL_PLANE_INTERNAL_URL", "http://cp.test");
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        Response.json({ token: "secret-session", new_user: true }, { status: 201 })
      );
    vi.stubGlobal("fetch", fetchMock);
    const response = await POST(
      new NextRequest(`https://www.all-source.xyz/api/v1/auth/${path}`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: "{}",
      }),
      { params: Promise.resolve({ path: [path] }) }
    );
    expect(fetchMock.mock.calls[0]?.[0]).toBe(`http://cp.test/api/v1/auth/${path}`);
    expect(await response.json()).toEqual({ session_established: true, new_user: true });
    expect(response.headers.get("set-cookie")).toContain("HttpOnly");
    expect(response.headers.get("cache-control")).toBe("no-store");
  });
  it("forwards product session to Query Service for me", async () => {
    vi.stubEnv("QUERY_SERVICE_URL", "http://query.test");
    const fetchMock = vi.fn().mockResolvedValue(Response.json({ user: {} }));
    vi.stubGlobal("fetch", fetchMock);
    await GET(
      new NextRequest("https://www.all-source.xyz/api/v1/auth/me", {
        headers: { cookie: "auth_token=session" },
      }),
      { params: Promise.resolve({ path: ["me"] }) }
    );
    expect(fetchMock).toHaveBeenCalledWith(
      "http://query.test/api/v1/auth/me",
      expect.objectContaining({
        headers: expect.objectContaining({ authorization: "Bearer session" }),
      })
    );
  });
  it("does not set session cookie for rejected credentials", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(Response.json({ message: "Invalid credentials" }, { status: 401 }))
    );
    const response = await POST(
      new NextRequest("https://www.all-source.xyz/api/v1/auth/login", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: "{}",
      }),
      { params: Promise.resolve({ path: ["login"] }) }
    );
    expect(response.status).toBe(401);
    expect(response.headers.get("set-cookie")).toBeNull();
  });
  it.each([
    "//evil.test",
    "/\\evil.test",
    "/api/auth/logout",
    "/api?x=1",
    "/\tevil.test",
  ])("rejects unsafe redirect %s", (path) => {
    expect(authRedirect(path, false)).toBe("/dashboard");
  });
  it("retains safe deep links and onboarding", () => {
    expect(authRedirect("/connect?flow=cli", true)).toBe("/connect?flow=cli");
    expect(authRedirect(null, true)).toBe("/onboarding");
  });
  it("rejects cross-origin login before setting a cookie", async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    const response = await POST(
      new NextRequest("https://www.all-source.xyz/api/v1/auth/login", {
        method: "POST",
        headers: { origin: "https://evil.example", "content-type": "application/json" },
        body: "{}",
      }),
      { params: Promise.resolve({ path: ["login"] }) }
    );
    expect(response.status).toBe(403);
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
