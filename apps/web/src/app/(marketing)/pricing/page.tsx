import { Section } from "@allsource/ui";
import FAQ from "@/components/sections/faq";
import PricingSection from "@/components/sections/pricing";
import { siteConfig } from "@/lib/config";
import { fetchCatalog, indexByTier, resolveMonthly } from "@/lib/pricing-catalog";
import { softwareApplicationSchema } from "@/lib/structured-data";

// Render at request time: build workers cannot reach the runtime billing service.
// The control plane caches provider prices for five minutes, then refreshes in
// background while preserving a verified last-known-good snapshot for 14 days.
export const revalidate = 0;

// Comparison matrix rows. Each row maps a label to a per-tier cell, keyed by the
// stable public tier id from siteConfig.pricing. Self-Host is intentionally NOT
// a column — the product has no free plan, so the matrix starts at Indie (the
// run-it-yourself Apache-2.0 path lives in the "Why no free plan?" FAQ instead).
const matrixRows: { label: string; cells: Record<string, string> }[] = [
  {
    label: "Events / mo",
    cells: {
      indie: "500K + 50K x402",
      studio: "5M + 500K x402",
      scale: "50M + 5M x402",
      enterprise: "Negotiated",
    },
  },
  {
    label: "Retention",
    cells: {
      indie: "14 days",
      studio: "90 days",
      scale: "365 days",
      enterprise: "Unlimited",
    },
  },
  {
    label: "MCP",
    cells: {
      indie: "read",
      studio: "read + write",
      scale: "read + write + dedicated",
      enterprise: "Dedicated cluster",
    },
  },
  {
    label: "Streams",
    cells: {
      indie: "3",
      studio: "Unlimited",
      scale: "Unlimited",
      enterprise: "Unlimited",
    },
  },
  {
    label: "Support",
    cells: {
      indie: "Email 48h",
      studio: "Email 24h + Discord",
      scale: "Priority + Slack",
      enterprise: "24/7 + dedicated SE",
    },
  },
];

export default async function PricingPage() {
  // Self-Host is excluded everywhere on /pricing — no free plan is advertised, so
  // the cards, matrix, and price row all start at Indie. (siteConfig.pricing still
  // carries Self-Host for the authenticated dashboard's legacy-tenant rendering.)
  const tiers = siteConfig.pricing.filter((p) => !p.isSelfHost);
  const catalog = await fetchCatalog();
  const prices = indexByTier(catalog);
  const productJsonLd = softwareApplicationSchema(catalog);

  return (
    <>
      <script
        type="application/ld+json"
        // biome-ignore lint/security/noDangerouslySetInnerHtml: JSON-LD structured data requires dangerouslySetInnerHTML
        dangerouslySetInnerHTML={{
          __html: JSON.stringify(productJsonLd).replace(/</g, "\\u003c"),
        }}
      />
      <div className="mx-auto w-full max-w-screen-xl px-4 lg:px-8">
        {/* Above the fold: promise, toggle, cards, enterprise strip, x402 lines. */}
        <PricingSection catalog={catalog} headingLevel={1} title="AllSource hosted pricing" />

        {/* Below the fold: comparison matrix, Indie → Enterprise (no free plan). */}
        <Section title="Compare tiers" subtitle="Everything, side by side">
          <div className="overflow-x-auto">
            <table className="w-full min-w-[720px] border-collapse text-sm">
              <thead>
                <tr className="border-b border-border">
                  <th className="px-4 py-3 text-left font-medium text-muted-foreground">Feature</th>
                  {tiers.map((tier) => (
                    <th key={tier.tier} className={cellHeaderClass(tier.isPopular)} scope="col">
                      {tier.name}
                      {tier.isPopular && (
                        <span className="ml-1 align-middle text-[10px] font-semibold text-primary">
                          Popular
                        </span>
                      )}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {matrixRows.map((row) => (
                  <tr key={row.label} className="border-b border-border">
                    <td className="px-4 py-3 text-left font-medium text-foreground">{row.label}</td>
                    {tiers.map((tier) => (
                      <td key={tier.tier} className={cellBodyClass(tier.isPopular)}>
                        {row.cells[tier.tier] ?? "—"}
                      </td>
                    ))}
                  </tr>
                ))}
                {/* Price row */}
                <tr>
                  <td className="px-4 py-3 text-left font-medium text-foreground">Price</td>
                  {tiers.map((tier) => {
                    // Live LemonSqueezy monthly price; paid tiers show a dash
                    // (not a config number) when no live/cached price.
                    const price = resolveMonthly(prices[tier.tier], tier.price);
                    return (
                      <td key={tier.tier} className={cellBodyClass(tier.isPopular)}>
                        {price}
                        {/^[£$€]/.test(price) && <span className="text-muted-foreground">/mo</span>}
                      </td>
                    );
                  })}
                </tr>
              </tbody>
            </table>
          </div>
        </Section>

        {/* Pricing-specific FAQ + its own FAQPage JSON-LD. Deliberately NOT the
          site-wide set: emitting the homepage's FAQPage here too would put the
          same graph on two URLs and split the citation signal. */}
        <FAQ
          items={siteConfig.pricingFaqs}
          title="Pricing FAQ"
          subtitle="What it costs, and what happens when you outgrow a tier"
        />
      </div>
    </>
  );
}

function cellHeaderClass(isPopular: boolean) {
  return [
    "px-4 py-3 text-center font-semibold",
    isPopular ? "text-primary" : "text-foreground",
  ].join(" ");
}

function cellBodyClass(isPopular: boolean) {
  return [
    "px-4 py-3 text-center",
    isPopular ? "bg-primary/[0.04] text-foreground" : "text-muted-foreground",
  ].join(" ");
}
