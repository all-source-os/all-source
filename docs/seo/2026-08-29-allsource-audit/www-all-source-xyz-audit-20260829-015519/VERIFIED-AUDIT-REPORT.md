# Verified SEO Audit: www.all-source.xyz

- Audit date: 2026-08-29
- Business type: developer-infrastructure SaaS
- Sitemap URLs: 99
- Verified SEO health score: **83/100**
- Raw deterministic pipeline score: 78/100

The raw pipeline could not reach PageSpeed during its first pass and substituted
heuristic LCP/INP values, producing a false 35/100 performance score. A direct
PageSpeed API run completed afterward. This report uses that verified evidence.

## Scorecard

| Category | Score | Evidence summary |
| --- | ---: | --- |
| Technical SEO | 87 | Crawl/index controls pass; caching and redirect warnings remain |
| Content quality | 76 | Concrete technical evidence; authority and contextual-link gaps |
| On-page SEO | 74 | Unique metadata and H1s; many snippet-length outliers |
| Schema | 74 | Syntax clean; global entity graph and SoftwareApplication eligibility need repair |
| Performance | 98 | PageSpeed mobile 96 and desktop 99; no CrUX eligibility yet |
| AI/GEO readiness | 80 | AI crawlers allowed, SSR and llms files strong; source consistency weak |
| Images | 98 | Modern, sized images with good alt text; one lazy-loading gap |

Weighted score: **83/100**.

## Executive findings

No current sitemap protocol failure exists. Google Search Console still shows its
August 27 result, while current deployment and sitemap were published August 29.
Googlebot receives valid XML, and all 99 listed URLs return 200.

Highest-value remaining work:

1. Restore public-page caching. Marketing routes send
   `private, no-cache, no-store` because server-side `cookies()` is used for the
   dismissible design-partner banner. This defeats intended ISR and bfcache.
2. Repair structured-data entity graph. Global layout emits Organization, Person,
   WebSite, and incomplete SoftwareApplication nodes on every public page.
3. Remove public-source drift. Docs.rs package READMEs, repository MCP tool counts,
   and current website claims disagree.
4. Fix metadata outliers and weak contextual linking before publishing more
   overlapping agent-memory content.
5. Fix four contrast failures and investigate the Vercel Analytics script that
   Lighthouse saw returning 404 with a MIME refusal.

## Sitemap and indexability

Live [`/sitemap.xml`](https://www.all-source.xyz/sitemap.xml):

- HTTP 200 for GET, HEAD, and Googlebot user agent
- `Content-Type: application/xml`
- valid XML namespace and syntax
- 9,328 bytes
- 99 URLs; zero duplicates
- 99/99 URLs return 200; zero redirecting sitemap entries
- no listed page carries `noindex`
- referenced by [`robots.txt`](https://www.all-source.xyz/robots.txt)

Google Search Console API currently reports:

- submitted: 2026-08-27 15:10 UTC
- state: pending
- parse errors: 0
- warnings: 0
- UI last read: 2026-08-27

Current Vercel sitemap response was deployed 2026-08-29 01:46 UTC. Resubmission is
the correct next action; changing working sitemap code is not.

GSC page-indexing report is still processing. Search Performance has 12 impressions
and zero clicks for 2026-08-01 through 2026-08-26, too little data for ranking
conclusions.

## Technical SEO

### Passes

- crawlability 100/100
- indexability 94/100
- HTTPS/TLS, HSTS, CSP, X-Frame-Options, nosniff, referrer and permissions headers
- self-canonical, one H1, English language, title and description on all 99 URLs
- no document-level horizontal overflow at 320, 390, or 1,280 pixels
- SSR exposes body copy, metadata, and JSON-LD without client execution

### Findings

- Public marketing HTML sends `private, no-cache, no-store`; server-side banner
  dismissal reads cookies on homepage and marketing layout.
- `http://all-source.xyz` reaches canonical host through two hops; HTTPS apex uses
  temporary 307. Prefer one permanent 308/301 to `https://www.all-source.xyz`.
- `x-powered-by` reveals Next.js. Low-risk hardening item.
- CSP permits `unsafe-inline` scripts and styles. Improve only with tested nonce/hash
  migration; do not break Next.js runtime.
- IndexNow absent. Optional and low priority.

## Performance and accessibility

Direct Google PageSpeed API, 2026-08-29:

| Metric | Mobile | Desktop |
| --- | ---: | ---: |
| Performance | 96 | 99 |
| Accessibility | 96 | 96 |
| Best Practices | 96 | 96 |
| SEO | 100 | 100 |
| LCP | 1.8 s | 0.5 s |
| FCP | 1.2 s | 0.3 s |
| TBT | 200 ms | 10 ms |
| CLS | 0 | 0 |

No CrUX field data exists yet because origin lacks enough Chrome traffic. Lab
results must not be presented as field INP.

Remaining performance work:

- about 121 KiB unused JavaScript in mobile lab run
- GTM/GA contributes third-party transfer and main-thread work
- one render-blocking stylesheet showed about 108-310 ms potential savings
- mobile Speed Index varied between 2.9 and 4.1 seconds across runs

Contrast failures:

- hero `recalling from event log...`
- `-20%` badge
- `Popular` badge
- disabled-looking `Talk to us`

## Content, on-page SEO, and SXO

AI-slop risk is low on technical articles: real failure modes, code contracts,
benchmarks, and source links dominate. Generic use-case cards such as Personal AI
Assistant, Research Assistant, and Code Review Context carry medium slop risk until
they gain real traces or fixtures.

Strong pages:

- [`/event-replay-debugging`](https://www.all-source.xyz/event-replay-debugging)
- [`/what-is-allsource`](https://www.all-source.xyz/what-is-allsource)
- [`/solutions/agent-memory`](https://www.all-source.xyz/solutions/agent-memory)

Priority metadata outliers:

- `/about`: title 5 characters
- `/docs/prime/quickstart`: title 16, description 58
- `/vs/mem0`: title 17
- `/blog/why-your-agents-memory-returned-nothing`: title 73, description 314
- `/blog/building-agent-memory-in-rust`: title 66, description 185
- `/compare/agent-memory`: description 187
- `/pricing`: description 170
- `/solutions/agent-memory`: description 166

Two sampled blog posts expose no contextual internal links beyond campaign, author,
and trial links. Add 3-5 relevant links per article and use
`/solutions/agent-memory` as commercial hub.

Universal `See program` banner becomes first body link across docs, blog,
comparisons, pricing, and product pages. Suppress it on task-focused docs or move it
below page-specific primary action.

`/compare/agent-memory` needs a primary-source-backed comparison table covering
portability, provenance, temporal reconstruction, semantic recall, and operational
burden.

## Structured data

Across 99 URLs:

- 506 JSON-LD blocks
- zero JSON parse errors
- zero bad contexts, relative schema URLs, placeholders, or invalid ISO dates
- 41/41 blog posts emit BlogPosting

Primary failure is semantic, not syntactic:

- 99 of 100 SoftwareApplication nodes omit `offers.price`, so they do not satisfy
  Google Software App eligibility.
- `/pricing` emits duplicate SoftwareApplication nodes with same `@id`.
- AllSource product/brand is modeled as Organization; Wolven Tech should be legal
  publisher Organization, AllSource should be Brand + SoftwareApplication.
- Personal X account appears in product Organization `sameAs`; keep it on Person.
- Organization, Person, WebSite, and incomplete SoftwareApplication are repeated on
  every page. Consolidate graph on homepage/about/pricing and keep page schema local.
- Two TechArticle pages need Article compatibility, image, dates, URL, and visible
  author/updated date.
- FAQPage remains useful to parsers but should not be treated as likely Google rich
  result outside government/health authority sites.

## GEO and AI-search readiness

Strengths:

- GPTBot, OAI-SearchBot, ChatGPT-User, Claude and Perplexity crawlers explicitly
  allowed and verified with identical 200 HTML responses
- [`llms.txt`](https://www.all-source.xyz/llms.txt) and
  [`llms-full.txt`](https://www.all-source.xyz/llms-full.txt) return 200 text/plain
- strong SSR, direct-answer pages, code, tables, and BlogPosting coverage

Weaknesses:

- 21/41 articles contain no external sources
- 18 modified posts hide modified date from visible template
- only 3/41 posts contain inline article images
- `llms-full.txt` contains expired literal sample `trial_expires_at: 2026-08-25`
- Docs.rs and repository claims disagree with current releases/tool counts
- no strong independent product entity in Wikipedia, Wikidata, Reddit, YouTube, or
  product-owned LinkedIn company page

## Evidence limits

- PageSpeed results are lab data; CrUX field metrics unavailable.
- Content/SXO deep review sampled 11 representative pages; metadata and indexability
  checks covered all 99 sitemap URLs.
- GSC property is new and still processing page-indexing data.
- No customer proof was inferred or invented.
