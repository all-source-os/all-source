import { Suspense } from "react";
import { siteConfig } from "@/lib/config";
import { constructMetadata } from "@/lib/utils";
import { ConnectClient } from "./connect-client";

export const metadata = constructMetadata({
  title: "Connect to Claude Desktop",
  description: `Mint an API key and copy a ready-to-paste config to wire ${siteConfig.name} Prime into Claude Desktop.`,
  canonical: "/connect",
});

export default function ConnectPage() {
  return (
    <div className="mx-auto w-full max-w-screen-md px-4 lg:px-8 py-24">
      <h1 className="mb-2 text-3xl font-bold text-foreground sm:text-4xl">
        Connect AllSource Prime to Claude Desktop
      </h1>
      <p className="text-lg text-muted-foreground">
        Create one scoped API key, then copy a ready-to-paste Claude Desktop configuration. Prime
        can write durable memories to your AllSource tenant and read them in later sessions.
      </p>
      {/* Suspense boundary is required by Next.js because ConnectClient
          reads URL search params (`source`, `key_name`, `return`) — see
          bead t-baff for the deep-link param contract. */}
      <Suspense
        fallback={<div className="mt-10 h-32 animate-pulse rounded-xl border bg-muted/20" />}
      >
        <ConnectClient />
      </Suspense>
    </div>
  );
}
