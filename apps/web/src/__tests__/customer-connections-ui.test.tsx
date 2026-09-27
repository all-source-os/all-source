import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CustomerConnections } from "@/components/settings/customer-connections";

afterEach(() => {
  vi.unstubAllGlobals();
});
describe("customer consent and connection settings", () => {
  it("requires explicit consent, shows secret once, lists and revokes without model approval", async () => {
    const id = "a".repeat(32);
    let created = false;
    let revoked = false;
    const fetcher = vi.fn(async (url: string, init: RequestInit) => {
      if (url.endsWith("create")) {
        expect(JSON.parse(init.body as string).consent).toEqual({
          accepted: true,
          version: "review-metadata-v1",
        });
        created = true;
        return Response.json({
          data: { id, token: "synthetic-only-credential", expires_at: 2_000_000_000, binding: {} },
        });
      }
      if (url.endsWith("revoke")) {
        revoked = true;
        return Response.json({ data: { revoked: true } });
      }
      return Response.json({
        data: {
          connections: created
            ? [
                {
                  id,
                  client_id: "claude-code",
                  resource: "https://api.example.test/review",
                  expires_at: 2_000_000_000,
                  status: revoked ? "revoked" : "active",
                },
              ]
            : [],
        },
      });
    });
    vi.stubGlobal("fetch", fetcher);
    render(<CustomerConnections />);
    await screen.findByText("No connections to display.");
    const create = screen.getByRole("button", { name: "Create connection" });
    expect(create).toBeDisabled();
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(create);
    const secret = await screen.findByLabelText("Connection credential");
    expect(secret).toHaveAttribute("type", "password");
    expect(secret).toHaveValue("synthetic-only-credential");
    fireEvent.click(screen.getByRole("button", { name: "Hide credential" }));
    expect(screen.queryByLabelText("Connection credential")).not.toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Revoke aaaaaaaa" })).toBeEnabled()
    );
    fireEvent.click(screen.getByRole("button", { name: "Revoke aaaaaaaa" }));
    await screen.findByText("revoked");
    expect(screen.queryByRole("button", { name: "Revoke aaaaaaaa" })).not.toBeInTheDocument();
    expect(fetcher.mock.calls.every(([url]) => !url.includes("synthetic-only-credential"))).toBe(
      true
    );
  });
  it("shows failed requests without pretending a connection was created", async () => {
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValue(
          Response.json({ error: { message: "Plan unavailable" } }, { status: 403 })
        )
    );
    render(<CustomerConnections />);
    await screen.findByRole("alert");
    expect(screen.queryByLabelText("Connection credential")).not.toBeInTheDocument();
  });
});
