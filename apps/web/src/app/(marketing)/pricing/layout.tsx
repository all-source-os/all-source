import type { Metadata } from "next";
import { constructMetadata } from "@/lib/utils";

export const metadata: Metadata = constructMetadata({
  title: "Pricing — pay for the events your agents write",
  description:
    "Self-host AllSource under Apache-2.0, or take a hosted plan from £18.99/month after a 14-day trial. Event quotas, retention and MCP access by tier.",
  canonical: "/pricing",
});

export default function Layout({ children }: { children: React.ReactNode }) {
  return children;
}
