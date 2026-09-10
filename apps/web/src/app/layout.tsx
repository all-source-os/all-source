import { Analytics } from "@vercel/analytics/next";
import type { Metadata, Viewport } from "next";
import { GeoReferralTracker } from "@/components/geo-referral-tracker";
import { GoogleAnalytics } from "@/components/google-analytics";
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
 * ADR — analytics choice for GEO layer 1 (prompt 024)
 *
 * The site had NO analytics of any kind before this. The constraint set was:
 * Vercel-hosted Next.js App Router; must expose the raw referrer and user
 * agent; must not force a cookie-banner rewrite.
 *
 * Chosen: **Vercel Web Analytics** (`@vercel/analytics`) for cookieless
 * site-wide traffic, a first-party `<GeoReferralTracker />` beacon for the GEO
 * event stream, and GA4 with denied-by-default storage for Search Console and
 * cross-product acquisition reporting.
 *
 * Why three measurement paths rather than one product:
 * - Vercel Web Analytics is cookieless and privacy-preserving, so it adds no
 *   consent-banner obligation the site did not already have. It is served from
 *   `/_vercel/insights/*` on our own origin, so it needs no third-party CSP
 *   entries, env var, or account setup beyond enabling it in the Vercel
 *   dashboard. GA4 uses narrowly scoped script and collection origins in
 *   `next.config.mjs`.
 * - It does NOT satisfy the second constraint. Vercel shows referrers in *its*
 *   dashboard; it gives us no programmatic access to the raw referrer and user
 *   agent, and it cannot join an arrival to a conversion inside AllSource.
 *   That is precisely what layer 1 is for, hence the beacon.
 * - GA4 supplies channel, landing-page, and Google Search Console reporting
 *   shared with the other Wolven Tech products. It runs with analytics and ad
 *   storage denied, strips query strings, and disables Google Signals.
 *   First-party GEO events remain authoritative for AI referrals because GA4
 *   may group them as Direct or Referral.
 *
 * Rejected:
 * - **PostHog** — the richest option, but it sets cookies by default and would
 *   pull the site into consent-banner work that is not this slice's job.
 * - **Plausible / Fathom** — cookieless and good, but a paid third-party
 *   script host (CSP widening) for aggregate numbers Vercel already gives us
 *   free, and still no raw referrer in our own pipeline.
 *
 * Env vars to set by hand: Vercel Web Analytics needs NONE (enable it in
 * Project Settings -> Analytics). GA4 uses `NEXT_PUBLIC_GA_MEASUREMENT_ID`,
 * with a canonical-host fallback for production. The referral route needs
 * `ALLSOURCE_API_KEY` (and optionally `ALLSOURCE_API_URL`) set in the Vercel
 * dashboard — server scope only, never `NEXT_PUBLIC_`. See
 * docs/runbooks/GEO_MEASUREMENT.md.
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
  themeColor: [
    { media: "(prefers-color-scheme: dark)", color: "#063A6C" },
    { media: "(prefers-color-scheme: light)", color: "#F7FBFF" },
  ],
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
        <ThemeProvider attribute="class" defaultTheme="dark" enableSystem={false}>
          {children}
          <TailwindIndicator />
          <GeoReferralTracker />
          {process.env.VERCEL ? <Analytics /> : null}
        </ThemeProvider>
      </body>
    </html>
  );
}
