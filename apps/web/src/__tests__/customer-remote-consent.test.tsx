import { render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import ClaudeConsentPage from "@/app/connect/claude/page";

const { getCookie } = vi.hoisted(() => ({ getCookie: vi.fn() }));
vi.mock("next/headers", () => ({ cookies: async () => ({ get: getCookie }) }));
vi.mock("next/server", () => ({ connection: async () => {} }));
afterEach(() => {
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

it("renders explicit field consent without serializing encrypted request, state or credentials", async () => {
  vi.stubEnv("CUSTOMER_REMOTE_ENABLED", "true");
  vi.stubEnv("CUSTOMER_CONNECTIONS_ENABLED", "true");
  vi.stubEnv("CUSTOMER_OAUTH_ISSUER", "https://www.example.test");
  vi.stubEnv("QUERY_SERVICE_URL", "http://127.0.0.1:3902");
  const token = `header.${Buffer.from(JSON.stringify({ sub: "synthetic-subject", tenant_id: "synthetic-workspace", provider: "google", email_verified: true })).toString("base64url")}.signature`;
  getCookie.mockImplementation((name: string) => ({
    value: name === "auth_token" ? token : "encrypted-request-never-render",
  }));
  vi.stubGlobal(
    "fetch",
    vi
      .fn()
      .mockResolvedValueOnce(
        Response.json({
          redirect_uri: "https://claude.ai/api/mcp/auth_callback",
          issuer: "https://www.example.test",
          resource: "https://www.example.test/mcp/customer-review",
          state: "private-state-never-render",
        })
      )
      .mockResolvedValueOnce(Response.json({ data: { connections: [] } }))
  );
  const { container } = render(await ClaudeConsentPage({ searchParams: Promise.resolve({}) }));
  expect(screen.getByRole("checkbox")).toBeRequired();
  expect(screen.getByRole("button", { name: "Allow connection" })).toHaveAttribute(
    "value",
    "allow"
  );
  expect(screen.getByText("synthetic-workspace")).toBeInTheDocument();
  expect(container.textContent).toContain("cannot approve reviews or execute changes");
  expect(container.innerHTML).not.toContain("encrypted-request-never-render");
  expect(container.innerHTML).not.toContain("private-state-never-render");
  expect(container.innerHTML).not.toContain(token);
  expect(container.querySelector("form")).toHaveAttribute("method", "post");
});
