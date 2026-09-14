import type { Metadata } from "next";
import { constructMetadata } from "@/lib/utils";

export const metadata: Metadata = constructMetadata({
  title: "AllSource Prime: Graph, Vector, Temporal Recall",
  description:
    "See how Prime derives knowledge graphs, vector search, temporal recall, compressed context, and provenance from durable AllSource Core events.",
  canonical: "/platform/prime",
});

export default function Layout({ children }: { children: React.ReactNode }) {
  return children;
}
