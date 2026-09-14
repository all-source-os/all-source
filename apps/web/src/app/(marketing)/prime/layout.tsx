import type { Metadata } from "next";
import { constructMetadata } from "@/lib/utils";

export const metadata: Metadata = constructMetadata({
  title: "AllSource Prime: Agent Memory for MCP Clients",
  description:
    "Store knowledge-graph relationships, embeddings, provenance, and compressed context for Claude Desktop, Claude Code, and other MCP clients.",
  canonical: "/prime",
});

export default function Layout({ children }: { children: React.ReactNode }) {
  return children;
}
