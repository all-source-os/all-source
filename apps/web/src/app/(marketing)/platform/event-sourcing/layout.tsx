import type { Metadata } from "next";
import { breadcrumbSchema } from "@/lib/structured-data";
import { constructMetadata } from "@/lib/utils";

export const metadata: Metadata = constructMetadata({
  title: "Event Store Database for Event Sourcing",
  description:
    "Purpose-built event store database with immutable streams, WAL and Parquet durability, snapshots, schema governance, replay, and temporal queries.",
  canonical: "/platform/event-sourcing",
});

// Emitted from the layout because page.tsx is a client component and cannot
// carry a metadata or schema export.
export default function Layout({ children }: { children: React.ReactNode }) {
  return (
    <>
      <script
        type="application/ld+json"
        // biome-ignore lint/security/noDangerouslySetInnerHtml: JSON-LD structured data requires dangerouslySetInnerHTML
        dangerouslySetInnerHTML={{
          __html: JSON.stringify(
            breadcrumbSchema([
              { name: "Home", path: "/" },
              { name: "Event sourcing", path: "/platform/event-sourcing" },
            ])
          ),
        }}
      />
      {children}
    </>
  );
}
