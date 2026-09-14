import type { Metadata } from "next";
import { constructMetadata } from "@/lib/utils";

export const metadata: Metadata = constructMetadata({
  title: "Agent Memory: Five Approaches Compared",
  description:
    "Compare platform memory, RAG, files, databases, and event-sourced memory by use case and trade-off, with primary sources and a local restart proof.",
  canonical: "/compare/agent-memory",
});

export default function CompareLayout({ children }: { children: React.ReactNode }) {
  return children;
}
