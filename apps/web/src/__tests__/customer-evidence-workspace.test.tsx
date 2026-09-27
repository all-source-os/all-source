import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CustomerEvidenceSource } from "@/components/agent-reviews/customer-evidence-source";
import { CustomerEvidenceWorkspace } from "@/components/agent-reviews/customer-evidence-workspace";

const connection = {
  id: "a".repeat(32),
  client_id: "claude-code",
  consent: { version: "review-evidence-v2" },
  status: "active",
  expires_at: 2_000_000_000,
};
const run = "00000000-0000-4000-8000-000000000001";
const pin = { kind: "run_evidence", ref: "b".repeat(32), revision: 6, sha256: "c".repeat(64) };
const inspection = {
  run_id: run,
  revision: 6,
  sha256: pin.sha256,
  changes: 1,
  attempts: 1,
  completed: true,
  unknowns: [],
  shared: false,
};
const receipt = {
  id: "d".repeat(32),
  version: 1,
  digest: "e".repeat(64),
  expires_at: 2_000_000_000,
  status: "saved",
};
const saved = {
  sources: [{ run_id: run, source: pin, expires_at: 2_000_000_000, status: "saved" }],
  reviews: [receipt],
  connection_status: "active",
};
const view = {
  ...receipt,
  state: "pending",
  approved: false,
  execution: "none",
  evidence: {
    state: "divergent",
    baseline: { run_id: run, revision: 6, digest: pin.sha256 },
    candidate: {
      run_id: "00000000-0000-4000-8000-000000000002",
      revision: 6,
      digest: "f".repeat(64),
    },
    descriptor_changes: [],
    first_divergence: { change_number: 1 },
    unknowns: ["capture_gap"],
    differences: [
      {
        change_number: 1,
        baseline: {
          state: "accepted",
          evidence_sha256: "1".repeat(64),
          attempts: [{ state: "succeeded", test_outcome: "pass", test_sha256: "2".repeat(64) }],
          evidence: [],
        },
        candidate: null,
      },
    ],
    execution: "none",
    approval_authority: "not_established",
  },
};
const json = (data: unknown) => Response.json({ data });
afterEach(() => {
  vi.unstubAllGlobals();
});

describe("product evidence workspace", () => {
  it("shows a loading failure without claiming there are no connections", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("unavailable")));
    render(<CustomerEvidenceWorkspace />);
    await screen.findByText("Connections could not be loaded. Retry below.");
    expect(screen.queryByText(/No evidence connections/)).not.toBeInTheDocument();
  });
  it("inspects without disclosure, requires explicit sharing consent, and preserves retry intent", async () => {
    let shares = 0;
    const fetcher = vi.fn(async (url: string, _init: RequestInit) => {
      if (url.endsWith("inspect-run")) return json(inspection);
      shares++;
      if (shares === 1) throw new Error("Interrupted");
      return json({ source: pin });
    });
    vi.stubGlobal("fetch", fetcher);
    const onShared = vi.fn().mockResolvedValue(undefined);
    render(<CustomerEvidenceSource connection={connection.id} onShared={onShared} />);
    fireEvent.change(screen.getByLabelText("Recorded run ID"), { target: { value: run } });
    fireEvent.click(screen.getByRole("button", { name: "Inspect run · 1 query" }));
    await screen.findByText(/Revision 6/);
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Share this revision · 1 query" })).toBeDisabled();
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(screen.getByRole("button", { name: "Share this revision · 1 query" }));
    await screen.findByRole("alert");
    fireEvent.click(screen.getByRole("button", { name: "Retry same share · 1 query" }));
    await waitFor(() => expect(onShared).toHaveBeenCalledOnce());
    expect(fetcher.mock.calls[1]?.[1]?.body).toEqual(fetcher.mock.calls[2]?.[1]?.body);
    const request = JSON.parse((fetcher.mock.calls[1]?.[1] as RequestInit).body as string);
    expect(request).toMatchObject({
      connection_id: connection.id,
      source: {
        run_id: run,
        revision: 6,
        sha256: pin.sha256,
        consent: { accepted: true, version: "selected-run-evidence-v1" },
      },
    });
  });

  it("recovers saved work, opens exact evidence, clears failed views, and revokes selected source", async () => {
    let reads = 0;
    let revoked = false;
    const fetcher = vi.fn(async (url: string, init: RequestInit) => {
      if (url.endsWith("list"))
        return json({
          connections: [
            connection,
            { ...connection, id: "metadata", consent: { version: "review-metadata-v1" } },
          ],
        });
      if (url.endsWith("workspace"))
        return json({
          ...saved,
          sources: saved.sources.map((item) => ({
            ...item,
            status: revoked ? "revoked" : "saved",
          })),
        });
      if (url.endsWith("revoke-source")) {
        revoked = true;
        expect(JSON.parse(init.body as string)).toEqual({
          connection_id: connection.id,
          source_id: pin.ref,
        });
        return json({ revoked: true });
      }
      if (url.endsWith("read-review")) {
        reads++;
        if (reads === 2) throw new Error("Access changed");
        return json(view);
      }
      throw new Error("Unexpected route");
    });
    vi.stubGlobal("fetch", fetcher);
    render(<CustomerEvidenceWorkspace />);
    await screen.findByRole("option", { name: /claude-code/ });
    expect(screen.queryByRole("option", { name: /metadata/ })).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Evidence connection"), {
      target: { value: connection.id },
    });
    const open = await screen.findByRole("button", { name: /Open review dddddddd/ });
    expect(screen.queryByText("Comparison evidence")).not.toBeInTheDocument();
    fireEvent.click(open);
    await screen.findByText("Comparison evidence");
    expect(screen.getByText("capture_gap")).toBeInTheDocument();
    expect(screen.getByText("succeeded · Test: pass")).toBeInTheDocument();
    expect(screen.getByText(/Pending human review · Unapproved/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Approve/ })).not.toBeInTheDocument();
    fireEvent.click(open);
    await screen.findByText("Access changed");
    expect(screen.queryByText("Comparison evidence")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry same review · 2 queries" }));
    await screen.findByText("Comparison evidence");
    const requests = fetcher.mock.calls.filter(([url]) => url.endsWith("read-review"));
    expect((requests[1]?.[1] as RequestInit).body).toEqual((requests[2]?.[1] as RequestInit).body);
    fireEvent.click(screen.getByRole("button", { name: /Revoke source bbbbbbbb/ }));
    await screen.findByText("Source revoked for this connection.");
    expect(screen.queryByText("Comparison evidence")).not.toBeInTheDocument();
    expect(
      fetcher.mock.calls.every(([url]) => !url.includes(connection.id) && !url.includes(pin.ref))
    ).toBe(true);
  });

  it("changing run selection discards previous inspection and consent", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(json(inspection)));
    render(<CustomerEvidenceSource connection={connection.id} onShared={vi.fn()} />);
    fireEvent.change(screen.getByLabelText("Recorded run ID"), { target: { value: run } });
    fireEvent.click(screen.getByRole("button", { name: "Inspect run · 1 query" }));
    await screen.findByRole("checkbox");
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.change(screen.getByLabelText("Recorded run ID"), {
      target: { value: "00000000-0000-4000-8000-000000000002" },
    });
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    expect(screen.queryByText(/Revision 6/)).not.toBeInTheDocument();
  });
});
