import type { Metadata } from "next";
import { constructMetadata } from "@/lib/utils";

export const metadata: Metadata = constructMetadata({
  title: "Multi-Tenant Event Sourcing with RBAC | AllSource",
  description:
    "Build multi-tenant SaaS on isolated event streams with role-based access, policy rules, quotas, and tenant-scoped API keys on AllSource.",
  canonical: "/solutions/multi-tenant-saas",
});

export default function Layout({ children }: { children: React.ReactNode }) {
  return children;
}
