import { NextRequest } from "next/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import { POST } from "@/app/api/design-partners/applications/route";

function applicationRequest(body: string): NextRequest {
  return new NextRequest("http://localhost/api/design-partners/applications", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-forwarded-for": "203.0.113.5, 10.0.0.2",
    },
    body,
  });
}

describe("POST /api/design-partners/applications", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
  });

  it("forwards application to Control Plane without changing private fields", async () => {
    vi.stubEnv("CONTROL_PLANE_INTERNAL_URL", "https://control.example.test");
    const body = JSON.stringify({ name: "Ada", email: "ada@example.com" });
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        Response.json({ application_id: "app-1", status: "new" }, { status: 201 })
      );
    vi.stubGlobal("fetch", fetchMock);

    const response = await POST(applicationRequest(body));

    expect(fetchMock).toHaveBeenCalledWith(
      "https://control.example.test/api/v1/design-partners/applications",
      expect.objectContaining({
        method: "POST",
        body,
        cache: "no-store",
        headers: expect.objectContaining({ "x-forwarded-for": "203.0.113.5" }),
      })
    );
    expect(response.status).toBe(201);
    await expect(response.json()).resolves.toEqual({ application_id: "app-1", status: "new" });
  });

  it("returns a generic 503 when Control Plane is unavailable", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("private upstream detail")));

    const response = await POST(applicationRequest("{}"));

    expect(response.status).toBe(503);
    await expect(response.json()).resolves.toEqual({
      error: "application_unavailable",
      message: "Applications are temporarily unavailable. Please try again later.",
    });
  });

  it("rejects oversized bodies before forwarding", async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);

    const response = await POST(applicationRequest("x".repeat(16 * 1024 + 1)));

    expect(response.status).toBe(413);
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
