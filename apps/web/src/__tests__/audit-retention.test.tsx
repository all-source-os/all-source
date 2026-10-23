import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AuditLogPage from "@/app/dashboard/settings/audit-log/page";

const auditState = vi.hoisted(() => ({ retentionDays: -1 as number | null }));

vi.mock("@/hooks/use-audit-logs", () => ({
  useAuditLogs: () => ({
    entries: [],
    retentionDays: auditState.retentionDays,
    actions: [],
    isLoading: false,
    error: undefined,
    refresh: vi.fn(),
  }),
}));

beforeEach(() => {
  auditState.retentionDays = -1;
});

describe("audit history presentation", () => {
  it("shows available history for the no-plan-retention sentinel", () => {
    render(<AuditLogPage />);

    expect(screen.getByText("Available audit history")).toBeInTheDocument();
    expect(screen.queryByText(/-1.day|last -1/)).not.toBeInTheDocument();
    expect(screen.queryByText(/90-day/)).not.toBeInTheDocument();
  });

  it("does not invent a retention limit before the server responds", () => {
    auditState.retentionDays = null;
    render(<AuditLogPage />);

    expect(screen.getByText("Audit history")).toBeInTheDocument();
    expect(screen.queryByText(/\d+-day retention/)).not.toBeInTheDocument();
  });
});
