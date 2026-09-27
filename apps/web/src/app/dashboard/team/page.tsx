"use client";

import { Button, Card, CardContent } from "@allsource/ui";
import { Plus, Users } from "lucide-react";
import { useState } from "react";
import { LoadError } from "@/components/dashboard/load-error";
import { AgentKeysSection } from "@/components/team/agent-keys-section";
import { InviteMemberDialog } from "@/components/team/invite-member-dialog";
import { JoinWorkspace } from "@/components/team/join-workspace";
import { MemberTable } from "@/components/team/member-table";
import { FadeIn } from "@/components/ui/fade-in";
import { useTeamMembers } from "@/hooks/use-team-members";

export default function TeamPage() {
  const {
    members,
    seatsUsed,
    canManage,
    currentUserId,
    isLoading,
    error,
    inviteMember,
    removeMember,
    updateRole,
    refresh,
  } = useTeamMembers();
  const [showInviteDialog, setShowInviteDialog] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState<string | null>(null);
  const [actionError, setActionError] = useState("");
  const [removing, setRemoving] = useState(false);

  const handleInvite = async (email: string, role: "admin" | "member") => {
    return inviteMember({ email, role });
  };

  const handleRemove = async (userId: string) => {
    if (confirmRemove !== userId) {
      setConfirmRemove(userId);
      return;
    }

    setActionError("");
    setRemoving(true);
    try {
      await removeMember(userId);
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "Could not remove member.");
    }
    setRemoving(false);
    setConfirmRemove(null);
  };

  const handleUpdateRole = async (userId: string, role: string) => {
    setActionError("");
    try {
      await updateRole(userId, role);
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "Could not change role.");
    }
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <FadeIn delay={0.1} inView>
        <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h1 className="text-2xl font-bold tracking-tight md:text-3xl">Team</h1>
            <p className="mt-1 text-muted-foreground">
              Manage workspace membership, invitations, and agent keys
            </p>
          </div>
          {canManage && (
            <Button onClick={() => setShowInviteDialog(true)}>
              <Plus className="mr-1.5 h-4 w-4" />
              Invite Member
            </Button>
          )}
        </div>
      </FadeIn>

      {/* Seat usage */}
      <FadeIn delay={0.2} inView>
        <Card className="border-primary/20 bg-primary/5">
          <CardContent className="flex items-start gap-4 p-4">
            <Users className="h-5 w-5 shrink-0 text-primary" />
            <div className="flex-1">
              <h2 className="text-xl font-semibold">Workspace members</h2>
              <p className="text-base text-muted-foreground">
                {isLoading || error
                  ? "Membership count unavailable."
                  : `${seatsUsed} ${seatsUsed === 1 ? "member" : "members"}.`}{" "}
                Team members share this workspace&apos;s event and query quotas.
              </p>
            </div>
          </CardContent>
        </Card>
      </FadeIn>

      {/* Members table */}
      {actionError && (
        <p role="alert" className="text-base text-destructive">
          {actionError}
        </p>
      )}
      <FadeIn delay={0.3} inView>
        {error ? (
          <LoadError title="Team members could not be loaded" message={error} onRetry={refresh} />
        ) : (
          <MemberTable
            members={members}
            isLoading={isLoading}
            canManage={canManage}
            currentUserId={currentUserId}
            onRemove={handleRemove}
            onUpdateRole={handleUpdateRole}
          />
        )}
      </FadeIn>

      <JoinWorkspace />

      {/* Agent Keys */}
      <FadeIn delay={0.4} inView>
        <AgentKeysSection />
      </FadeIn>

      {/* Invite dialog */}
      <InviteMemberDialog
        open={showInviteDialog}
        onClose={() => setShowInviteDialog(false)}
        onInvite={handleInvite}
      />

      {/* Remove confirmation */}
      {confirmRemove && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <button
            type="button"
            className="absolute inset-0 bg-background/80 backdrop-blur-sm"
            onClick={() => setConfirmRemove(null)}
            aria-label="Close confirmation dialog"
          />
          <Card className="relative z-10 w-full max-w-sm mx-4">
            <CardContent className="p-6">
              <Users className="mx-auto mb-4 h-12 w-12 text-destructive" />
              <h3 className="mb-2 text-center text-lg font-semibold">Remove Team Member?</h3>
              <p className="mb-6 text-center text-sm text-muted-foreground">
                Remove this person from the team. To restore membership, create a new invitation.
                Revoke any issued API keys separately.
              </p>
              <div className="flex gap-2">
                <Button variant="outline" className="flex-1" onClick={() => setConfirmRemove(null)}>
                  Cancel
                </Button>
                <Button
                  variant="destructive"
                  className="flex-1"
                  onClick={() => handleRemove(confirmRemove)}
                  disabled={removing}
                >
                  Remove Member
                </Button>
              </div>
            </CardContent>
          </Card>
        </div>
      )}
    </div>
  );
}
