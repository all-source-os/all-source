import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { InviteMemberDialog } from "@/components/team/invite-member-dialog";
import { JoinWorkspace } from "@/components/team/join-workspace";
import { MemberTable } from "@/components/team/member-table";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
it("displays a durable invite code, no claimed email delivery or unsupported viewer role", async () => {
  const onInvite = vi.fn().mockResolvedValue({
    token: "synthetic-code",
    email: "member@example.test",
    role: "member",
    expires_at: "2026-10-01T12:00:00Z",
  });
  render(<InviteMemberDialog open onClose={() => {}} onInvite={onInvite} />);
  fireEvent.change(screen.getByLabelText("Email address"), {
    target: { value: "member@example.test" },
  });
  expect(screen.queryByRole("option", { name: /Viewer/ })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Create invitation" }));
  await screen.findByText("Invitation ready");
  expect(screen.getByLabelText("Invitation code")).toHaveValue("synthetic-code");
  expect(screen.getByText(/No email has been sent/)).toBeInTheDocument();
  expect(onInvite).toHaveBeenCalledWith("member@example.test", "member");
});
it("hides mutation controls for regular members and prevents self removal", () => {
  const props = {
    members: [
      {
        user_id: "one",
        name: "Member",
        email: "one@example.test",
        role: "member" as const,
        joined_at: "2026-09-27T12:00:00Z",
      },
    ],
    isLoading: false,
    currentUserId: "one",
    onRemove: vi.fn(),
    onUpdateRole: vi.fn(),
  };
  const view = render(<MemberTable {...props} canManage={false} />);
  expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /Remove/ })).not.toBeInTheDocument();
  view.rerender(<MemberTable {...props} canManage />);
  expect(screen.getByRole("combobox")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /Remove/ })).not.toBeInTheDocument();
});
it("submits invitation only in request body and shows join errors", async () => {
  const fetcher = vi
    .fn()
    .mockResolvedValue(
      Response.json({ error: { message: "Verified email required" } }, { status: 403 })
    );
  vi.stubGlobal("fetch", fetcher);
  render(<JoinWorkspace />);
  const code = "a".repeat(32);
  fireEvent.change(screen.getByLabelText("Invitation code"), { target: { value: code } });
  fireEvent.click(screen.getByRole("button", { name: "Join workspace" }));
  await waitFor(() =>
    expect(screen.getByRole("alert")).toHaveTextContent("Verified email required")
  );
  expect(fetcher).toHaveBeenCalledWith(
    "/api/team/join",
    expect.objectContaining({ body: JSON.stringify({ token: code }) })
  );
});
