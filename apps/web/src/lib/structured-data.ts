import { siteConfig } from "@/lib/config";
import type { EventSourcingPattern } from "@/lib/event-sourcing-patterns";
import type { Catalog } from "@/lib/pricing-catalog";
import type { ProductVertical } from "@/lib/product-verticals";

/**
 * Centralized JSON-LD builders.
 *
 * GEO/AEO note: every entity URL flows from `siteConfig.url` so the
 * Organization/WebSite/Article graph shares ONE canonical host. Mixing
 * apex and www splits the entity across answer engines and weakens citation.
 *
 * Stable `@id` anchors (`#organization`, `#website`) let nodes reference each
 * other instead of duplicating data, which is what crawlers resolve against.
 */

const ORG_ID = `${siteConfig.url}/#organization`;
const WEBSITE_ID = `${siteConfig.url}/#website`;
const FOUNDER_ID = `${siteConfig.url}/#founder`;

/** Public profiles that prove the entity is the same one across the web. */
const sameAs = [siteConfig.links.github, siteConfig.links.twitter];

export function organizationSchema() {
  return {
    "@context": "https://schema.org",
    "@type": "Organization",
    "@id": ORG_ID,
    name: siteConfig.productName,
    alternateName: [siteConfig.name, "all-source.xyz", "all-source-os"],
    url: siteConfig.url,
    logo: {
      "@type": "ImageObject",
      url: `${siteConfig.url}/logo.svg`,
    },
    description:
      "Purpose-built event store database for immutable streams, replay, projections, snapshots, schema governance, temporal queries, and durable consumers. Published Core reference results: 469K events/sec batch ingestion and 11.9us p99 indexed reads.",
    disambiguatingDescription:
      "Developer infrastructure published at all-source.xyz; unrelated to Esri ArcGIS AllSource, the all-source intelligence discipline, and other companies using AllSource or Allsource.",
    sameAs,
    founder: { "@id": FOUNDER_ID },
    parentOrganization: {
      "@type": "Organization",
      name: "Wolven Tech",
      url: "https://wolventech.com",
    },
    contactPoint: {
      "@type": "ContactPoint",
      email: siteConfig.links.email,
      contactType: "customer service",
    },
  };
}

export function founderSchema() {
  return {
    "@context": "https://schema.org",
    "@type": "Person",
    "@id": FOUNDER_ID,
    name: "Decebal Dobrica",
    url: "https://decebaldobrica.com",
    jobTitle: "Founder and product engineer",
    worksFor: {
      "@type": "Organization",
      name: "Wolven Tech",
      url: "https://wolventech.com",
    },
    knowsAbout: ["Event sourcing", "Rust", "AI agent memory", "Model Context Protocol"],
    sameAs: [
      "https://github.com/decebal",
      "https://www.linkedin.com/in/decebaldobrica",
      "https://x.com/ddonprogramming",
    ],
  };
}

export function websiteSchema() {
  return {
    "@context": "https://schema.org",
    "@type": "WebSite",
    "@id": WEBSITE_ID,
    name: siteConfig.productName,
    alternateName: siteConfig.name,
    url: siteConfig.url,
    description: siteConfig.description,
    inLanguage: "en",
    publisher: { "@id": ORG_ID },
  };
}

export type BreadcrumbItem = { name: string; path: string };

export function breadcrumbSchema(items: BreadcrumbItem[]) {
  return {
    "@context": "https://schema.org",
    "@type": "BreadcrumbList",
    itemListElement: items.map((item, i) => ({
      "@type": "ListItem",
      position: i + 1,
      name: item.name,
      item: `${siteConfig.url}${item.path}`,
    })),
  };
}

export type FaqItem = { question: string; answer: string };

/**
 * FAQPage builder. Comparison pages answer the obvious "is X better than
 * AllSource?" intent — emitting these as schema makes the answers eligible for
 * AI-answer-engine and rich-result citation. Kept here (not inlined per page)
 * so the FAQ shape stays consistent with the rest of the JSON-LD graph.
 */
export function faqPageSchema(items: readonly FaqItem[]) {
  return {
    "@context": "https://schema.org",
    "@type": "FAQPage",
    mainEntity: items.map((item) => ({
      "@type": "Question",
      name: item.question,
      acceptedAnswer: {
        "@type": "Answer",
        text: item.answer,
      },
    })),
  };
}

/**
 * Parses a live catalog display price into the numeric amount + ISO currency
 * schema.org expects.
 */
function parseDisplayPrice(price: string): { value: string; currency: string } | null {
  if (/^free$/i.test(price.trim())) return { value: "0", currency: "USD" };
  const match = price.trim().match(/^([$£€])\s*([\d.,]+)$/);
  if (!match) return null;
  const [, symbol, amount] = match;
  if (!symbol || !amount) return null;
  const currencyBySymbol: Record<string, string> = { $: "USD", "£": "GBP", "€": "EUR" };
  return { value: amount.replace(/,/g, ""), currency: currencyBySymbol[symbol] ?? "USD" };
}

/**
 * SoftwareApplication + Offer graph.
 *
 * GEO/AEO note: "what is X?" and "what does X cost?" are the two highest-intent
 * questions an answer engine fields about a developer tool, and both were
 * previously answerable only from prose. SoftwareApplication is the canonical
 * entity type for this product — without it, models infer the category
 * themselves, and the observed failure mode is landing on "in-memory cache" or
 * "logging tool" rather than "event store".
 *
 * Every paid offer flows from the live billing catalog used by the pricing
 * page. When the catalog is unavailable, offers are omitted rather than
 * publishing fallback prices that might disagree with checkout.
 */
export function softwareApplicationSchema(catalog?: Catalog | null) {
  const livePrices = new Map(catalog?.tiers.map((tier) => [tier.tier, tier.monthly]));
  const offers = catalog
    ? siteConfig.pricing
        // Self-Host and Enterprise are both excluded. Self-Host is not a
        // hosted offer; Enterprise has no numeric public price.
        .filter((tier) => !tier.isEnterprise && !tier.isSelfHost)
        .map((tier) => {
          const livePrice = livePrices.get(tier.tier)?.formatted;
          const parsed = livePrice ? parseDisplayPrice(livePrice) : null;
          if (!parsed) return null;
          return {
            "@type": "Offer",
            name: tier.name,
            price: parsed.value,
            priceCurrency: parsed.currency,
            url: tier.href.startsWith("http") ? tier.href : `${siteConfig.url}${tier.href}`,
            category: "SaaS",
            priceSpecification: {
              "@type": "UnitPriceSpecification",
              price: parsed.value,
              priceCurrency: parsed.currency,
              unitText: tier.period === "month" ? "MONTH" : tier.period,
            },
          };
        })
        .filter((offer) => offer !== null)
    : [];

  return {
    "@context": "https://schema.org",
    "@type": "SoftwareApplication",
    "@id": `${siteConfig.url}/#software`,
    name: siteConfig.productName,
    alternateName: siteConfig.name,
    url: siteConfig.url,
    applicationCategory: "DeveloperApplication",
    applicationSubCategory: "Event store database",
    operatingSystem: "Linux, macOS, Windows",
    description:
      "AllSource is a purpose-built event store database for event sourcing. Its Apache-2.0 Rust core records immutable streams with a CRC32-checked write-ahead log, Parquet persistence, snapshots, schema governance, temporal reconstruction, and durable consumers.",
    disambiguatingDescription:
      "The developer product at all-source.xyz, not Esri ArcGIS AllSource or the all-source intelligence discipline.",
    sameAs: [siteConfig.links.github, "https://crates.io/crates/allsource-core"],
    publisher: { "@id": ORG_ID },
    ...(offers.length > 0 && { offers }),
  };
}

export function productVerticalListSchema(verticals: readonly ProductVertical[]) {
  return {
    "@context": "https://schema.org",
    "@type": "ItemList",
    name: "AllSource Event Store product map",
    description: "Canonical map of AllSource Core, Prime, hosted services, and MCP connectors.",
    numberOfItems: verticals.length,
    itemListElement: verticals.map((vertical, index) => ({
      "@type": "ListItem",
      position: index + 1,
      url: `${siteConfig.url}${vertical.path}`,
      name: vertical.name,
      description: vertical.directAnswer,
    })),
  };
}

export function eventSourcingPatternListSchema(patterns: readonly EventSourcingPattern[]) {
  return {
    "@context": "https://schema.org",
    "@type": "ItemList",
    name: "Production event-sourcing patterns",
    description:
      "Implementation guides for aggregate streams, concurrency, projections, replay, schemas, temporal queries, tenancy, and durable consumers.",
    numberOfItems: patterns.length,
    itemListElement: patterns.map((pattern, index) => ({
      "@type": "ListItem",
      position: index + 1,
      url: `${siteConfig.url}/event-sourcing/patterns/${pattern.slug}`,
      name: pattern.title,
      description: pattern.directAnswer,
    })),
  };
}

export function techArticleSchema(pattern: EventSourcingPattern, wordCount: number) {
  const url = `${siteConfig.url}/event-sourcing/patterns/${pattern.slug}`;
  return {
    "@context": "https://schema.org",
    "@type": "TechArticle",
    headline: pattern.title,
    description: pattern.description,
    url,
    mainEntityOfPage: { "@type": "WebPage", "@id": url },
    datePublished: "2026-09-10",
    dateModified: "2026-09-10",
    inLanguage: "en",
    articleSection: "Event sourcing patterns",
    keywords: pattern.keywords,
    wordCount,
    proficiencyLevel: "Expert",
    about: [
      { "@type": "Thing", name: "Event sourcing" },
      { "@type": "Thing", name: "Event store database" },
      { "@type": "SoftwareApplication", "@id": `${siteConfig.url}/#software` },
    ],
    author: { "@id": FOUNDER_ID },
    publisher: { "@id": ORG_ID },
  };
}

export type BlogPostingInput = {
  title: string;
  description: string;
  slug: string;
  /** Absolute image URL (already resolved against siteConfig.url). */
  image: string;
  datePublished: string;
  dateModified?: string;
  author?: string;
  section?: string;
  keywords?: string[];
  wordCount?: number;
};

export function blogPostingSchema(post: BlogPostingInput) {
  const url = `${siteConfig.url}/blog/${post.slug}`;
  return {
    "@context": "https://schema.org",
    "@type": "BlogPosting",
    headline: post.title,
    description: post.description,
    image: post.image,
    datePublished: post.datePublished,
    dateModified: post.dateModified || post.datePublished,
    url,
    mainEntityOfPage: { "@type": "WebPage", "@id": url },
    inLanguage: "en",
    ...(post.section && { articleSection: post.section }),
    ...(post.keywords?.length && { keywords: post.keywords }),
    ...(typeof post.wordCount === "number" && { wordCount: post.wordCount }),
    author: {
      "@type": "Person",
      "@id": FOUNDER_ID,
      name: post.author || "Decebal Dobrica",
      url: "https://decebaldobrica.com",
    },
    publisher: { "@id": ORG_ID },
  };
}
