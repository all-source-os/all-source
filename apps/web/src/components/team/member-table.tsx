"use client";

import { Button, Card, CardContent } from "@allsource/ui";
import { useState } from "react";
import type { TeamMember } from "@/lib/api/client";

export function MemberTable({
  members,
  isLoading,
  canManage,
  currentUserId,
  onRemove,
  onUpdateRole,
}: {
  members: TeamMember[];
  isLoading: boolean;
  canManage: boolean;
  currentUserId?: string;
  onRemove: (userId: string) => Promise<void>;
  onUpdateRole: (userId: string, role: "admin" | "member") => Promise<void>;
}) {
  const [pending, setPending] = useState<string | null>(null);
  if (isLoading)
    return (
      <Card>
        <CardContent className="p-6" role="status">
          Loading members…
        </CardContent>
      </Card>
    );
  return (
    <Card>
      <CardContent className="overflow-x-auto p-0">
        <table className="w-full text-base">
          <caption className="sr-only">Current workspace members</caption>
          <thead>
            <tr className="border-b text-left">
              <th className="p-4">Member</th>
              <th className="p-4">Team role</th>
              <th className="p-4">Joined</th>
              {canManage && <th className="p-4">Actions</th>}
            </tr>
          </thead>
          <tbody>
            {members.map((member) => (
              <tr key={member.user_id} className="border-b last:border-0">
                <td className="p-4">
                  <p className="font-medium">
                    {member.name || member.email || member.user_id}
                    {member.user_id === currentUserId && " (you)"}
                  </p>
                  <p className="text-muted-foreground">{member.email}</p>
                </td>
                <td className="p-4">
                  {canManage ? (
                    <select
                      aria-label={`Role for ${member.name || member.email}`}
                      className="rounded-md border bg-background p-2"
                      value={member.role}
                      disabled={pending !== null}
                      onChange={async (event) => {
                        setPending(member.user_id);
                        try {
                          await onUpdateRole(
                            member.user_id,
                            event.target.value as "admin" | "member"
                          );
                        } finally {
                          setPending(null);
                        }
                      }}
                    >
                      <option value="member">Member</option>
                      <option value="admin">Admin</option>
                    </select>
                  ) : (
                    member.role
                  )}
                </td>
                <td className="p-4 text-muted-foreground">
                  {member.joined_at
                    ? new Date(member.joined_at).toLocaleDateString()
                    : "Not recorded"}
                </td>
                {canManage && (
                  <td className="p-4">
                    {member.user_id !== currentUserId && (
                      <Button
                        variant="outline"
                        disabled={pending !== null}
                        onClick={() => onRemove(member.user_id)}
                      >
                        Remove<span className="sr-only"> {member.name || member.email}</span>
                      </Button>
                    )}
                  </td>
                )}
              </tr>
            ))}
          </tbody>
        </table>
        {members.length === 0 && <p className="p-6">No members recorded.</p>}
      </CardContent>
    </Card>
  );
}
