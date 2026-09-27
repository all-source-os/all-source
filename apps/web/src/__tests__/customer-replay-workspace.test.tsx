import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { CustomerEvidenceWorkspace } from "@/components/agent-reviews/customer-evidence-workspace";
import { CustomerReplayReport } from "@/components/agent-reviews/customer-replay-report";
import { CustomerReplayWorkspace } from "@/components/agent-reviews/customer-replay-workspace";
import type { ReplayReview } from "@/lib/customer-replay-client";

const connection = {
  id: "a".repeat(32),
  client_id: "claude-code",
  expires_at: 2_000_000_000,
  status: "active",
};
const pending: ReplayReview = {
  id: "b".repeat(32),
  version: 1,
  digest: "c".repeat(64),
  expires_at: connection.expires_at,
  state: "pending",
  effective_state: "pending",
  replay_operation_id: "synthetic-operation",
  action: { projection_name: "event-count", history: "retained_at_dispatch", live_catchup: true },
  evidence: {
    sample_sha256: "d".repeat(64),
    catalog_sha256: "e".repeat(64),
    analysis: {
      projection_name: "event-count",
      projection_status: "ready",
      analyzed_at: "2026-09-27T00:00:00Z",
      sampled_events: 9,
      sampled_entity_count: 3,
      current_entity_count: 1,
      reported_total_events: null,
      analysis_scope: null,
      unknowns: [],
    },
  },
};
afterEach(() => {
  vi.unstubAllGlobals();
});
it("source revocation clears an open replay result across workspace panels", async () => {
  let revoked = false;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      let data: unknown;
      if (url.endsWith("/list"))
        data = { connections: [{ ...connection, consent: { version: "review-replay-v3" } }] };
      else if (url.endsWith("/revoke-source")) {
        revoked = true;
        data = { revoked: true };
      } else if (url.includes("/connections/workspace"))
        data = {
          connection_status: "active",
          reviews: [],
          sources: [
            {
              source: {
                ref: "source-example-id",
                kind: "replay_analysis",
                revision: 1,
                sha256: "f".repeat(64),
              },
              status: revoked ? "revoked" : "saved",
              projection_name: "event-count",
              expires_at: connection.expires_at,
            },
          ],
        };
      else if (url.endsWith("/workspace")) data = { reviews: [pending] };
      else if (url.endsWith("/read")) data = pending;
      else throw new Error("Unexpected request");
      return Response.json({ data });
    })
  );
  render(<CustomerEvidenceWorkspace replayEnabled />);
  await screen.findByRole("option", { name: /claude-code/ });
  fireEvent.change(screen.getByRole("combobox", { name: "Evidence connection" }), {
    target: { value: connection.id },
  });
  fireEvent.click(await screen.findByRole("button", { name: /Open replay review/ }));
  await screen.findByRole("heading", { name: "Projection rebuild review" });
  fireEvent.click(screen.getByRole("button", { name: "Revoke source source-e" }));
  await waitFor(() =>
    expect(
      screen.queryByRole("heading", { name: "Projection rebuild review" })
    ).not.toBeInTheDocument()
  );
  expect(revoked).toBe(true);
});
it("requires explicit versioned decision and preserves the same request across an interrupted reply", async () => {
  const decisions: Record<string, unknown>[] = [];
  const fetcher = vi.fn(async (url: string, init: RequestInit) => {
    const input = JSON.parse(init.body as string).input;
    if (url.endsWith("/workspace")) return Response.json({ data: { reviews: [pending] } });
    if (url.endsWith("/read")) return Response.json({ data: pending });
    if (url.endsWith("/approve")) {
      decisions.push(input);
      if (decisions.length === 1) throw new Error("interrupted");
      return Response.json({
        data: {
          ...pending,
          state: "approved",
          effective_state: "approved",
          replay: { status: "unknown" },
        },
      });
    }
    throw new Error("Unexpected request");
  });
  vi.stubGlobal("fetch", fetcher);
  render(<CustomerReplayWorkspace connection={connection} />);
  fireEvent.click(await screen.findByRole("button", { name: /Open replay review/ }));
  await screen.findByRole("button", { name: "Approve version 1 and rebuild" });
  expect(decisions).toHaveLength(0);
  expect(screen.getByText(/A reported total does not prove complete history/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Approve version 1 and rebuild" }));
  await screen.findByRole("alert");
  fireEvent.click(screen.getByRole("button", { name: /Open replay review/ }));
  fireEvent.click(await screen.findByRole("button", { name: "Approve version 1 and rebuild" }));
  await screen.findByText("Execution: unknown");
  expect(decisions).toHaveLength(2);
  expect(decisions[1]).toEqual(decisions[0]);
  expect(decisions[0]).toMatchObject({ id: pending.id, digest: pending.digest, version: 1 });
  expect(screen.queryByRole("button", { name: /Approve version/ })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Refresh replay reviews" }));
  await waitFor(() => expect(screen.queryByText("Execution: unknown")).not.toBeInTheDocument());
  expect(decisions).toHaveLength(2);
});
it.each(["superseded", "expired", "unavailable"])("%s review cannot approve", (state) => {
  render(
    <CustomerReplayReport
      review={{ ...pending, effective_state: state }}
      busy={false}
      onDecision={vi.fn()}
    />
  );
  expect(screen.queryByRole("button", { name: /Approve/ })).not.toBeInTheDocument();
});
it("approved receipt with absent dispatch exposes explicit same-operation recovery", () => {
  const decide = vi.fn();
  render(
    <CustomerReplayReport
      review={{
        ...pending,
        state: "approved",
        effective_state: "approved",
        replay: { status: "not_started" },
      }}
      busy={false}
      onDecision={decide}
    />
  );
  expect(decide).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Resume approved replay" }));
  expect(decide).toHaveBeenCalledWith("approve");
});
