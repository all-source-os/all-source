# Runbook — GEO measurement

**Status:** layers 1, 2, 3 and 4 are built. `apps/web` emits
`geo.referral.observed` for arrivals from AI surfaces, `geo crawl` ingests
verified AI-bot hits from the edge logs as `geo.crawl.observed`, `geo probe`
runs the frozen layer-3 probe set across the engines, and both signup paths
(the onboarding form and `POST /api/v1/onboard/start`) capture
`geo.selfreport.captured`. **Layer 3 has not yet run live** — no provider API
key exists in the environment it was built in, so its baseline documents are
committed as empty templates. Layer 5 is a stub.

**Scope:** how AllSource shows up in generative engines (ChatGPT, Claude,
Perplexity, Gemini) — who arrives from them, which of their crawlers read the
site, what they say about us, and whether a change moved any of it.

- Event contract: [`docs/contracts/geo-events/README.md`](../contracts/geo-events/README.md)
- Tooling: [`tooling/geo/`](../../tooling/geo) — `geo-core` (types + emitter), `geo-cli` (the `geo` binary)

## Why this exists

An increasing share of AllSource's buyers never see a blue link. They ask an
assistant what to use for agent memory and act on the answer. `apps/web` has no
analytics, no crawl-log pipeline and no record of what the engines say about
us — so every GEO decision to date has been a guess. This program replaces the
guessing with a durable, queryable timeline in Core.

## The five layers

| layer | question it answers | event | status |
|---|---|---|---|
| 1 — Referral attribution | Who arrived here from an AI surface? | `geo.referral.observed` | **live** (prompt 024) |
| 2 — Crawl diagnostics | Which AI crawlers read the site, and did they get a 200? | `geo.crawl.observed` | **live** (prompt 024) |
| 3a — Share of voice | Do the engines name us when asked our category question? | `geo.sov.probed` | **built** (prompt 025), awaiting a live sweep |
| 3b — Interrogation | When they name us, is what they say true? | `geo.interrogation.probed` | **built** (prompt 025), awaiting a live sweep |
| 4 — Self-report | What do people say sent them, when the referrer was stripped? | `geo.selfreport.captured` | **live** (prompt 026) |
| 5 — Experiments | Did a change to a surface move any of the above? | `geo.experiment.started` / `.scored` | not implemented (prompt 027) |

Layers 1 and 2 are observational and cheap. Layer 3 costs LLM calls. Layer 4 is
the only one that survives a stripped referrer. Layer 5 closes the loop.

The layer-3 section is at the bottom of this file.

## Getting an API key

GEO telemetry is written through the **Control Plane gateway**
(`https://api.all-source.xyz`), never to Core directly — Core does not
authenticate public traffic. You need a gateway API key with ingest rights.

Either mint one in the dashboard, or self-serve:

```bash
curl -X POST https://api.all-source.xyz/api/v1/onboard/start \
  -H 'Content-Type: application/json' \
  -d '{"email": "you@example.com", "name": "GEO measurement"}'
```

Then:

```bash
export ALLSOURCE_API_KEY=<key>
export ALLSOURCE_API_URL=https://api.all-source.xyz   # optional; this is the default
```

`ALLSOURCE_API_KEY` is required for anything that touches the gateway. Nothing
in this tooling degrades to a silent no-op when it is missing — a GEO window
with an unnoticed hole in it is worse than a job that fails.

## Building

`tooling/geo` is its own Cargo workspace with its own `Cargo.lock`, excluded
from the root workspace so it can never pull `apps/core` into a tooling build.

```bash
cd tooling/geo
cargo build --release          # binary at tooling/geo/target/release/geo
cargo test
cargo clippy --all-targets -- -D warnings
```

## Running `geo report`

Count `geo.*` events per layer over a window.

```bash
# last 30 days (default), live against the gateway
./tooling/geo/target/release/geo report

# an explicit window
./tooling/geo/target/release/geo report \
  --since 2026-07-01T00:00:00Z --until 2026-08-01T00:00:00Z

# no key, no network: prints the envelopes the emitter would POST,
# then tallies them
./tooling/geo/target/release/geo report --dry-run
```

Flags: `--days` (default 30, ignored when `--since` is given), `--since`,
`--until` (RFC 3339, UTC), `--max-events` (default 100000, guards a runaway
scan), `--page-size` (default 500).

### Reading the table

```
LAYER NAME                   EVENT TYPE                    EVENTS  ENTITIES  PRODUCED BY
1     Referral attribution   geo.referral.observed              1         1  prompt 026
```

**EVENTS vs ENTITIES is the important column pair.** Every observation's
`entity_id` embeds its idempotency key, so re-ingesting the same log window
appends a version to the same entity rather than creating a new one. `ENTITIES`
is therefore replay-safe and `EVENTS` is not — if `EVENTS` runs well ahead of
`ENTITIES`, something re-ingested a window.

An `unrecognised geo.* types` section means a newer producer is writing to the
stream than this binary knows about. Rebuild.

## The other subcommands

`geo probe` and `geo research` are declared but exit non-zero, each naming the
slice that implements it. That is intentional: the CLI surface is fixed now so
the layer slices can land without re-litigating the shape. `geo crawl` is
implemented — see the layer 2 section below.

## Emitting from a later slice

```rust
use geo_core::{EmitMode, GeoEmitter, GeoEvent, CrawlObserved};

let emitter = GeoEmitter::from_env(EmitMode::Live)?;   // or EmitMode::DryRun
emitter.emit(&GeoEvent::Crawl(CrawlObserved { /* ... */ })).await?;
```

The emitter is transport-thin — it knows nothing about layers, bots, prompt
sets or scoring. Keep it that way; that separation is what lets the layer
slices be built independently.

## Troubleshooting

**`ALLSOURCE_API_KEY is not set`** — expected when no key is exported. Export
one, or use `--dry-run`.

**Report shows zeros everywhere** — expected until a layer slice ships. There
is nothing writing `geo.*` events yet.

**`docs/contracts/geo-events/examples/… has drifted from geo-core`** — a
payload type changed without regenerating the committed examples:

```bash
cd tooling/geo && cargo test -p geo-core -- --ignored regenerate_contract_examples
```

Then re-read the diff before committing — it is the contract changing, and the
contract doc's field tables need the same edit.

## Appending to this runbook

Each layer slice should add: how its ingest is triggered and on what schedule,
what it costs, and how to tell a bad run from an empty one.

---

# Layer 1 — Direct attribution (AI referrals)

**Question:** who arrived here from an AI surface, and did they convert?

## Read this before you read the number

**Layer 1 is a floor, not a measurement.** ChatGPT, Claude and most assistant
surfaces strip or omit the `Referer` header on outbound links. The majority of
AI-sourced sessions therefore arrive with an empty referrer and are
indistinguishable from Direct traffic. Published estimates of the undercount
vary, but every one of them is large: assume **the true number is a multiple of
what layer 1 reports**, not a small correction to it.

Practical consequences:

- A small `geo.referral.observed` count means *"the referrer survived rarely"*.
  It never means *"the channel is small"*. Do not size the AI channel from this
  layer alone.
- Movement is more trustworthy than level. A 3× rise week-on-week is real
  signal even though the absolute number is wrong.
- **Layer 2 (crawl) is the leading indicator**, not this one. Cloudflare's June
  2025 figures put OpenAI's crawl-to-referral ratio around 1,700:1 and
  Anthropic's around 73,000:1, versus Google's 14:1 — crawl volume moves months
  before referral volume does.
- **Layer 4 (self-report)** is the only layer that sees through a stripped
  referrer, and it is now live — read layer 1 next to it, never alone. The
  report prints the two side by side over a shared denominator; see the layer-4
  section at the bottom of this file.

`geo report` prints this caveat under the layer 1 table so it cannot be read
without it.

## How it works

```
browser  ──POST /api/geo/referral──▶  Next.js route handler  ──▶  Control Plane
(no key)                              (holds ALLSOURCE_API_KEY)      ──▶ Core
```

| piece | file |
|---|---|
| AI-surface map | `apps/web/src/lib/geo-referrers.ts` (`AI_SURFACES`) |
| envelope + idempotency key | same file (`buildReferralEnvelope`) |
| browser beacon | `apps/web/src/components/geo-referral-tracker.tsx` |
| server forwarder | `apps/web/src/app/api/geo/referral/route.ts` |
| analytics install + ADR | `apps/web/src/app/layout.tsx` |

The browser posts a **raw referrer**; it never says which surface it came from.
The route re-classifies server-side, so this endpoint cannot be used to write
arbitrary rows. **No AllSource key is reachable from the client bundle** — the
key is read from `process.env` inside the route handler at request time.

Two producers write `geo.referral.observed` — `apps/web` (TypeScript) and
`geo-core` (Rust). Their idempotency keys must agree byte-for-byte, or one
session would land in Core as two entities and every layer-1 count would
inflate. A vitest asserts the TypeScript envelope against the committed
contract example that the Rust emitter generates:

```bash
cd apps/web && bun run test   # src/__tests__/geo-referrers.test.ts
```

### Conversions

A conversion (`signup_started`, `api_key_minted`) re-emits the **same natural
key** as the arrival, so Core appends version 2 to the arrival's entity with
`converted: true`. The session is counted once; its latest version is its
current state. Call it from anywhere in the app:

```ts
import { reportGeoConversion } from "@/components/geo-referral-tracker";
reportGeoConversion("signup_started");   // no-op for non-AI sessions
```

It is already wired into both signup paths in `apps/web/src/app/(auth)/signup/page.tsx`.

## Analytics choice (ADR summary)

**PostHog Cloud EU** measures site-wide acquisition and product UX in
cookieless mode. Autocapture, person profiles, and session replay are disabled.
The first-party beacon above remains authority for AI-referral provenance and
durable product outcomes. See `docs/runbooks/POSTHOG_ANALYTICS.md` and the ADR
comment in `apps/web/src/app/layout.tsx`.

## Environment variables you must set BY HAND

Server-only values are Fly secrets on `allsource-web`. Public PostHog values
are build arguments in `apps/web/fly.toml`.

| variable | scope | required | notes |
|---|---|---|---|
| `ALLSOURCE_API_KEY` | **server only** | yes | Gateway API key with ingest rights. Never prefix with `NEXT_PUBLIC_` — that would publish it in the browser bundle. |
| `ALLSOURCE_API_URL` | server only | no | Defaults to `https://api.all-source.xyz`. |
| `NEXT_PUBLIC_POSTHOG_KEY` | public build arg | yes | Public write-only project token. Never substitute a personal API key. |
| `NEXT_PUBLIC_POSTHOG_HOST` | public build arg | yes | `https://eu.i.posthog.com`. |

Same names as `tooling/geo` on purpose: one env-var scheme for the whole GEO
programme, not two.

### Verifying no key leaked into the bundle

```bash
cd apps/web
rm -rf .next
ALLSOURCE_API_KEY='as_CANARY_MUST_NOT_SHIP' bun run build
grep -r 'as_CANARY_MUST_NOT_SHIP' .next/          # must print nothing
```

(The string `ALLSOURCE_API_KEY` *does* appear in one client chunk — that is the
pre-existing onboarding UI rendering a copy-paste install command containing the
*user's own* key, not ours.)

## Troubleshooting

**Route returns 503 `GEO telemetry is not configured`** — `ALLSOURCE_API_KEY`
is unset in the Vercel environment. The route logs `[geo] ALLSOURCE_API_KEY is
not set` and refuses; it deliberately does not pretend to succeed.

**Route returns 204** — the arrival was not from a known AI surface. Normal:
the beacon fires on the first page view of every session.

**Arrivals but no conversions** — `reportGeoConversion` reads `sessionStorage`,
which is per-tab. A signup completed in a new tab loses the link. That is a
known undercount on top of the referrer undercount.

---

# Layer 2 — Crawl log diagnostics

**Question:** which AI bots read the site, how often, and were they really who
they said they were?

## The three categories, and why they are never blended

| category | what a rise means | examples |
|---|---|---|
| **Training crawlers** | infrastructure readiness — we are reachable by the corpus | `GPTBot`, `ClaudeBot`, `CCBot` |
| **Search indexers** | citation eligibility — we are in the answer-time index | `OAI-SearchBot`, `Claude-SearchBot`, `PerplexityBot` |
| **User-triggered fetchers** | real-time human demand — someone is asking about us *now* | `ChatGPT-User`, `Claude-User`, `Perplexity-User` |

Their volumes differ by orders of magnitude and they answer different
questions. `geo report` prints them separately and **has no all-bots total by
design**. If you find yourself adding one, stop.

The machine-readable taxonomy is `tooling/geo/geo-core/src/bots.rs`
(`BOTS`, versioned by `TAXONOMY_VERSION`), with each vendor's own documentation
URL cited next to the entry. `geo.crawl.observed` stores the `category` and the
`taxonomy_version` on the event, so a historical count keeps the categorisation
it was written with even after the taxonomy moves a bot.

## Verification is not optional

A `User-Agent` is a string the client chose. Scrapers routinely wear `GPTBot`
to get past a permissive robots.txt, so an unverified count is not a
measurement. Every hit is checked against the vendor's **own published IP
ranges**:

| range list | verifies |
|---|---|
| `https://openai.com/gptbot.json` | `GPTBot` |
| `https://openai.com/searchbot.json` | `OAI-SearchBot` |
| `https://openai.com/chatgpt-user.json` | `ChatGPT-User` |
| `https://claude.com/crawling/bots.json` | `ClaudeBot`, `anthropic-ai`, `Claude-SearchBot`, `Claude-User` |
| `https://www.perplexity.ai/perplexitybot.json` | `PerplexityBot` |
| `https://www.perplexity.ai/perplexity-user.json` | `Perplexity-User` |
| `https://developers.google.com/static/crawling/ipranges/special-crawlers.json` | `Google-CloudVertexBot` |

All seven publish the same JSON shape, so one parser covers them.

Some vendors publish nothing (Common Crawl, ByteDance, Meta, DuckDuckGo). Their
hits are honestly reported as **unverifiable**, never as verified, and are only
emitted with `--include-unverified` (stamped `verified: false`, and excluded
from every count in the report).

There are four distinct outcomes, and collapsing them into a boolean would hide
the one that matters:

| verdict | meaning | emitted? |
|---|---|---|
| `verified` | IP is in the vendor's published range | yes |
| `rejected` | vendor publishes ranges, IP is not in them — **someone is spoofing** | **never** |
| `unverifiable (vendor publishes no ranges)` | claim is uncheckable | only with `--include-unverified` |
| `unverifiable (range list unavailable)` | *our* fetch failed / `--offline` | only with `--include-unverified` |

A fetch failure is never reported as a spoof. That distinction is unit-tested,
because a bug there would make "our network broke" look like "the whole
internet is impersonating GPTBot".

## Running the ingest

```bash
cd tooling/geo && cargo build --release
GEO=./tooling/geo/target/release/geo

# dry run over a log-drain dump — categorised counts, no writes, no key needed
$GEO crawl --since 7d --dry-run --file /path/to/drain.ndjson

# live: write verified hits to the gateway
export ALLSOURCE_API_KEY=<key>
$GEO crawl --since 7d --file /path/to/drain.ndjson

# fully offline and deterministic, against the committed fixture + range snapshot
$GEO crawl --since 2026-07-06T00:00:00Z --until 2026-08-11T00:00:00Z --dry-run \
  --file tooling/geo/geo-cli/tests/fixtures/vercel-log-drain.ndjson \
  --ranges-dir tooling/geo/geo-cli/tests/fixtures/ranges --offline
```

Flags: `--since` (duration `7d`/`12h`/`4w` or RFC 3339), `--until`, `--days`
(default 7), `--file` (repeatable), `--vercel-project`, `--fly-app`,
`--format auto|json|clf`, `--aggregate hit|hourly|daily`, `--ranges-dir`,
`--offline`, `--include-unverified`, `--dry-run`.

### Where the logs come from

- **`--file`** is the workhorse and needs no credential. A **Vercel log drain**
  writes exactly the NDJSON this parses. This is the only source that gives a
  durable multi-week series, so stand the drain up.
- **`--vercel-project`** shells out to `vercel logs <project> --json`. Retrieval
  only — every decision about the bytes is made in Rust. `vercel logs` serves a
  short recent window, not history.
- **`--fly-app`** shells out to `fly logs -a <app> --no-tail --json`, for
  `api.all-source.xyz` (Control Plane). Its lines carry the access line inside
  `message`, which the parser falls through to Combined Log Format for.

Both vendor CLIs need you to be logged in (`vercel login`, `fly auth login`).
If a CLI is missing or unauthenticated the run fails loudly with the install
hint — it does not report zero.

### Aggregation: which level, and why

**Default is `--aggregate hit`: one event per bot hit.** Core is the database
(WAL + Parquet + DashMap, 469K events/sec); it is not the thing under strain,
and per-hit fidelity is what makes "which paths does `Claude-User` actually
fetch" answerable later. The machine noise argument does not apply either,
because `geo.*` lives in its own `geo:` entity namespace and no human reads the
raw stream — they read `geo report`.

Use `--aggregate hourly` or `--aggregate daily` for a wide backfill where you
want a trend and not a million rows. Aggregation is **declared in the payload**
(`aggregation`, `hits`, `window_end`), so a reader can never mistake one row
for one hit, and the bucketed rows' natural key includes the level and window —
so re-running at `--aggregate hit` over the same raw logs recovers per-hit
fidelity without colliding with them.

### Idempotency

Re-running the same window is safe. Each event's `entity_id` embeds its
idempotency key, so a replay appends a version to the same Core entity instead
of creating a second one — and `geo report` folds crawl rows **by entity, last
version wins**, so the hit counts do not double either.

Proven locally: three identical ingests of the same window produced 687 stored
events across **229 distinct entities**, and the reported hit counts
(185 / 27 / 17) were identical to a single ingest.

The natural key includes `request_id`, so two genuinely distinct hits in the
same second on the same path stay two entities rather than collapsing into one.

## Reading `geo report`

```bash
$GEO report --since 30d
```

Per category, per week: hit count, distinct paths, top paths, and a **rolling
4-week median**.

**A median, not a mean, on purpose.** A single overnight crawl burst — one
vendor re-reading the whole site in an hour — moves a 4-week mean by a quarter
and leaves the median untouched. In the committed fixture, week 4 is a 69-hit
burst against a ~28-hit baseline; the mean would read 38, the median reads 29.5.

The `event inventory` table at the top counts **stored rows, not bot hits**.
`EVENTS` vs `ENTITIES` diverging there means a window was re-ingested — which is
fine, and is exactly why the hit counts are folded by entity.

## Fixture and tests

`tooling/geo/geo-cli/tests/fixtures/` holds a five-week Vercel log-drain dump
(256 lines, including humans, two unreadable lines and a spoofed `GPTBot` from
`203.0.113.77`) plus a snapshot of all seven **real** vendor range lists. That
makes the whole pipeline testable offline and deterministically against the
vendors' actual published prefixes.

```bash
cd tooling/geo
cargo test
cargo clippy --all-targets -- -D warnings

# regenerate the fixture after changing the generator
cargo test -p geo-cli -- --ignored regenerate_crawl_fixture
```

## robots.txt

`apps/web/public/robots.txt` now names all three categories explicitly and
**allows all three**, with one paragraph of reasoning per category. Before this
slice the file said `User-agent: * / Allow: /` and named no AI bot: every AI
crawler was allowed *by accident*.

**If you change a decision, read the STRUCTURE NOTE at the top of the file
first.** A crawler obeys exactly one group — the most specific one naming it —
and a named group does **not** inherit the `User-agent: *` rules. The
closed-area `Disallow` list is therefore repeated verbatim in all four groups.
Add a path to one and you must add it to all four, or you will quietly open
`/dashboard/` to a named bot.

## Troubleshooting

**"0 verified" with a non-zero identified count** — the run prints a loud
warning for exactly this, because a verification bug looks identical to "no AI
traffic". Check the `vendor IP range lists` block in the output: if every list
says `UNAVAILABLE`, it is a fetch problem, not a traffic problem.

**"refusing to report a crawl number built from a fraction of the log"** — over
half the lines were unreadable. Check `--format` against the sample lines the
run prints.

**A vendor list looks stale** — the run prints each list's own `creationTime`.
Vendors update these irregularly; a months-old `creationTime` is normal, but a
sudden jump in `rejected` right after a vendor rotates ranges is not a spoofing
wave — refresh `--ranges-dir`.

---

# Layer 3 — what the engines say about us

**Status:** the harness is complete and fully exercised offline. **No live
sweep has run yet**, because no provider API key exists in the environment this
was built in. `docs/marketing/geo-baseline-<date>.md`, `geo-key-terms.md` and
`geo-remediation-backlog.md` are committed as empty templates for exactly that
reason — see "The first live baseline" below for the one command that fills
them.

Layer 3 is the keystone. Layers 1 and 2 tell you about traffic and crawlers;
layer 3 is the only layer that tells you **what the machine actually says about
you**, and it produces the scored metric that prompt 027's optimization loop
optimises against. Without a stable, reproducible score here, 027 is a loop with
no objective function.

Two deliberately separate jobs:

| | 3a — share of voice | 3b — interrogation |
|---|---|---|
| Question | Of the answers to buyer questions, what share name us? | When they name us, is what they say **true**? |
| Prompts | 32, no brand named | 17, all brand-named, 25 ground-truth claims |
| Event | `geo.sov.probed` | `geo.interrogation.probed` |
| Output | a trend instrument | a concrete content backlog |

## Read this before you read the number

**Share of voice alone is a vanity metric.** It tells you whether you are
appearing in answers, not whether anyone is buying anything. It is an *input*
to layer 5 and a trend instrument — never the KPI, never a number to put in a
deck on its own. Read it next to layer 1 (arrivals) and layer 4 (self-report).

The report prints that warning above the tables, on purpose: a warning that
lives only in a runbook is a warning nobody reads.

## The prompt set is frozen, and that is the whole point

`tooling/geo/prompts/sov.toml` and `interrogation.toml` are compiled into the
`geo` binary (`include_str!`) and their SHA-256 digest rides in every emitted
`run_id`:

```
sov-2026-08-11-507348cd#r1
└f┘ └── date ──┘ └digest┘ └repetition
```

Two sweeps are comparable **if and only if** their run ids carry the same
digest. Prompt 027 will change the *site*; if the prompt set drifts at the same
time, the experiment is uninterpretable. Freezing the set is what makes the
metric a metric.

**Editing the set is allowed and starts a new baseline.** If you edit it: bump
`version` in the file, note what changed and why in this runbook, and do not
edit it in the same week as a site change.

Interrogation version history:

- **v1** — initial 13-prompt, 19-claim factual set.
- **v2, 2026-08-14** — new baseline for canonical product identity. Added the
  exact qualified-identity, ArcGIS-disambiguation, Core/Prime/Hosted/MCP product
  map, and canonical-source questions. Updated resolved GBP, MCP-count, Prime,
  and machine-readable facts. SOV remains frozen at v1. V2 was authored with
  the site deployment but is not used for a comparative score until recrawl;
  its digest starts a new baseline rather than extending v1. The immediate
  post-deploy evidence that motivated this version is preserved in
  [`geo-answer-engine-sweep-2026-08-14.md`](../marketing/geo-answer-engine-sweep-2026-08-14.md).

The `#r<n>` suffix is load-bearing too. The contract's natural key for a probe
is `run_id + engine + prompt_id`, so without the repetition in the id, the N
samples of one prompt would collapse into N *versions of one entity* — and a
reader folding by entity (which is what makes a re-ingest safe) would see one
sample where N were taken. The whole point of repeating is to keep the
distribution.

## Sample size and confidence

- **Minimum 3 repetitions per prompt per engine** (`--repetitions`, default 3).
  Below three there is no distribution and the intervals are useless. Raise it
  once you know what a sweep costs.
- **Every rate is a 95% Wilson score interval**, never a point estimate. The
  type has no "just the percentage" accessor. Wilson rather than the normal
  approximation because our n is small and our p is near zero — exactly where
  the normal approximation gives a zero-width interval at 0/n. A 0-of-12 run is
  not "0% ± 0", it is "0%, and anything up to ~24% is consistent with what we
  saw".
- **Overlapping intervals have not moved.** Two cells whose intervals overlap
  are not different, however different their percentages look.
- **Minimum observation window before publishing a claimed relationship between
  a change and a score: 12 weeks.** Anything shorter is noise with a slope.
- **SOV is reported per intent class and never blended.** Being named on
  "alternatives to Mem0" and on "my agent keeps forgetting" are different wins.

## Scoring

- `payload.score` on a SOV probe is **reciprocal rank**: `1/rank` among every
  product named, `0` when we are absent. Rank 1 → 1.0, rank 4 → 0.25.
- **Mention rate, not that score, is the headline.** A single score that folds
  presence and position together is convenient and unfalsifiable; the report
  prints both and 027 optimises against mention rate.
- **Competitor share uses the same denominator**, so share of voice is relative
  rather than absolute. "Plain files" is in the competitor table on purpose: the
  most common answer to "how do I give my agent memory" is often a JSON file,
  and counting only funded products makes the category look far more contested,
  and far more commercial, than it is.

## Accuracy, and why the judge's reasoning is stored

Accuracy cannot be string-matched — "£18.99/month" and "about nineteen pounds a
month" are the same answer. So an LLM-as-judge grades one claim at a time
against a ground-truth expectation extracted from this repository, and returns
one of:

| verdict | means | counts as |
|---|---|---|
| `accurate` | states the claim correctly | 1.0 |
| `partially_accurate` | right in substance, wrong in a detail a buyer notices | 0.5 |
| `inaccurate` | states something false | 0.0, goes in the backlog |
| `absent` | does not address the claim, or says it does not know | 0.0, reported separately — **silence is not wrongness**, and the fix is different (write the page, versus correct the page) |
| `unscored` | the judge's own reply could not be read | excluded from every denominator |

**Every verdict is stored with the judge's reasoning and a verbatim excerpt**
(`payload.reasoning`, `payload.answer_excerpt`) so a human can overrule it. The
parser *refuses* a verdict with no reasoning, or a non-`absent` verdict that
quotes nothing — a verdict nobody can audit must never enter an accuracy rate.

**Known conflict of interest:** the judge runs on Claude and one of the probed
engines is Claude. Sanity-check the `claude` column by hand.

## Running it

```bash
cd tooling/geo && cargo build --release
GEO=./target/release/geo

# No key, no network, no spend — the CI path. Exercises scoring, the judge
# parser, event construction and idempotency against committed fixtures.
$GEO probe --dry-run

# Cheap live smoke: 5 prompts, one engine, one sample each.
$GEO probe --family interrogation --limit 5 --engine claude --repetitions 1

# The real baseline sweep.
$GEO probe --repetitions 3 --markdown-out docs/marketing/geo-baseline-$(date -u +%F).md

# Read the stored stream back (any window), and write the same section out.
$GEO report --since 30d --markdown-out docs/marketing/geo-baseline-$(date -u +%F).md
```

Useful flags: `--family sov|interrogation`, `--engine <id>` (repeatable),
`--limit N`, `--repetitions N`, `--concurrency N` (per engine), `--no-judge`
(3a-only, no judge spend).

## Environment variables, and the `.env` file

The `geo` CLI loads a `.env` through `dotenvy`, searching **upward from the
working directory**, so the repository-root `.env` is found whether you run from
the repo root or from `tooling/geo`.

**Real process environment variables always win over `.env`.** The loader is
non-overriding (`dotenvy::dotenv()`, one call site, asserted by a test), so
`ANTHROPIC_API_KEY=sk-... geo probe` beats whatever is in the file. Getting that
backwards would let a stale `.env` silently shadow a deliberate inline override
during a live sweep. A missing `.env` is not an error.

`.env` is gitignored (`.gitignore:22`). `.env.example` at the repository root
lists every variable with empty values.

| variable | needed for | notes |
|---|---|---|
| `ALLSOURCE_API_KEY` | any write or read of `geo.*` events | not needed for `--dry-run` |
| `ALLSOURCE_API_URL` | — | optional, defaults to `https://api.all-source.xyz` |
| `OPENAI_API_KEY` | the `chatgpt` engine | missing ⇒ engine skipped loudly |
| `ANTHROPIC_API_KEY` | the `claude` engine **and the layer-3b judge** | missing ⇒ every claim comes back `unscored` |
| `GEMINI_API_KEY` | the `gemini` engine | |
| `PERPLEXITY_API_KEY` | the `perplexity` engine | |
| `GEO_CHATGPT_MODEL` etc. | overriding a model id | see below |
| `GEO_JUDGE_MODEL` | overriding the judge | defaults to `claude-opus-5` |

**A missing key is a loud skip, never a zero.** A silently skipped engine reads
downstream as "we lost all our share on Gemini" and would send prompt 027
chasing a phantom. The run prints the skip, and the report carries an "Engines
skipped" section saying in words that absence is not zero share.

**Verify the model ids before the first live sweep.** Vendor model names churn
faster than this repository does; the defaults are a starting point, not a
guarantee. `geo probe` prints the model it will call for each engine before it
calls anything.

## Cost

The run prints a spend table. **Only Anthropic's rates are claimed** — that is
the one rate card this repository has a maintained source for (the `claude-api`
skill: Opus 5 at $5/$25 per MTok, as of 2026-06-24). Every other engine reports
token counts and the literal word `unpriced`. A fabricated price on a spend
report is worse than no price.

Sweep size: `prompts × engines × repetitions`, plus one judge call per claim per
interrogation answer.

| sweep | probe calls | judge calls |
|---|---|---|
| full, 4 engines, 3 reps | (32 + 17) × 4 × 3 = **588** | 25 × 4 × 3 = **300** |
| SOV only, 4 engines, 3 reps | 384 | 0 |
| smoke (`--limit 5 --engine claude --repetitions 1`) | 5 | ~7 |

Use `--limit` and `--family` for a cheap smoke run before committing to a full
sweep, and `--no-judge` to price 3a on its own.

### Estimated cost of the first full baseline

**Not measured — no live sweep has run.** Worked from the sweep sizes above and
the only rate card this repository maintains. State the assumptions when you
quote it:

- Probe input ≈ 40 tokens/call (the prompts are one or two sentences).
- Probe output ≈ 1,500 tokens/call on Claude (thinking is on by default on
  Opus 5 and is billed as output), ≈ 600 on the others.
- Judge input ≈ 1,300 tokens/call (instructions + question + the whole answer +
  the expectation); judge output ≈ 750.
- `MAX_OUTPUT_TOKENS` is 4,000, so the worst case is ~2.7x the estimate below.

| line | calls | est. cost |
|---|---|---|
| `claude` engine (Opus 5, $5/$25 per MTok) | 147 | ~$6 |
| judge (Opus 5, 25 claims × 4 engines × 3 reps) | 300 | ~$8 |
| `chatgpt` / `gemini` / `perplexity` | 441 | **unpriced here** — roughly $9–13 at typical frontier list rates, but this repository has no maintained source for them and the tool will not guess |
| **first full baseline** | 888 | **~$25–35** |

A smoke run (`--limit 5 --engine claude --repetitions 1`) is under $0.20.
Setting `GEO_CLAUDE_MODEL` and `GEO_JUDGE_MODEL` to `claude-haiku-4-5` cuts the
Anthropic share roughly fivefold, but a cheaper judge is a worse judge — fine
for rehearsing the pipeline, not for the baseline everything else is compared
against.

## The first live baseline

The three files under `docs/marketing/` that prompt 027 consumes are committed
as **empty labelled templates**. They are filled by one command:

```bash
export OPENAI_API_KEY=... ANTHROPIC_API_KEY=... GEMINI_API_KEY=... PERPLEXITY_API_KEY=...
export ALLSOURCE_API_KEY=...
cd tooling/geo && cargo build --release
./target/release/geo probe --repetitions 3 \
  --markdown-out ../../docs/marketing/geo-baseline-$(date -u +%F).md
```

Then copy the "Observed vocabulary" table into `geo-key-terms.md` and the
"Remediation backlog" table into `geo-remediation-backlog.md`, both with the run
id and date. **Do not hand-write any of it.** Those files are 027's experiment
queue; a plausible-looking invented row there is worse than an empty file,
because nothing downstream can tell the difference.

## Ground-truth contradictions found while writing the probe set

Grounding the 25 version-2 interrogation claims meant reading what this repository
actually says, and it does not always agree with itself. These are findings
about **us**, not about any model, and several expectations in
`interrogation.toml` are deliberately written to accommodate them — a model that
repeats one of these is quoting our own file:

1. **Licence.** The repository root `LICENSE` is Apache-2.0 and `LICENSE-BSL`
   covers enterprise features, but `apps/core/LICENSE` still says MIT (a
   deliberate carve-out per `MEMORY.md`), as do the published Rust and
   TypeScript SDK manifests. A model answering "MIT" is reading a real file, so
   the claim grades that as `partially_accurate` rather than a clean miss.
2. **Machine-readable product facts — resolved 2026-08-14.** `llms.txt` now
   states current GBP catalogue prices, exact configuration-dependent MCP tool
   counts, storage architecture, and benchmark scope.
3. **MCP tool count — resolved on canonical public surfaces 2026-08-14.** Use
   45 read-only, 55 default read/write, 64 with control-plane access, and 73 with
   system administration. Prime is separate: 19 `prime_*` memory tools and 27
   tools across its full optional registry.
4. **Currency — resolved on canonical public surfaces 2026-08-14.** Config
   fallbacks mirror the verified GBP catalogue. Visible and structured paid
   offers still come from the live billing endpoint; schema offers are omitted
   when that endpoint is unavailable.
5. **Latency — resolved on canonical public surfaces 2026-08-14.** The 11.9µs
   p99 figure is a Core indexed-read reference benchmark, not end-to-end Prime
   graph/vector recall. Public answer pages now state that boundary and remove
   the sub-microsecond overclaim.
6. **Hosted MCP release gap — open.** Latest published connector v0.22.0 cannot
   attach the gateway Authorization header. The fix exists on main for v0.23.0
   but needs a release before stable hosted-MCP setup instructions can work.

These are recorded here rather than in the remediation backlog because the
backlog is reserved for what a *live* run observes.

## Product-identity interrogation

Bare-name prompts currently collide with Esri ArcGIS AllSource, the
all-source-intelligence discipline, and unrelated companies. Correct-entity
answers can still merge Core, Prime, hosted services, and MCP or repeat stale
tool counts. Treat these as separate failures: identity, category, boundary,
facts, and freshness.

Use
[`docs/marketing/GEO_AEO_ENGINE_QUESTION_CHECKLIST.md`](../marketing/GEO_AEO_ENGINE_QUESTION_CHECKLIST.md)
for copy-paste prompts and scoring. Keep this interrogation separate from a
frozen share-of-voice run: changing prompts and site copy together destroys
comparability. Run product-identity questions in fresh conversations, retain
raw answers and citations, then version the formal prompt set before its next
baseline.

Expected canonical entity:

- name: **AllSource Event Store**;
- domain: `www.all-source.xyz`;
- repository: `github.com/all-source-os/all-source`;
- category: developer event-store infrastructure with a bounded agent-memory
  layer;
- product map: Core stores, Prime remembers, hosted services operate, MCP
  connectors connect;
- disambiguation: unrelated to ArcGIS AllSource and all-source intelligence.

---

# Layer 4 — self-report

**Question:** what do people say sent them, when the referrer was stripped?

Layer 4 is the correction for layer 1. It is the only layer that sees through
a stripped referrer, the only one that connects an AI answer to a real human
who signed up, and — uniquely — the only one that captures **the actual
question they asked the model**. That free text is first-party buyer language;
no probe harness can synthesise it, and it is what prompt 027 folds back into
the SOV prompt set.

## The gap is the finding

The sharpest empirical result in this whole framework is that **self-reported
attribution routinely lands in double digits while analytics attribution reads
under 1%**. That is not two measurements disagreeing; it is one measurement
(layer 1) being structurally blind. `geo report` therefore prints both numbers
**on the same lines, over the same denominator**, because a gap between two
differently-normalised percentages is an arithmetic accident, not a finding:

```
== LAYER 1 vs LAYER 4 — the attribution gap ========================

   analytics (layer 1, referrer survived) :     0 of 1    0.0%
   humans    (layer 4, they told us)      :     1 of 1  100.0%
```

The shared denominator is *signups that answered the question*. The two rows
are still not the same measurement — the top counts sessions whose AI referrer
survived **and** that were seen to convert; the bottom counts people who said
so — and the report says that in words underneath, every time. Neither row is
a channel size: the top is a floor, the bottom is a sample of the people who
chose to answer an optional question.

Where layer 4 captured signups from `POST /api/v1/onboard/start`, the report
also prints the **web-only slice**. Layer 1 structurally cannot see the API
path — an agent calling the endpoint never had a browser session to strip a
referrer from — so including those rows on the top line would overstate the
gap.

## Both signup paths, or the number is a lie

| path | where | capture-path id |
|---|---|---|
| web | first onboarding screen, `apps/web/src/components/geo-discovery-question.tsx` | `signup-form` |
| API | optional fields on `POST /api/v1/onboard/start` | `onboard-api` |

**The API path is not optional scope.** That endpoint is published in
`apps/web/public/llms.txt` and agents sign up through it without ever loading
the site. A capture that covered only the browser form would systematically
miss exactly the AI-native users this whole programme exists to measure —
while looking complete.

### The web capture, and why it cannot cost a signup

It is on the **first screen after signup**, not in the signup form. The account
already exists by then, so the question cannot cost a conversion, and the
tenant it attaches to already exists too. One click, skippable, no gate on
continuing. A signup funnel that drops 5% to gain attribution is a bad trade.

The free-text box ("what did you ask it?") only appears for the AI options.
Asking someone who arrived from Hacker News what they "asked it" is nonsense,
and a nonsense question costs completion rate on the one question we get.

**Two beats, one entity.** The source is recorded the instant it is clicked, so
a user who then walks away still counts; the free text follows and *replays the
first answer's `observed_at`*, so it derives the same natural key and Core
appends version 2 to the same entity. One signup, not two — the same mechanism
layer 1 uses for a conversion. The route only honours a replayed timestamp
within an hour.

### The API capture

```bash
curl -X POST https://api.all-source.xyz/api/v1/onboard/start \
  -H 'Content-Type: application/json' \
  -d '{"email":"you@example.com","name":"My Agent",
       "discovery_source":"chatgpt",
       "discovery_prompt":"what should I use to give my agent long-term memory?"}'
```

**Backward compatibility is non-negotiable and is pinned by a test.** Both
fields are optional; a caller that omits them gets the same status, the same
response keys and the same values as before, and no event is written. An
unrecognised `discovery_source` is *ignored*, never rejected — the endpoint's
job is to mint a tenant, and a typo in an optional telemetry field must not
cost somebody their signup. See `TestOnboardRequestBindsWithoutTheGeoFields`
and `TestBuildGeoSelfReport` in `apps/control-plane/geo_selfreport_test.go`.

## One vocabulary, three languages

The canonical discovery-source list is
[`tooling/geo/geo-core/src/discovery.rs`](../../tooling/geo/geo-core/src/discovery.rs),
serialised to
[`docs/contracts/geo-events/discovery-sources.json`](../contracts/geo-events/discovery-sources.json).

| side | file | its check |
|---|---|---|
| web form + route handler | `apps/web/src/lib/geo-discovery-sources.ts` | `src/__tests__/geo-discovery-sources.test.ts` |
| `/onboard/start` validation | `apps/control-plane/geo_selfreport.go` | `TestDiscoverySourcesMatchTheContract` |
| `geo report` | `geo-core` | `tests/discovery_contract.rs` |

Three copies because the three cannot import each other — three languages, and
the monorepo isolation rule. The failure this guards against is quiet: if the
form wrote `"ChatGPT"` and the API path wrote `"chatgpt"`, the report would
show two channels where there is one and the **AI-sourced share would be
silently halved**. All three sides also derive the idempotency key with the
same SHA-256-over-`0x1f`-joined-parts scheme, so one capture can never land as
two entities.

Regenerating after a vocabulary change:

```bash
cd tooling/geo && cargo test -p geo-core -- --ignored regenerate_discovery_contract
# then mirror the diff into the .ts and .go files; their tests fail until you do
```

**An `id` is never renamed** — a rename splits a historical series in two with
no way to stitch it back. Add entries; do not re-letter them. `other-ai` counts
as AI on purpose: an assistant we have not named is still an assistant, and
excluding it would bias the correction this layer *is*.

An id this binary's vocabulary does not know is **never counted as AI**; the
report names it under "discovery sources this binary's vocabulary does not
know" instead. A newer producer must not retroactively inflate the AI share of
an old window read by an old binary.

## Revenue, not just traffic

`tier` rides on the event, stored at capture rather than joined at read time —
a tenant's tier moves, and "what tier did AI-sourced signups start on" is a
question about the past. The report prints the AI-sourced share of **paid**
signups, where paid means `indie | studio | scale | enterprise`. `trial` is not
revenue and `self-host` is not billed, so neither counts. A capture with no
tier is excluded from *both* sides of that split and reported separately —
counting it as unpaid would understate the number, and inflating it is the one
error here that would end up in a deck.

## Privacy

**Where it is stored.** In Core, as `geo.selfreport.captured` events under the
`geo:selfreport:` entity namespace, in the GEO tenant — the same durable
WAL + Parquet store as every other `geo.*` event. Not in PostgreSQL, and there
is no CRM in this stack; Core is the system of record.

**What is stored.** The selected option, the free text verbatim (bounded to 500
characters), the tenant id, the tier at capture, the capture path, and the
timestamp.

**`contact_ref` is the tenant id and never an email address.** GEO telemetry is
a trend timeline, not a place to accumulate PII. Do not extend the payload with
identifying fields — the contract says so and a test asserts the canonical
example has no `@` in it.

**It is operator-visible, and we say so.** `verbatim` is user-submitted free
text that our team reads — that is the entire point of collecting it. §2.3 of
the privacy policy promises we do not read *your event data*; this is our own
telemetry in our own tenant and is a different thing, so it gets its own
clause rather than hiding under "usage data". `apps/web/src/app/(marketing)/privacy/page.tsx`
**§2.5 "Attribution Answers You Choose to Give Us"** was added by this slice
and covers: that both fields are optional, exactly what is retained, that it is
read by us, that it is never sold or used for advertising, and that it is
deleted on request. If you change what is captured, change §2.5 in the same
commit.

## Environment

The web capture needs `ALLSOURCE_API_KEY` in the **Vercel** environment — the
same key layer 1 already uses, one env-var scheme for the whole programme. The
route refuses with 503 and logs loudly when it is missing rather than
pretending to succeed. It reads the tenant from the httpOnly `auth_token`
cookie and the tier from the Query Service (`QUERY_SERVICE_URL`); a tier lookup
failure degrades to `tier: null`, never to a lost capture.

The API capture writes through the Control Plane's own Core client, so it needs
no key — but it **does** need one variable:

| variable | where | default | why |
|---|---|---|---|
| `GEO_TENANT_ID` | Control Plane (Fly) | `geo` | The tenant GEO telemetry lands in |

**This must be the same tenant the `ALLSOURCE_API_KEY` belongs to.** Everything
else in the programme writes through the gateway, which injects the *key's*
tenant. The Control Plane has no key, so it names the tenant explicitly. If the
two disagree, web captures and API captures land in two different tenants and
`geo report` — which reads the one tenant its key belongs to — shows one of
them. That reads as "the API path captures nothing", which would be both wrong
and expensive.

It is emphatically **not** the signing-up tenant. Writing our telemetry into a
customer's event stream would put our operational data in their tenant, where
they pay for it, see it and can delete it, and would scatter the `geo.*` family
across every tenant that ever signed up. The join back to the signup is
`payload.contact_ref`, which is what that field is for.

Deploying it: `fly deploy` for the Control Plane runs with build context
`apps/control-plane`, **not** the repo root (Core is the one that builds from
root). `apps/web` ships via Vercel and must never be `fly deploy`ed.

## Troubleshooting

**Layer 4 is empty but people are signing up** — check in this order: the
route returns 503 (no `ALLSOURCE_API_KEY` in Vercel); the route returns 401
(the onboarding screen loaded without a session cookie); or nobody clicked. The
report distinguishes none of these, so read the function logs.

**Captures with `(unknown)` as the discovery source** — a producer wrote a
`surface` this binary does not know. Rebuild `geo`; if it persists, something
is writing outside the vocabulary and its check has been bypassed.

**The count looks too high** — check `EVENTS` vs `ENTITIES` in the inventory
table. The report folds self-reports by entity (last version wins), so a replay
cannot inflate it, but a *different* natural key can: the key is
`observed_at`(s) + capture path + source + tenant, so a user who answers twice
more than an hour apart is genuinely two captures.

**`discovery_source` was sent but no event appeared** — the value was not in
the vocabulary and was silently ignored by design. Check it against
`docs/contracts/geo-events/discovery-sources.json`; the ids are lowercase kebab
(`x-twitter`, not `X/Twitter`).
