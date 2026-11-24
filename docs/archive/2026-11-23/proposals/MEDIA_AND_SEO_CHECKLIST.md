# Media Production & SEO Checklist — Marketing Readiness

> **Status**: Active
> **Date**: 2026-03-24
> **Goal**: Every page on all-source.xyz renders correctly, has proper SEO, and has real visual assets

---

## CRITICAL: Missing Assets Blocking Production

These are referenced in code but **do not exist**:

| Asset | Referenced By | Times | Impact |
|-------|-------------|-------|--------|
| `/dashboard.png` | All 13 blog posts, hero section, OG image generator, solution pages | 30+ | Every blog post has broken OG image, hero has broken visual |
| `/author.jpg` | Blog post template (hardcoded) | 1 | Broken author avatar on every blog post |

**Action**: Create these two files or update all references.

---

## Part 1: Videos to Produce

### 1.1 Terminal Recordings (asciinema) — 3 recordings

| Recording | Command | Duration | Embed Location |
|-----------|---------|----------|----------------|
| `prime-graph.cast` | `cargo run --features prime --example prime_graph` | ~15s | `/docs/prime/quickstart` |
| `prime-vectors.cast` | `cargo run --features prime-full --example prime_vectors` | ~20s | `/docs/prime/embedded` |
| `prime-recall.cast` | `cargo run --features prime-recall --example prime_recall` | ~25s | `/docs/prime/concepts` |

**Tool**: `asciinema rec <file>.cast -c "<command>"`
**Playback speed**: 1.5x
**Embed**: `<asciinema-player>` component or GIF conversion

### 1.2 Demo Video: 60 seconds — Claude Desktop + Prime

**Storyboard**:
1. (0:00-0:10) User opens Claude Desktop, Prime MCP is connected
2. (0:10-0:20) User: "What do you know about our project?" → Claude calls `prime_index` → compressed index shown
3. (0:20-0:30) User: "Alice is now leading the security team" → Claude calls `prime_add_node` + `prime_add_edge`
4. (0:30-0:45) User: "How does security relate to engineering?" → Claude calls `prime_context` → cross-domain result
5. (0:45-0:60) Tagline: "cargo install allsource-prime" + GitHub link

**Requirements**:
- Captions (SRT file) for muted autoplay on social
- 1920x1080 or 1080x1080 (square for X/Twitter)
- Dark theme Claude Desktop

**Distribution**: X thread #2, YouTube, blog embed, `/solutions/agent-memory`

### 1.3 Technical Walkthrough: 5 minutes

**Outline**:
1. (0:00-0:30) Problem: agents forget, 3-database problem
2. (0:30-1:30) Architecture: everything is an event (show diagram)
3. (1:30-2:30) Projections: show `Projection` trait, DashMap lookups
4. (2:30-3:30) Compressed index: `build_heuristic_index()`, auto-generation
5. (3:30-4:30) Hybrid recall: `RecallEngine.context()`, cross-domain
6. (4:30-5:00) Benchmarks + install command

**Requirements**: Large font, dark IDE theme, code readable at 720p

---

## Part 2: Architecture Diagrams to Create

### 2.1 "One Engine, Not Three Databases" — PRIMARY

**Currently**: ASCII art in `agent-memory/page.tsx` and blog posts
**Need**: Polished SVG/PNG, 1200x600px

```
┌─────────────────────────────────────────────┐
│              AllSource Prime                 │
│  Graph    Vectors    Temporal    Compressed   │
│  ┌──────────────────────────────────────┐   │
│  │  WAL + Parquet + DashMap + HLC + CRDT │   │
│  └──────────────────────────────────────┘   │
└─────────────────────────────────────────────┘
```

**Use in**: Homepage hero, `/solutions/agent-memory`, blog posts, X thread #3

### 2.2 "Everything Is an Event" — Event Flow

**Shows**: `prime.node.created` → Projections → Query Results
**Use in**: `building-agent-memory-in-rust.mdx`, `/docs/prime/concepts`

### 2.3 "zer0dex vs AllSource" — Side-by-Side

**Shows**: Two architectures compared visually (not just table)
**Use in**: `zer0dex-vs-allsource-recall.mdx`, X thread #1

### 2.4 "Recall Pipeline" — Data Flow

**Shows**: Domain events → DomainIndex → CrossDomain → CompressedIndex → RecallEngine → Response
**Use in**: `compressed-index-doubles-cross-domain-recall.mdx`, `/docs/prime/concepts`

### 2.5 "Agent Memory Problem" — Before/After

**Shows**: Current state (ephemeral context window) vs Prime (persistent graph)
**Use in**: `ai-agents-need-memory.mdx`, homepage

---

## Part 3: Static Images to Create

### 3.1 Dashboard Hero Screenshot — `/public/dashboard.png`

**THE most critical missing asset.** Referenced 30+ times.

**Should show**: Real Prime dashboard or mockup with:
- Graph visualization (nodes colored by domain)
- Compressed index panel
- Query results with relevance scores
- Dark theme matching the site

**Size**: 1920x1080 (displayed at various sizes)

### 3.2 Blog Post OG Images — 13 unique images

Each blog post currently uses `/dashboard.png` (which doesn't exist). Need unique OG images:

| # | Blog Post | OG Image Concept |
|---|-----------|-----------------|
| 1 | Introducing AllSource | Logo + "Time-travel your data" tagline |
| 2 | AI Agents Need Memory | Brain/memory icon + context window visualization |
| 3 | zer0dex vs AllSource | Split comparison visual |
| 4 | Building Agent Memory in Rust | Rust logo + architecture diagram |
| 5 | Compressed Index Doubles Recall | Chart: 37.5% → 80% bar graph |
| 6 | 12μs Agent Memory | Speedometer/latency visualization |
| 7 | From zer0dex to AllSource | Arrow from one to the other |
| 8 | Time-Travel Queries | Timeline visualization |
| 9 | Why Event Sourcing 2026 | Event stream visualization |
| 10 | Temporal AI: Future of RAG | AI + temporal data mashup |
| 11 | Event Store vs Database | Two icons compared |
| 12 | MCP Tools + Claude | Claude logo + tools grid |
| 13 | Tiered Context Loading | L0/L1/L2 tier pyramid |

**Size**: 1200x630px (standard OG), PNG
**Branding**: AllSource logo + purple-blue gradient + white text

### 3.3 Author Avatar — `/public/author.jpg`

**Currently**: Hardcoded reference to `/author.jpg` in blog template
**Need**: AllSource team avatar or logo mark
**Size**: 100x100px minimum

### 3.4 Docs Hero Banners — 5 images

| Page | Banner Concept |
|------|---------------|
| `/docs/prime/quickstart` | Terminal with "cargo install" |
| `/docs/prime/concepts` | Projection architecture |
| `/docs/prime/mcp` | Claude Desktop integration |
| `/docs/prime/http` | API endpoint curl examples |
| `/docs/prime/embedded` | Rust code with crate import |

**Size**: 1200x400px, dark theme

---

## Part 4: SEO Fixes Required

### 4.1 CRITICAL: Missing robots.txt

**Action**: Create `/apps/web/public/robots.txt`

```
User-agent: *
Allow: /
Disallow: /dashboard/
Disallow: /api/
Disallow: /onboarding/

Sitemap: https://www.all-source.xyz/sitemap.xml
```

### 4.2 CRITICAL: Sitemap incomplete

**Current state**: Only includes `/` and `/blog/*` posts.
**Missing**:
- `/docs` and all `/docs/*` subpages
- `/solutions/agent-memory`
- `/solutions/quant-intelligence`
- `/privacy`, `/terms`, `/changelog`, `/status`

**Action**: Update `apps/web/src/app/sitemap.ts` to include all public pages.

### 4.3 Missing metadata on "use client" pages

These pages have NO server-side metadata (title, description, OG tags):

| Page | Fix |
|------|-----|
| `/solutions/agent-memory` | Convert to server component or add `generateMetadata` |
| `/solutions/quant-intelligence` | Same |
| `/status` | Same |
| `/` (homepage) | Add explicit metadata export |

**Impact**: Google shows generic title, no OG preview on social sharing.

### 4.4 Missing canonical URLs

No pages have explicit `<link rel="canonical">` tags.

**Action**: Add to `constructMetadata()` in `utils.ts`:
```ts
alternates: {
  canonical: `${siteConfig.url}${pathname}`,
}
```

### 4.5 Hardcoded stats inconsistencies

| Stat | Location 1 | Location 2 | Correct Value |
|------|-----------|-----------|---------------|
| MCP tools | "27" (hero, features, 20+ places) | "43" (config.ts) | **13 Prime + 61 Elixir = 74 total** |
| Version | "v0.10.0" (hero pill) | "0.17.0" (Cargo.toml) | **v0.17.0** |
| Performance | "469K events/sec" | Same everywhere | Current (verify with latest bench) |
| Latency | "11.9μs" → "12μs" | Mixed | Standardize to one |

### 4.6 Structured data (JSON-LD) gaps

**Has JSON-LD**: Blog posts (BlogPosting schema)
**Missing JSON-LD**:
- Homepage: `Organization` + `SoftwareApplication` schema
- Solutions pages: `Product` schema
- Docs pages: `TechArticle` schema

### 4.7 Image alt text audit

Many images use generic or missing alt text. The `dashboard.png` references have alt text like "AllSource Dashboard" which is fine, but images in blog posts and marketing pages should have descriptive alt text for accessibility and SEO.

---

## Part 5: Social Media Assets

### 5.1 X/Twitter Launch Campaign — 3 threads

| Thread | Visual Needs | Status |
|--------|-------------|--------|
| #1 Launch (10 tweets) | Architecture diagram (2.1) | Text drafted |
| #2 Demo (4 tweets) | 60-second video (1.2) + benchmark chart | Video TODO |
| #3 Technical (5 tweets) | Code screenshots + diagram (2.2) | TODO |

### 5.2 Benchmark Publication Visuals

| Chart | Data | Format |
|-------|------|--------|
| Cross-domain recall bar chart | zer0dex 80%, RAG 37.5%, Prime 80%+ | PNG 1200x600 |
| Latency comparison | zer0dex 70ms, Mem0 200ms+, Prime 12μs | PNG 1200x600 |
| Feature matrix card | 7 features × 5 systems | PNG 1200x800 |

---

## Production Priority Order

### Must-have before launch (blocks marketing):
1. [x] **`robots.txt`** — DONE (committed c183b1e)
2. [x] **Fix sitemap.ts** — DONE, 30+ entries (committed c183b1e)
3. [x] **Fix metadata** on solutions + homepage — DONE, canonical URLs + Twitter cards (committed c183b1e)
4. [x] **Fix version number** in hero pill — DONE, v0.10.0 → v0.17.0 (committed c183b1e)
5. [x] **Fix MCP tools count** — DONE, 27 → 74 (committed c183b1e)
6. [x] **Canonical URLs** on all pages — DONE (committed c183b1e)
7. [x] **Recharts v3 build fix** — DONE, Tooltip formatter type (committed 6e142dc)
8. [x] **Demo login proxy fix** — DONE, route to Control Plane (committed 8ecaf80)
9. [ ] **`/public/dashboard.png`** — NEEDS HUMAN: take a real screenshot of the dashboard or create a high-quality mockup. Referenced 30+ times. Every blog OG image and the hero section are broken without this.
10. [ ] **`/public/author.jpg`** — NEEDS HUMAN: create or source a team avatar (100x100px minimum). Blog author cards are broken without this.

### Should-have for launch week:
11. [ ] **3 terminal recordings** — NEEDS HUMAN: install asciinema, run each example, record output
    ```bash
    brew install asciinema
    cd apps/core
    asciinema rec prime-graph.cast -c "cargo run --features prime --example prime_graph"
    asciinema rec prime-vectors.cast -c "cargo run --features prime-full --example prime_vectors"
    asciinema rec prime-recall.cast -c "cargo run --features prime-recall --example prime_recall"
    ```
    Upload to asciinema.org or convert to GIF with `agg`.

12. [ ] **60-second demo video** — NEEDS HUMAN: screen record Claude Desktop with Prime MCP
    - Setup: `cargo install allsource-prime`, add to Claude Desktop config
    - Storyboard: ask question → prime_index → add knowledge → cross-domain query → result
    - Tools: OBS Studio or macOS screen recording, add captions with CapCut/Descript
    - Export: 1920x1080 MP4 + 1080x1080 square crop for X/Twitter
    - Upload to YouTube, embed on `/solutions/agent-memory`

13. [ ] **5 architecture diagrams** — NEEDS HUMAN: create in Figma, Excalidraw, or draw.io
    - 2.1 "One Engine" — Prime layers over Core (1200x600px SVG)
    - 2.2 "Everything Is an Event" — event flow to projections (1200x600px SVG)
    - 2.3 "zer0dex vs AllSource" — side-by-side comparison (1200x600px SVG)
    - 2.4 "Recall Pipeline" — domain → cross-domain → index → recall (1200x600px SVG)
    - 2.5 "Agent Memory Problem" — before/after (1200x600px SVG)
    - Save as SVG in `/apps/web/public/diagrams/` and reference in blog posts

14. [ ] **13 unique blog OG images** — NEEDS HUMAN: design template + generate per post
    - Template: 1200x630px, AllSource brand (purple-blue gradient), white text
    - Each post gets unique title overlay + relevant icon/visual
    - Tools: Figma template with text layer, or use `@vercel/og` dynamic generation
    - Alternative: update `/app/og/route.tsx` to generate better dynamic OG images (code fix, no design needed)
    - Place in `/apps/web/public/blog/` or use dynamic OG route

### Nice-to-have post-launch:
15. [ ] **5-minute technical walkthrough video** — NEEDS HUMAN: record IDE walkthrough
    - Show architecture diagram, then code: Projection trait, CompressedIndex, RecallEngine
    - Tools: OBS Studio with IDE zoom, dark theme, large font
    - Upload to YouTube, embed on `/docs/prime/concepts`

16. [ ] **Interactive playground component** — code task (can be done by Claude)
    - React component at `/dashboard/demo/prime/` with Cytoscape.js graph
    - Calls `allsource-prime.fly.dev` HTTP API
    - Pre-seeded with demo data

17. [ ] **5 docs hero banners** — NEEDS HUMAN: design matching brand
    - 1200x400px, dark theme, one per docs/prime/* page
    - Place in `/apps/web/public/docs/`

18. [ ] **JSON-LD structured data** — code task (can be done by Claude)
    - Add `Organization` schema to homepage
    - Add `SoftwareApplication` schema to `/solutions/agent-memory`
    - Add `TechArticle` schema to docs pages

19. [ ] **Benchmark publication visuals** — NEEDS HUMAN after benchmarks run
    - Bar chart: cross-domain recall comparison (5 systems)
    - Latency chart: 12μs vs 70ms vs 200ms+
    - Feature matrix card for social sharing

20. [ ] **Code screenshots for blog posts** — NEEDS HUMAN (optional)
    - Use ray.so or carbon.now.sh for polished code screenshots
    - Replace text code blocks in blog posts with images for social sharing

---

## Redeploy Checklist

After creating the missing assets, redeploy the web app:

1. [ ] Add `dashboard.png` to `/apps/web/public/`
2. [ ] Add `author.jpg` to `/apps/web/public/`
3. [ ] `git add . && git commit -m "assets: dashboard screenshot + author avatar"`
4. [ ] `git push origin main` — Vercel auto-deploys
5. [ ] Verify with lightpanda: `lightpanda fetch --dump markdown https://www.all-source.xyz/blog/zer0dex-vs-allsource-recall`
6. [ ] Verify OG images: paste URL in https://opengraph.xyz to check preview
7. [ ] Verify sitemap: `curl https://www.all-source.xyz/sitemap.xml`
8. [ ] Verify robots.txt: `curl https://www.all-source.xyz/robots.txt`
9. [ ] Submit sitemap to Google Search Console
10. [ ] Submit sitemap to Bing Webmaster Tools
