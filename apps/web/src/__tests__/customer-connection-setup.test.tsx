import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CustomerConnectionSetup } from "@/components/settings/customer-connection-setup";

const issued = {
  id: "a".repeat(32),
  token: `asreview_v2_${"a".repeat(32)}.${"b".repeat(64)}`,
  expires_at: 2_000_000_000,
  binding: {
    tenant_id: "synthetic-workspace",
    subject_id: "oauth:google:123",
    client_id: "claude-code",
    resource: "https://api.example.test/customer-review",
  },
};

afterEach(() => vi.unstubAllGlobals());

function setup(value: typeof issued & { consent_version?: "review-evidence-v2" } = issued) {
  const writeText = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  render(<CustomerConnectionSetup issued={value} />);
  fireEvent.change(screen.getByLabelText("MCP release directory"), {
    target: { value: "/Users/test/AllSource release" },
  });
  fireEvent.change(screen.getByLabelText("New private connection file"), {
    target: { value: "/Users/test/.allsource-review/review.json" },
  });
  return writeText;
}

describe("private connection installation", () => {
  it("copies complete one-time data separately from shell and host configuration", async () => {
    const clipboard = setup();
    fireEvent.click(screen.getByRole("button", { name: "Copy install command" }));
    await waitFor(() => expect(clipboard).toHaveBeenCalledTimes(1));
    const install = clipboard.mock.calls[0]?.[0] ?? "";
    expect(install).toContain(
      "pbpaste | '/Users/test/AllSource release/bin/allsource-customer-connection' --install"
    );
    expect(install).not.toContain(issued.token);

    fireEvent.click(screen.getByRole("button", { name: "Copy connection data" }));
    await waitFor(() => expect(clipboard).toHaveBeenCalledTimes(2));
    expect(JSON.parse(clipboard.mock.calls[1]?.[0] ?? "")).toEqual({
      version: 1,
      url: "https://api.example.test",
      token: issued.token,
      binding: issued.binding,
    });
    expect(document.body.textContent).not.toContain(issued.token);

    fireEvent.click(screen.getByRole("button", { name: "Copy Claude Code command" }));
    await waitFor(() => expect(clipboard).toHaveBeenCalledTimes(3));
    const host = clipboard.mock.calls[2]?.[0] ?? "";
    expect(host).toContain("claude mcp add-json --scope local allsource_review");
    expect(host).toContain(
      '"CUSTOMER_REVIEW_CONNECTION_FILE":"/Users/test/.allsource-review/review.json"'
    );
    expect(host).not.toContain(issued.token);
    expect(host).not.toContain("CUSTOMER_REVIEW_GRANT");
    expect(host).not.toContain("ALLSOURCE_CUSTOMER_EVIDENCE_REVIEW");
  });

  it("enables evidence tools only for the issued evidence consent", async () => {
    const clipboard = setup({ ...issued, consent_version: "review-evidence-v2" });
    fireEvent.click(screen.getByRole("button", { name: "Copy Claude Code command" }));
    await waitFor(() => expect(clipboard).toHaveBeenCalledTimes(1));
    expect(clipboard.mock.calls[0]?.[0]).toContain('"ALLSOURCE_CUSTOMER_EVIDENCE_REVIEW":"true"');
  });

  it("quotes metacharacters literally and rejects relative or multiline paths", async () => {
    const clipboard = setup();
    fireEvent.change(screen.getByLabelText("MCP release directory"), {
      target: { value: "/Users/test/it's $(touch injected)" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Copy install command" }));
    await waitFor(() => expect(clipboard).toHaveBeenCalledTimes(1));
    expect(clipboard.mock.calls[0]?.[0] ?? "").toContain(
      "'/Users/test/it'\\''s $(touch injected)/bin/allsource-customer-connection'"
    );
    fireEvent.change(screen.getByLabelText("New private connection file"), {
      target: { value: "~/review.json" },
    });
    expect(screen.getByRole("button", { name: "Copy connection data" })).toBeDisabled();
  });

  it("does not copy data with a non-HTTPS or credential-bearing audience", async () => {
    const clipboard = setup({
      ...issued,
      binding: { ...issued.binding, resource: "https://secret@other.test/review" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Copy connection data" }));
    await screen.findByText(
      "Connection configuration is unavailable. Revoke this connection and try again."
    );
    expect(clipboard).not.toHaveBeenCalled();
  });
});
