# SEO and AI-search evidence baseline — www.all-source.xyz

Captured 2026-10-20. Property `https://www.all-source.xyz`, production, English,
US search database.

## The claim

**The site's pages are technically clean and almost nobody reaches them.** Static
HTML scores 100/100 with ten of ten checks executed, canonicals are correct and
strip query parameters, the sitemap is valid at 113 URLs, and `robots.txt` and
`llms.txt` are both deliberate. Against that: Domain Authority **2**, **3**
referring domains, **12** ranked keywords, best position **32**, and **2** organic
clicks a month.

The seven metadata-length findings from the static audit are real and are fixed in
this change. They are also not why the site gets two clicks a month. Nothing in
the metadata layer moves a DA-2 domain.

## Evidence ledger

### Observed

**Organic footprint** — SpyFu, US, September 2026 snapshot:

| Measure | Value |
|---|---|
| Ranked keywords | 12 |
| Average organic rank | 60.4 |
| Organic clicks / month | 2 |
| Modelled organic value / month | $172.20 |
| Paid keywords | 0 |

Positions, same source:

| Keyword | Volume | KD | Our URL | Rank |
|---|---|---|---|---|
| all source | 180 | 15 | `/` | 32 |
| all source | 180 | 15 | `/pricing` | 74 |
| allsource | 180 | 20 | `/platform/event-sourcing` | 56 |
| allsource | 180 | 20 | `/` | 77 |
| allsource | 180 | 20 | `/signup` | 92 |
| allsource | 180 | 20 | `/pricing` | 94 |
| eventsource db | — | 15 | `/platform/event-sourcing` | 35 |
| streamsource | 40 | 18 | `/platform/stream-processing` | 42 |
| database for event sourcing | — | 15 | `/platform/event-sourcing` | 44 |
| event sourcing databases | — | 15 | `/platform/event-sourcing` | 51 |
| cross-sourced querying | — | 19 | `/blog/time-travel-queries` | 62 |
| agent source | 40 | 31 | `/event-sourcing-for-ai-agents` | 66 |

Nothing ranks inside the top 30. `allsource` moved **-32 positions** over the
snapshot period. One page, `/platform/event-sourcing`, holds four of the twelve
and every genuinely commercial term among them.

**Authority** — Moz via `treg`, crawled 2026-10-16: Domain Authority **2**, Page
Authority 12, 9 backlinks, **3 referring domains**, spam score 2, 104 pages
crawled from the root domain. The apex `all-source.xyz/` answers **308** to the
`www` host.

**Link profile** — SE Ranking via `treg`, 33 rows across 7 referring domains:

| Domain | DR | Links | Follow? | Points at |
|---|---|---|---|---|
| inite.ai | 44 | 8 | follow | `robots.txt` and `llms.txt` |
| wolventech.com | 33 | 3 | follow | homepage, `/event-replay-debugging` |
| decebaldobrica.com | 9 | 4 | follow | homepage, two checklist pages |
| sundayclub.so | 22 | 2 | nofollow | homepage |
| opendeer.net | 1 | 12 | nofollow | homepage |
| skillavatars.com | 3 | 2 | follow | homepage |
| xploredomains.com | 51 | 1 | nofollow | homepage |

The highest-authority follow links on the whole profile — eight from inite.ai at
DR 44 — point at `robots.txt` and `llms.txt`. Two of the three remaining follow
domains are owned by the same person as the site.

**Rendered DOM** — local headless Chrome, `/docs`, 119,275 bytes: the rendered
schema types are **identical** to the static response
(`Organization`×3, `WebSite`, `SoftwareApplication`, `Person`, `ImageObject`,
`ContactPoint`). No content is gated behind JavaScript, which is what licenses
treating a static-HTML audit as a proxy for the real page on this site.

Across the 8 template pages rendered, **4 carried no page-type schema at all** —
only the site-wide blocks inherited from the root layout:

| Page | Page-type schema in rendered DOM |
|---|---|
| `/` | `FAQPage`, `Question`, `Answer` |
| `/pricing` | `Offer`, `FAQPage` |
| `/what-is-an-event-store` | `TechArticle`, `BreadcrumbList` |
| `/blog/why-your-agents-memory-returned-nothing` | `BlogPosting`, `BreadcrumbList` |
| `/docs` | **none** |
| `/prime` | **none** |
| `/platform/event-sourcing` | **none** |
| `/install/cursor` | **none** |

`/platform/event-sourcing` is on that list, and it is the one page holding every
commercial keyword in the table above.

**Canonicalization** — verified by direct request: `/?bet=allsource`,
`/pricing`, `/platform/event-sourcing` and `/event-replay-debugging?bet=allsource`
all return 200 with a self-referential canonical that strips the tracking
parameter. There is no duplicate-URL problem.

`/signup` now returns `noindex, nofollow` with a canonical to the homepage, so
the rank-92 row above will drop out. The SpyFu snapshot predates that change.

### Observed empty

Serpstat's `g_us` database answers **"Data not found"** for the domain — zero
ranked keywords. Cross-checked against SpyFu's 12, so this is thin coverage in
one vendor's index rather than a second confirmation of zero.

### Setup required

| Source | Blocked on |
|---|---|
| Search Console API | service-account key + property grant |
| GA4 Data API | service-account key + property Viewer + numeric property id |
| PageSpeed Insights | a Google Cloud API key — the anonymous quota is **0** |
| Chrome UX Report | a Google Cloud API key |
| Bing Webmaster | site verification + API key |

No ranking-trend, traffic, Core Web Vitals or indexation-count claim is made from
these. `docs/runbooks/SEO_EVIDENCE_SETUP.md` carries the unblock condition for
each; `seo-evidence doctor` is the live state.

### Not applicable

Google Business Profile. A listing needs a physical location or service area; a
software product has neither.

## Findings

Ordered by what moves the numbers above, which is not the order the static audit
produced.

**1. Three referring domains, two of them owned by the site's author.**
Evidence: the link table above. Every ranking in the first table is a position
that DA 2 can hold and no higher. **Fix:** earn follow links from domains nobody
here controls. **Verification:** `treg.web.backlinks.summary` → `referring_domains`
and `domain_authority`, re-read monthly.

**2. The best follow links on the profile point at `robots.txt` and `llms.txt`.**
Evidence: 8 links from inite.ai (DR 44), both targets. Those files are not
content and pass nothing usable. **Fix:** nothing to change on the site — this is
a note so the DR-44 row is not mistaken for authority already arriving.

**3. First-party measurement is entirely absent.** Evidence: five sources in the
setup-required table; GA4's tag ships but its Data API has no credential, and
Search Console is verified for the UI but has no API grant. Consequence: no
impression, click-through or indexation figure in this report is first-party, and
a ranking change cannot be attributed to a deploy. **Fix:** the runbook, six
secrets. **Verification:** `seo-evidence doctor --strict` exits 0.

**4. One page carries the commercial terms.** Evidence:
`/platform/event-sourcing` holds `eventsource db` (35), `database for event
sourcing` (44), `event sourcing databases` (51). **Fix:** treat it as the
money page — internal links, depth, schema — rather than spreading the same
terms thinner. **Verification:** its four positions, monthly.

**5. Four of eight sampled pages emit no schema describing themselves.**
Evidence: the schema table above, from rendered DOM. Fixed here for the two that
matter most — `/docs` gets `CollectionPage` + `BreadcrumbList` (it is a hub),
`/platform/event-sourcing` gets `BreadcrumbList` from its layout, since its page
is a client component and cannot carry a schema export. **Left undone:** `/prime`
and `/install/cursor` (9 install pages share that template). **Verification:**
`seo-evidence collect --source render` — `schema_types` for `/docs` contains
`CollectionPage`, and `/platform/event-sourcing` contains `BreadcrumbList`.

**6. Seven metadata length findings, all low.** `/blog/why-your-agents-memory-returned-nothing`
(314-char description, 73-char title), `/blog/allsource-as-cms-from-claude-desktop`
(265), `/install/cursor` (187), `/pricing` (170),
`/blog/multi-agent-coordination-event-stream` (165), `/docs` (13-char title).
Fixed at the **template** for the blog (43 of 113 URLs) and the install pages
(9 URLs) with a clamp, so the next author cannot reintroduce them; `/docs` and
`/pricing` fixed directly. The full post summary is kept in `BlogPosting` schema,
which has no length limit and reads better as an extractable passage.

## Smallest useful sequence

1. The six secrets in the runbook. Until they exist, every claim about trend is
   somebody else's crawl of the site.
2. Links from domains nobody here owns. This is the binding constraint and the
   slowest to move.
3. `/platform/event-sourcing` as the single commercial entry point.
4. Nothing else in the metadata layer. It is already clean.

Confidence: **rock** on 1 and 3 — the work is understood and bounded. **Sand** on
2: no shaped plan for earning links exists yet, and a link-building bet made
before shaping is how one bet becomes three.

## Measurement plan

`.github/workflows/seo-evidence.yml` runs weekly, writes a ledger, and compares
it against `docs/reports/baselines/seo-evidence-2026-10-20.json`. It fails on a
**regression**, meaning a source that moved down
`observed > observed_empty > failed > setup_required` or vanished.

Losing an observation is the failure this is for. A run whose credential was
revoked produces a ledger full of `setup_required` and no error, and reads as a
clean report until something compares it with the run before.

The `treg` figures are not in the workflow — they cost money per call and are
run on demand. Monthly is enough for DA and referring domains; nothing in that
table moves weekly.

## Limits

Single property, one market, US database. The ranking and link figures are
**third-party crawls**, not this site's own data, and the SpyFu snapshot is from
September — one finding in it (`/signup` at rank 92) is already stale because the
page is now `noindex`. No Core Web Vitals figure of any kind appears here, lab or
field. The static audit covered 9 of 113 sitemap URLs, so 104 are unaudited. The
rendered-DOM comparison covered one page. Nothing here establishes how many pages
are actually indexed — that needs Search Console.
