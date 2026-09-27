"use client";

import useSWR, { mutate } from "swr";
import { apiClient, type InviteMemberRequest } from "@/lib/api/client";

const fetcher = async () => {
  const response = await apiClient.listTeamMembers();
  if (response.error) throw new Error(response.error.message);
  return response.data;
};

export function useTeamMembers() {
  const { data, error, isLoading } = useSWR("/api/team/members", fetcher, {
    revalidateOnFocus: false,
  });

  const inviteMember = async (request: InviteMemberRequest) => {
    const response = await apiClient.inviteTeamMember(request);
    if (response.error) throw new Error(response.error.message);
    if (!response.data?.token) throw new Error("Invitation was not created. Try again.");
    mutate("/api/team/members");
    return response.data;
  };

  const removeMember = async (userId: string) => {
    const response = await apiClient.removeTeamMember(userId);
    if (response.error) throw new Error(response.error.message);
    mutate("/api/team/members");
  };

  const updateRole = async (userId: string, role: string) => {
    const response = await apiClient.updateTeamMemberRole(userId, role);
    if (response.error) throw new Error(response.error.message);
    mutate("/api/team/members");
    return response.data;
  };

  return {
    members: data?.members || [],
    seatLimit: data?.seat_limit ?? null,
    seatsUsed: data?.seats_used || 0,
    canManage: data?.can_manage === true,
    currentUserId: data?.current_user_id,
    isLoading,
    error: error?.message,
    inviteMember,
    removeMember,
    updateRole,
    refresh: () => mutate("/api/team/members"),
  };
}
