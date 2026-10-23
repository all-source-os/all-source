"use client";

import useSWR from "swr";
import { type AuditLogParams, apiClient } from "@/lib/api/client";

export function useAuditLogs(params?: AuditLogParams) {
  const key = params
    ? `/api/tenant/audit-logs?${JSON.stringify(params)}`
    : "/api/tenant/audit-logs";

  const { data, error, isLoading, mutate } = useSWR(
    key,
    async () => {
      const response = await apiClient.listAuditLogs(params);
      if (response.error) throw new Error(response.error.message);
      return response.data;
    },
    { revalidateOnFocus: false }
  );

  return {
    entries: data?.entries || [],
    retentionDays: data?.retention_days ?? null,
    actions: data?.actions || [],
    isLoading,
    error: error?.message,
    refresh: mutate,
  };
}
