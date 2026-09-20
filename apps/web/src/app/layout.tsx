import type { Metadata, Viewport } from "next";
import { GeoReferralTracker } from "@/components/geo-referral-tracker";
import { GoogleAnalytics } from "@/components/google-analytics";
import { PostHogAnalytics } from "@/components/posthog-analytics";
import { TailwindIndicator } from "@/components/tailwind-indicator";
import { ThemeProvider } from "@/components/theme-provider";
import {
  founderSchema,
  organizationSchema,
  softwareApplicationSchema,
  websiteSchema,
} from "@/lib/structured-data";
import { cn, constructMetadata } from "@/lib/utils";
import "./globals.css";

/*
 * ADR — production analytics after Fly.io migration
 *
 * PostHog Cloud EU measures acquisition and product UX. It runs cookieless,
 * with autocapture, session replay, and person profiles disabled. URLs are
 * reduced to origin + pathname before capture.
 *
 * First-party AllSource events remain authoritative for durable product
 * outcomes and AI-referral attribution. GA4 remains denied-by-default for
 * Search Console and cross-product acquisition reporting. PostHog never
 * receives event payloads, entity IDs, API keys, emails, or free text.
 *
 * Fly builds require `NEXT_PUBLIC_POSTHOG_KEY` and
 * `NEXT_PUBLIC_POSTHOG_HOST`. Referral routes require server-only
 * `ALLSOURCE_API_KEY` and optional `ALLSOURCE_API_URL`.
 */

export const metadata: Metadata = constructMetadata({
  title: "Event Store Database for Event Sourcing | AllSource",
  description:
    "Purpose-built event store database for immutable streams, replay, projections, snapshots, schemas, temporal queries, and durable consumers.",
  canonical: "/",
  verification: {
    google: "BbHb4BnJ4QZYJmCEPpGhADhmJdSq6eGYRtAteMyjYwU",
  },
});

export const viewport: Viewport = {
  colorScheme: "dark",
  themeColor: "#0E1A2A",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <script
          type="application/ld+json"
          // biome-ignore lint/security/noDangerouslySetInnerHtml: JSON-LD structured data requires dangerouslySetInnerHTML
          dangerouslySetInnerHTML={{ __html: JSON.stringify(organizationSchema()) }}
        />
        <script
          type="application/ld+json"
          // biome-ignore lint/security/noDangerouslySetInnerHtml: JSON-LD structured data requires dangerouslySetInnerHTML
          dangerouslySetInnerHTML={{ __html: JSON.stringify(founderSchema()) }}
        />
        <script
          type="application/ld+json"
          // biome-ignore lint/security/noDangerouslySetInnerHtml: JSON-LD structured data requires dangerouslySetInnerHTML
          dangerouslySetInnerHTML={{ __html: JSON.stringify(websiteSchema()) }}
        />
        <script
          type="application/ld+json"
          // biome-ignore lint/security/noDangerouslySetInnerHtml: JSON-LD structured data requires dangerouslySetInnerHTML
          dangerouslySetInnerHTML={{ __html: JSON.stringify(softwareApplicationSchema()) }}
        />
      </head>
      <body className={cn("min-h-screen bg-background antialiased w-full mx-auto scroll-smooth")}>
        <GoogleAnalytics />
        <PostHogAnalytics />
        <ThemeProvider attribute="class" defaultTheme="dark" enableSystem={false}>
          {children}
          <TailwindIndicator />
          <GeoReferralTracker />
        </ThemeProvider>
      </body>
    </html>
  );
}
