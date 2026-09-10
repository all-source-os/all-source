import type { Metadata } from "next";
import { constructMetadata } from "@/lib/utils";

export const metadata: Metadata = constructMetadata({
  title: "Event Store Database for Event Sourcing",
  description:
    "Purpose-built event store database with immutable streams, WAL and Parquet durability, snapshots, schema governance, replay, and temporal queries.",
  canonical: "/platform/event-sourcing",
});

export default function Layout({ children }: { children: React.ReactNode }) {
  return children;
}
