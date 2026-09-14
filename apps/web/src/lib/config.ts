export const BLUR_FADE_DELAY = 0.15;

export const platformNavigationGroups = [
  {
    id: "core",
    label: "Core",
    items: [
      {
        href: "/what-is-allsource",
        label: "Platform overview",
        description: "Core, Query Service, Prime, hosted services, and MCP.",
      },
      {
        href: "/platform/event-sourcing",
        label: "Core event store",
        description: "Immutable streams, replay, snapshots, and schemas.",
      },
      {
        href: "/platform/query-service",
        label: "Query Service",
        description: "Tenant-scoped HTTP, realtime, analytics, and projections.",
      },
    ],
  },
  {
    id: "build-with-it",
    label: "Build with it",
    items: [
      {
        href: "/prime",
        label: "Prime agent memory",
        description: "Graph, vector, and temporal memory over Core history.",
      },
      {
        href: "/event-sourcing/patterns",
        label: "Production patterns",
        description: "Ten practical event-sourcing implementation guides.",
      },
      {
        href: "/examples",
        label: "Live demo",
        description: "Inspect timelines, replay, projections, and MCP access.",
      },
    ],
  },
] as const;

export const primaryNavigation = [
  { href: "/use-cases", label: "Use cases" },
  { href: "/docs", label: "Docs" },
  { href: "/pricing", label: "Pricing" },
] as const;

export const siteConfig = {
  name: "AllSource",
  productName: "AllSource Event Store",
  description:
    "AllSource is a purpose-built event store database for immutable streams, replay, projections, temporal queries, and durable consumers.",
  // Single source of truth for the headline performance numbers. Both the
  // homepage demo chrome and the below-the-fold stat strip read from here so
  // the values can never desync — and so they can be rendered at their FINAL
  // value on first paint (no animate-from-zero "0K"/"0μs" flash).
  //   display — the exact string to paint (already formatted).
  //   numeric/suffix — used only by the optional count-up animation, which
  //                    must START from `display`, never from 0.
  stats: [
    { display: "469K", numeric: 469, suffix: "K", label: "events/sec" },
    { display: "11.9μs", numeric: 11.9, suffix: "μs", label: "p99 indexed read" },
    { display: "55", numeric: 55, suffix: "", label: "default MCP tools" },
    { display: "129MB", numeric: 129, suffix: "MB", label: "footprint" },
  ],
  // Published Core indexed-read benchmark shown by the homepage demo. This is
  // not an end-to-end vector or graph recall measurement.
  referenceReadLatency: "11.9μs p99",
  // Falls back to the production URL — not localhost — so a missing
  // NEXT_PUBLIC_APP_URL on Vercel (or any other consumer of this config) can't
  // leak `http://localhost:3000` into og:url / canonical / share-sheet URLs.
  // Override in `.env.local` if you want localhost in dev OG tags.
  url: process.env.NEXT_PUBLIC_APP_URL || "https://www.all-source.xyz",
  keywords: [
    "Event Store Database",
    "Event Sourcing",
    "Event Store",
    "Event Sourcing Database",
    "Immutable Event Streams",
    "Event Store Projections",
    "Event Store Snapshots",
    "Durable Subscriptions",
    "Temporal Data",
    "Stream Processing",
    "Real-time Analytics",
    "CQRS",
    "Data Intelligence",
    "Durable Agent Memory",
    "Event Replay",
    "Data Provenance",
    "AllSource Event Store",
    "AllSource Core",
    "AllSource Prime",
  ],
  // Bare X/Twitter handle (no @) — single source for cards, JSON-LD, and the
  // blog author chip. `links.twitter` is the profile URL built from it.
  twitterHandle: "ddonprogramming",
  links: {
    email: "hello@all-source.xyz",
    twitter: "https://x.com/ddonprogramming",
    discord: "https://github.com/all-source-os/all-source/discussions",
    github: "https://github.com/all-source-os/all-source",
    instagram: "https://instagram.com/allsourcedev",
  },
  header: [
    {
      trigger: "Platform",
      content: {
        main: {
          icon: "logo" as const,
          title: "Platform overview",
          description: "Core, Query Service, Prime, hosted services, and MCP.",
          href: "/what-is-allsource",
        },
        items: [
          {
            href: "/platform/event-sourcing",
            title: "Core event store",
            description: "Immutable streams, replay, snapshots, and schemas.",
          },
          {
            href: "/platform/query-service",
            title: "Query Service",
            description: "Tenant-scoped HTTP, realtime, analytics, and projections.",
          },
          {
            href: "/prime",
            title: "Prime agent memory",
            description: "Graph, vector, and temporal memory over Core history.",
          },
          {
            href: "/event-sourcing/patterns",
            title: "Production patterns",
            description: "Ten practical event-sourcing implementation guides.",
          },
          {
            href: "/examples",
            title: "Live demo",
            description: "Inspect timelines, replay, projections, and MCP access.",
          },
        ],
      },
    },
    {
      href: "/use-cases",
      label: "Use cases",
    },
    {
      href: "/docs",
      label: "Docs",
    },
    {
      href: "/pricing",
      label: "Pricing",
    },
  ],
  // Single source of truth for pricing tiers. The /pricing page and the
  // dashboard billing page both map over this array.
  //
  // Field contract for downstream prompts:
  //   tier         — canonical tier id (self-host | indie | studio | scale | enterprise).
  //                  This is the ONE naming scheme used end-to-end (matches the backend
  //                  subscription_tier after canonicalTier() normalization). DO NOT rename
  //                  casually; it also keys the LemonSqueezy catalog + checkout.
  //   mcp          — explicit MCP verbs so a buyer can price the upgrade at a glance.
  //   x402         — per-tier micropayment allowance, rendered as a single line.
  //                  `null` for tiers without metered x402 (Self-Host runs its own;
  //                  Enterprise is negotiated).
  pricing: [
    {
      // Self-Host is NOT advertised on /pricing — the product has no free plan
      // (14-day trial, then paid/enterprise), so the public pricing cards and
      // matrix filter this entry out (it reads as "Free"). It stays in this array
      // ONLY so the authenticated dashboard can still render a tenant that's on
      // the legacy `self-host` tier (the web analog of admin's planLabel) and so
      // getPlanConfig's `pricing[0]` fallback resolves to a safe entry. The
      // run-it-yourself Apache-2.0 story lives in the "Why no free plan?" FAQ below.
      name: "Self-Host",
      tier: "self-host" as const,
      href: "https://github.com/all-source-os/all-source",
      price: "Free",
      period: "your infra",
      yearlyPrice: "Free",
      mcp: "Full MCP (self-host)",
      x402: null,
      features: [
        "Unlimited events (your hardware)",
        "Forever retention",
        "Unlimited streams",
        "Full MCP (self-host)",
        "GitHub community support",
        "Apache-2.0 licensed",
      ],
      description: "Tinkerers, OSS, on-prem. Run it yourself.",
      buttonText: "Self-host on GitHub",
      isPopular: false,
      isSelfHost: true,
      isEnterprise: false,
    },
    {
      name: "Indie",
      tier: "indie" as const,
      href: "/signup",
      price: "£18.99",
      period: "month",
      yearlyPrice: "£15.17",
      mcp: "Hosted MCP: read",
      // x402 overage rate per PRICING_EXPOSURE_PLAN.md §3.5 ("$0.0001/call after").
      x402: { included: "50K x402 calls", overage: "$0.0001/call after" },
      features: [
        "500K events/month",
        "14-day retention",
        "3 streams",
        "Hosted MCP: read",
        "Email support (48h)",
      ],
      description: "Solo builders running one agent.",
      buttonText: "Start Indie",
      isPopular: false,
      isSelfHost: false,
      isEnterprise: false,
    },
    {
      name: "Studio",
      tier: "studio" as const,
      href: "/signup",
      price: "£78.99",
      period: "month",
      yearlyPrice: "£63.17",
      mcp: "Hosted MCP: read + write",
      x402: { included: "500K x402 calls", overage: "$0.0001/call after" },
      features: [
        "5M events/month",
        "90-day retention",
        "Unlimited streams",
        "Hosted MCP: read + write",
        "Email support (24h) + Discord",
      ],
      description: "Teams running 1–5 agents.",
      buttonText: "Start Studio",
      isPopular: true,
      isSelfHost: false,
      isEnterprise: false,
    },
    {
      name: "Scale",
      tier: "scale" as const,
      href: "/signup",
      price: "£298.99",
      period: "month",
      yearlyPrice: "£239.17",
      mcp: "Hosted MCP: read + write + dedicated",
      x402: { included: "5M x402 calls", overage: "$0.0001/call after" },
      features: [
        "50M events/month",
        "365-day retention",
        "Unlimited streams",
        "Hosted MCP: read + write + dedicated",
        "Priority support + Slack",
      ],
      description: "Companies with 50+ agents.",
      buttonText: "Start Scale",
      isPopular: false,
      isSelfHost: false,
      isEnterprise: false,
    },
    {
      name: "Enterprise",
      tier: "enterprise" as const,
      href: "mailto:sales@all-source.xyz?subject=Enterprise%20Plan%20Inquiry",
      price: "Custom",
      period: "month",
      yearlyPrice: "Custom",
      mcp: "Dedicated MCP cluster",
      x402: null,
      features: [
        "Negotiated event volume",
        "Unlimited retention",
        "Unlimited streams",
        "Dedicated MCP cluster",
        "24/7 + dedicated SE",
        "SLA & compliance",
      ],
      description: "Regulated / SLA workloads.",
      buttonText: "Talk to us",
      isPopular: false,
      isSelfHost: false,
      isEnterprise: true,
    },
  ],
  /**
   * Pricing-page FAQ set.
   *
   * GEO/AEO note: kept separate from `faqs` so /pricing and / do not emit the
   * SAME FAQPage graph on two URLs (duplicate schema splits the signal). Each
   * answer is written to be extractable on its own — it names the product,
   * carries the real number, and makes sense with no surrounding page context,
   * because an answer engine lifts the answer, not the page.
   *
   * Every figure here must match `siteConfig.pricing` and the live catalog.
   * Wrong prices in schema are worse than no schema: they get cited.
   */
  pricingFaqs: [
    {
      question: "How much does AllSource cost?",
      answer:
        "Hosted AllSource starts at £18.99/month for Indie (500K events/month, 14-day retention, 3 streams). Studio is £78.99/month (5M events, 90-day retention, unlimited streams) and Scale is £298.99/month (50M events, 365-day retention). Enterprise is negotiated. The live LemonSqueezy catalog is authoritative.",
    },
    {
      question: "Does AllSource have a free tier?",
      answer:
        "No free hosted tier. AllSource Core and community components are available under Apache-2.0, so the community route can be self-hosted on your own hardware. Designated enterprise features use BSL 1.1. Hosted plans start with a 14-day trial (1,000 events) and then require a paid tier.",
    },
    {
      question: "What happens if I exceed my monthly event quota?",
      answer:
        "Each paid tier includes metered x402 micropayment credits: 50K calls on Indie, 500K on Studio, 5M on Scale. Beyond the included allowance, usage bills at $0.0001 per call. Events themselves are quota'd per tier, so sustained overage is a signal to move up rather than an open-ended bill.",
    },
    {
      question: "Can I self-host AllSource instead of paying?",
      answer:
        "Yes. The core event store is Apache-2.0 licensed and runs anywhere Docker does, with unlimited events, forever retention, unlimited streams, and full MCP access on your own infrastructure. Some enterprise-specific features are licensed under BSL 1.1. Self-hosting is supported through GitHub rather than email or Slack.",
    },
    {
      question: "Is AllSource priced per seat or per user?",
      answer:
        "Neither. AllSource prices on events written and retained, not on people. A team of one and a team of thirty pay the same for the same event volume, because the cost driver is agent throughput and retention window — not headcount. Streams and MCP access level vary by tier.",
    },
  ],
  faqs: [
    {
      question: "Why no free plan?",
      answer:
        "There is no permanent free hosted plan. The Apache-2.0 Core and community route can be self-hosted; designated enterprise features use BSL 1.1. Hosted pricing covers operated infrastructure.",
    },
    {
      question: "What is AllSource?",
      answer:
        "AllSource is a purpose-built event store database for event sourcing. Core stores ordered, immutable application events; Query Service builds tenant-facing read paths; Prime derives agent memory from the same history; hosted services operate the stack.",
    },
    {
      question: "Is AllSource the same as ArcGIS AllSource?",
      answer:
        "No. AllSource Event Store is the developer product at all-source.xyz and github.com/all-source-os/all-source. ArcGIS AllSource is Esri intelligence-analysis software. The products are unrelated.",
    },
    {
      question: "How does AllSource compare to traditional databases?",
      answer:
        "Traditional databases can preserve history, but teams usually add audit tables, change-data capture, or application logs. AllSource makes immutable event history the primary record, so replay, point-in-time reconstruction, and provenance use the same ordered stream.",
    },
    {
      question: "What is the MCP Server integration?",
      answer:
        "AllSource has separate MCP connectors. The event-store connector exposes 45 read-only tools, 55 by default, 64 with control-plane access, and 73 with system administration. Prime exposes 19 prime_* memory tools, or 27 with optional inbox and hound modules. Do not add these registries together.",
    },
    {
      question: "How secure is my data?",
      answer:
        "Hosted AllSource separates tenant data and supports JWT authentication, role-based access, policy enforcement, and audit logs. Administrative operations include dry-run previews where the API supports them.",
    },
    {
      question: "Can I self-host AllSource?",
      answer:
        "Yes. The core is Apache-2.0 licensed, while designated enterprise features use BSL 1.1. Docker images, Helm charts, and Kubernetes manifests are available in the repository; hosted plans operate the services for you.",
    },
    {
      question: "What is Quant Intelligence?",
      answer:
        "Quant Intelligence is a solution route using AllSource event history and shipped analytics endpoints for bars, correlations, forecasts, and regime summaries. It is not a separate database, a trading model, or a promise of profitable probabilities.",
    },
  ],
  footer: [
    {
      title: "Platform",
      links: [
        { href: "/what-is-allsource", text: "What is AllSource?", icon: null },
        { href: "/platform/event-sourcing", text: "Event Store", icon: null },
        { href: "/event-sourcing/patterns", text: "Event-Sourcing Patterns", icon: null },
        { href: "/platform/stream-processing", text: "Stream Processing", icon: null },
        { href: "/platform/prime", text: "Prime (agent memory)", icon: null },
        { href: "/docs/mcp", text: "MCP Server", icon: null },
        { href: "/docs", text: "Documentation", icon: null },
      ],
    },
    {
      // GEO/AEO note: every /solutions/* page was an orphan — present in
      // sitemap.ts, cross-linked between three of the pages, and reachable from
      // no site-wide navigation at all. Sitemap presence alone is a weak
      // discovery signal, and crawlers that follow links (including AI
      // indexers) had no path to them. This column is that path.
      title: "Solutions",
      links: [
        { href: "/solutions/agent-memory", text: "Agent Memory", icon: null },
        { href: "/solutions/real-time-analytics", text: "Real-Time Analytics", icon: null },
        { href: "/solutions/audit-compliance", text: "Audit & Compliance", icon: null },
        { href: "/solutions/financial-services", text: "Financial Services", icon: null },
        { href: "/solutions/multi-tenant-saas", text: "Multi-Tenant SaaS", icon: null },
        { href: "/solutions/iot-telemetry", text: "IoT Telemetry", icon: null },
        { href: "/solutions/quant-intelligence", text: "Quant Intelligence", icon: null },
      ],
    },
    {
      title: "Developers",
      links: [
        { href: "/docs", text: "Getting Started", icon: null },
        { href: "/docs/api", text: "API Reference", icon: null },
        { href: "/sdks", text: "SDKs", icon: null },
        { href: "/examples", text: "Examples", icon: null },
        { href: "/changelog", text: "Changelog", icon: null },
      ],
    },
    {
      title: "Compare",
      links: [
        { href: "/event-sourcing-for-ai-agents", text: "Event Sourcing for AI Agents", icon: null },
        { href: "/vs/mem0", text: "vs mem0", icon: null },
        { href: "/vs/letta", text: "vs Letta", icon: null },
        { href: "/vs/zep", text: "vs Zep", icon: null },
        { href: "/vs/stoolap", text: "vs stoolap", icon: null },
      ],
    },
    {
      title: "Company",
      links: [
        { href: "/about", text: "About", icon: null },
        { href: "/design-partners", text: "Design partners", icon: null },
        { href: "/blog", text: "Blog", icon: null },
        { href: "/status", text: "Status", icon: null },
        { href: "/privacy", text: "Privacy Policy", icon: null },
        { href: "/terms", text: "Terms of Service", icon: null },
      ],
    },
    {
      title: "Connect",
      links: [
        {
          href: "https://github.com/all-source-os/all-source",
          text: "GitHub",
          icon: "github" as const,
        },
        {
          href: "https://x.com/ddonprogramming",
          text: "Twitter",
          icon: "twitter" as const,
        },
        {
          href: "https://github.com/all-source-os/all-source/discussions",
          text: "Community",
          icon: "discord" as const,
        },
      ],
    },
  ],
};

export type SiteConfig = typeof siteConfig;

// The Indie tier's monthly price, sourced from the pricing array so the
// homepage CTA can never desync from the current fallback snapshot. Live
// LemonSqueezy catalog remains authoritative when available.
export const indieTier = siteConfig.pricing.find((p) => p.tier === "indie");
export const indiePrice = indieTier?.price ?? "£18.99";
