import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { InstallPage } from "@/components/install/install-page";
import { siteConfig } from "@/lib/config";
import { getIntegration, integrations } from "@/lib/integrations";
import { clampDescription, constructMetadata } from "@/lib/utils";

// One route, every per-tool install page. Adding a tool to lib/integrations.ts
// produces a new statically-generated page here — no new file required.

export function generateStaticParams() {
  return integrations.map((i) => ({ slug: i.slug }));
}

// Reject any slug not in the data module so we never serve a half-rendered page.
export const dynamicParams = false;

export async function generateMetadata({
  params,
}: {
  params: Promise<{ slug: string }>;
}): Promise<Metadata> {
  const { slug } = await params;
  const integration = getIntegration(slug);
  if (!integration) {
    return constructMetadata({ title: "Integration not found" });
  }
  return constructMetadata({
    title: `${integration.name} + ${siteConfig.name} Prime`,
    description: clampDescription(
      `${integration.blurb} Mint a hosted ${siteConfig.name} key or run Prime locally over stdio.`
    ),
    canonical: `/install/${integration.slug}`,
  });
}

export default async function IntegrationInstallPage({
  params,
}: {
  params: Promise<{ slug: string }>;
}) {
  const { slug } = await params;
  const integration = getIntegration(slug);
  if (!integration) {
    notFound();
  }
  return <InstallPage integration={integration} />;
}
