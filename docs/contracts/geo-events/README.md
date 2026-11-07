# GEO event contract (`geo.*`)

Source of truth for the durable Core events the GEO (Generative Engine
Optimization) measurement program emits for `www.all-source.xyz`.

An increasing share of AllSource's buyers will never see a blue link — they ask
ChatGPT, Claude, Perplexity or Gemini *"what should I use to give my agent
memory?"* and act on the answer. Before this contract there was no
instrumentation for that channel at all. Every layer of the measurement program
writes into this one queryable timeline instead of into five disconnected
scripts.

Producer today: [`tooling/geo`](../../../tooling/geo) (`geo-core` types +
emitter, `geo` CLI). The layer slices listed below fill in the callers.

## Why a contract, not just types

`geo-core` is the only producer *today*, but the report reader, the
optimization loop and anything that later reads this stream out of Core are
separate consumers with their own release cadence. The contract is the
authority; the Rust types in
[`tooling/geo/geo-core/src/events.rs`](../../../tooling/geo/geo-core/src/events.rs)
are derived from it, never the reverse.

Drift is mechanically prevented: the JSON in [`examples/`](examples/) is
**generated from the emitter**, and
[`geo-core/tests/contract_examples.rs`](../../../tooling/geo/geo-core/tests/contract_examples.rs)
fails if a committed example and the real serialisation disagree. Regenerate
after changing a payload type:

```bash
cd tooling/geo && cargo test -p geo-core -- --ignored regenerate_contract_examples
```

This is deliberate. gh#250 burned us with mocks that faithfully honoured a
contract the server did not implement — so the fixtures here are the emitter's
own output, not a hand-typed approximation of it.

## Envelope

Every `geo.*` event is POSTed to `POST /api/v1/events` **through the Control
Plane gateway** (`https://api.all-source.xyz`). Core does not authenticate
public traffic; the gateway validates the API key or registered client
assertion ([decision](../../decisions/2026-11-07-tenant-clients-registered-es256-keys.md)),
injects `tenant_id` and forwards.

| field | who sets it | rule |
|---|---|---|
| `event_type` | emitter | const — one of the seven below |
| `entity_id` | emitter | the dedupe / lifecycle key (see below) |
| `payload` | emitter | per-event (see the tables below) |
| `metadata` | emitter | `{"emitter": "tooling/geo", "idempotency_key": "<hex>"}` |
| `tenant_id` | Control Plane | injected at the gateway; **never client-trusted** |

Core assigns `id`, `timestamp` and the per-entity monotonic `version`
server-side — they are not part of the ingest body. Core is the database (WAL +
Parquet + DashMap): these events are durable across restarts, which is what
makes a 12-week trend window possible at all.

### `entity_id`

Every GEO entity id is prefixed `geo:` so GEO telemetry is trivially separable
from — and can never be confused with — a customer tenant's entity stream.

- **Observations** (`referral`, `crawl`, `sov`, `interrogation`, `selfreport`)
  → `geo:<namespace>:<idempotency_key>`. One entity per observation.
- **Experiments** (`started`, `scored`) → `geo:experiment:<experiment_id>`.
  One entity per experiment, so its lifecycle is one ordered stream and
  `?entity_id=geo:experiment:<id>&order=desc&limit=1` yields current state.

### Idempotency

Each event type has a *natural key* — the tuple of facts that makes it the same
observation rather than a new one. `geo-core` hashes that tuple (SHA-256,
first 128 bits, hex) into `metadata.idempotency_key`, which is also the
`entity_id` suffix for observations.

Because the key lands in the `entity_id`, **re-ingesting the same log window
appends a version to the same Core entity instead of creating a second one**.
Counting distinct entities is therefore replay-safe; counting raw events is
not. `geo report` prints both columns for exactly this reason.

Timestamps that feed a key are truncated to whole seconds, so sub-second jitter
between two reads of the same log line cannot split one observation into two.
Payloads keep full precision.

| event | natural key |
|---|---|
| `geo.referral.observed` | `observed_at` (s) + `surface` + `landing_path` + `session_id` + `referrer_url` |
| `geo.crawl.observed` | `observed_at` (s) + `bot` + `path` + `status` + `source` + `aggregation` + `request_id` + `window_end` (s) |
| `geo.sov.probed` | `run_id` + `engine` + `prompt_id` (the `run_id` carries the prompt-set digest **and** the repetition — see below) |
| `geo.interrogation.probed` | `run_id` + `engine` + `prompt_id` + `claim_id` |
| `geo.selfreport.captured` | `observed_at` (s) + `source` + `surface` + `contact_ref` |
| `geo.experiment.started` | `experiment_id` + `iteration` + `"started"` |
| `geo.experiment.scored` | `experiment_id` + `iteration` + `metric` + `"scored"` |

### `schema_version`

Every payload carries `schema_version` (currently `1`). Later slices add fields
**additively** at version 1. Bump only when an existing field changes meaning
or type, so a replay over a mixed-version stream can branch on it.

### Time

Every timestamp is UTC, RFC 3339. GEO is trend analysis over multi-week
windows and a timezone slip silently ruins one, so nothing in this contract is
local time. The CLI accepts an offset (`+02:00`) and normalises it; it refuses
a timestamp with no offset at all.

## Events

| event | entity_id | meaning | layer | produced by | example |
|---|---|---|---|---|---|
| `geo.referral.observed` | `geo:referral:<key>` | a human session arriving from an AI surface | 1 | prompt 024 | [ex](examples/geo.referral.observed.json) |
| `geo.crawl.observed` | `geo:crawl:<key>` | a verified AI bot hit | 2 | prompt 024 | [ex](examples/geo.crawl.observed.json) |
| `geo.sov.probed` | `geo:sov:<key>` | one scored share-of-voice probe result | 3a | prompt 025 | [ex](examples/geo.sov.probed.json) |
| `geo.interrogation.probed` | `geo:interrogation:<key>` | one scored brand-accuracy probe result | 3b | prompt 025 | [ex](examples/geo.interrogation.probed.json) |
| `geo.selfreport.captured` | `geo:selfreport:<key>` | a human telling us an AI sent them | 4 | prompt 026 | [ex](examples/geo.selfreport.captured.json) |
| `geo.experiment.started` | `geo:experiment:<id>` | one optimization iteration opened | 5 | prompt 027 | [ex](examples/geo.experiment.started.json) |
| `geo.experiment.scored` | `geo:experiment:<id>` | one optimization iteration scored | 5 | prompt 027 | [ex](examples/geo.experiment.scored.json) |

The prompt column is the plan of record at time of writing. If a slice lands
under a different number, fix it here **and** in the `LAYERS` table in
[`geo-cli/src/report.rs`](../../../tooling/geo/geo-cli/src/report.rs) — the two
are the only places the layer map exists.

---

### `geo.referral.observed` — layer 1

A human session arriving from an AI surface.

| field | type | required | meaning |
|---|---|---|---|
| `schema_version` | integer | yes | `1` |
| `observed_at` | string (RFC 3339, UTC) | yes | when the session was observed |
| `surface` | string | yes | AI surface, free-form host-ish id (`"chatgpt.com"`). **Not** an enum — the surface map lives in [`apps/web/src/lib/geo-referrers.ts`](../../../apps/web/src/lib/geo-referrers.ts) |
| `referrer_url` | string \| null | yes | full referrer when the surface sent one |
| `landing_path` | string | yes | site-relative path landed on, query and fragment stripped |
| `session_id` | string \| null | yes | opaque analytics session id |
| `user_agent` | string \| null | yes | arriving browser's user agent |
| `converted` | boolean | yes | whether this session was later seen to convert |
| `conversion_kind` | string \| null | yes | what the conversion was (`"signup_started"`, `"api_key_minted"`); `null` while `converted` is false |

**`converted` is not in the natural key, on purpose.** A conversion is the same
arrival, later. The arrival is written the moment it happens with
`converted: false` — waiting for a conversion that may never come would lose
the arrival — and the conversion re-emits the *same* natural key, so Core
appends version 2 to the same entity. Reading the entity's latest version gives
current truth, and counting entities stays replay-safe.

**Two producers write this event**, and their idempotency keys must agree
byte-for-byte or one session would land as two entities:
[`apps/web`](../../../apps/web/src/lib/geo-referrers.ts) (TypeScript, the live
beacon) and `geo-core` (Rust). A vitest in
`apps/web/src/__tests__/geo-referrers.test.ts` asserts the TypeScript envelope
against the committed example below, which the Rust emitter generates.

[`examples/geo.referral.observed.json`](examples/geo.referral.observed.json)

---

### `geo.crawl.observed` — layer 2

One AI bot hit, or one aggregated bucket of them.

| field | type | required | meaning |
|---|---|---|---|
| `schema_version` | integer | yes | `1` |
| `observed_at` | string (RFC 3339, UTC) | yes | when the request hit the edge; for an aggregated row, the start of the bucket |
| `bot` | string | yes | normalised bot id (`"gptbot"`) — the `id` of a [`BotSpec`](../../../tooling/geo/geo-core/src/bots.rs) |
| `category` | string | yes | `"training_crawler"` \| `"search_indexer"` \| `"user_fetcher"`. Stored on the event, not re-derived at read time, so a historical count keeps the categorisation it was written with |
| `taxonomy_version` | integer | yes | which `TAXONOMY_VERSION` categorised it |
| `verified` | boolean | yes | whether the claimed identity was verified against the vendor's **published IP ranges**. `verified: false` rows must never be counted as AI crawl volume |
| `user_agent` | string | yes | raw `User-Agent` header |
| `path` | string | yes | site-relative path requested |
| `status` | integer | yes | HTTP status served |
| `source` | string | yes | which log/edge produced the line (`"vercel-log-drain"`, `"file:…"`, `"fly:…"`) |
| `aggregation` | string | yes | `"hit"` \| `"hourly"` \| `"daily"` — how much this row stands for |
| `hits` | integer | yes | raw log lines collapsed into this row; `1` at `"hit"` |
| `window_end` | string \| null | yes | end of the bucket (exclusive) when aggregated; `null` at hit level |
| `request_id` | string \| null | yes | the edge's request id when the log carried one |

**Never blend the three categories.** They answer different questions —
infrastructure readiness, citation eligibility, and live human demand — and
their volumes differ by orders of magnitude. `geo report` prints them
separately and has no all-bots total by design.

**Aggregation is declared, not implied.** `aggregation` + `hits` + `window_end`
mean a reader can never mistake one row for one hit, and record the exact
grouping key, so re-running the ingest over the same raw logs at
`--aggregate hit` recovers per-hit fidelity without disturbing the bucketed
rows (their natural key includes `aggregation` and `window_end`, so the two
never collide).

[`examples/geo.crawl.observed.json`](examples/geo.crawl.observed.json)

---

### `geo.sov.probed` — layer 3a

One scored share-of-voice probe result.

| field | type | required | meaning |
|---|---|---|---|
| `schema_version` | integer | yes | `1` |
| `observed_at` | string (RFC 3339, UTC) | yes | when the answer came back |
| `run_id` | string | yes | groups every probe in one sweep |
| `engine` | string | yes | `"chatgpt"`, `"claude"`, `"perplexity"`, `"gemini"` |
| `prompt_id` | string | yes | stable id within the probe set. The **set** is [`tooling/geo/prompts/sov.toml`](../../../tooling/geo/prompts/sov.toml) |
| `prompt_text` | string | yes | the prompt as sent, kept verbatim so a historical score stays readable after the set is edited |
| `intent` | string | yes | buyer-intent class: `"category"` \| `"problem"` \| `"comparison"` \| `"integration"`. Stored, not re-derived — SOV is reported per class and never blended, so a historical row keeps the classification it was scored under |
| `mentioned` | boolean | yes | was AllSource named at all |
| `rank` | integer \| null | yes | 1-based position among named products; `null` when absent |
| `competitors` | array of string | yes | other products named, in order of appearance |
| `cited_urls` | array of string | yes | URLs the engine cited |
| `score` | number | yes | normalised 0.0–1.0. **Reciprocal rank** (`1/rank`, `0` when absent) — see `geo-core/src/scoring.rs`. Mention rate, not this score, is the headline |

### `run_id` — `<family>-<YYYY-MM-DD>-<digest>#r<repetition>`

Two parts carry meaning, and both exist to stop a silent lie:

- **`<digest>`** is the SHA-256 (truncated) of the frozen probe-set TOML. Two
  sweeps are comparable **iff** their run ids carry the same digest; an edited
  set produces a visibly different id rather than a quietly incomparable
  number.
- **`#r<repetition>`** is the sample index within one sweep. It is in the
  `run_id` because the natural key is `run_id + engine + prompt_id` — without
  it, the N repetitions of one prompt would collapse into N *versions of one
  entity*, and a reader folding by entity (which is what makes a re-ingest
  safe) would see one sample where N were taken. The whole point of repeating
  is to keep the distribution, so the repetitions must be distinct entities.

Split on `#` to recover the sweep that groups them.

[`examples/geo.sov.probed.json`](examples/geo.sov.probed.json)

---

### `geo.interrogation.probed` — layer 3b

One scored brand-accuracy probe result: not *whether* we were named, but
whether what was said about us is true.

| field | type | required | meaning |
|---|---|---|---|
| `schema_version` | integer | yes | `1` |
| `observed_at` | string (RFC 3339, UTC) | yes | when the answer came back |
| `run_id` | string | yes | groups every probe in one sweep |
| `engine` | string | yes | engine probed |
| `prompt_id` | string | yes | stable id within the probe set |
| `prompt_text` | string | yes | the prompt as sent |
| `claim_id` | string | yes | which factual claim was under test. Claims are declared in [`tooling/geo/prompts/interrogation.toml`](../../../tooling/geo/prompts/interrogation.toml), each with its ground truth and the repository file that defines it |
| `verdict` | string | yes | one of `accurate` \| `partially_accurate` \| `inaccurate` \| `absent` \| `unscored`. `absent` (the model said nothing about the claim) is **not** wrongness, and `unscored` (the judge's reply could not be read) is excluded from every accuracy denominator |
| `reasoning` | string | yes | the judge's own argument for the verdict, stored so a human can overrule it. A verdict without its reasoning is a number nobody can audit |
| `judge_model` | string | yes | which model produced the verdict. An accuracy trend that silently spans two judge models is not a trend |
| `answer_excerpt` | string | yes | the part of the answer the verdict was drawn from, verbatim. Empty only when the verdict is `absent` |
| `cited_urls` | array of string | yes | URLs the engine cited |
| `score` | number | yes | normalised 0.0–1.0 |

[`examples/geo.interrogation.probed.json`](examples/geo.interrogation.probed.json)

---

### `geo.selfreport.captured` — layer 4

A human telling us an AI sent them — the only layer that survives a referrer
being stripped.

| field | type | required | meaning |
|---|---|---|---|
| `schema_version` | integer | yes | `1` |
| `observed_at` | string (RFC 3339, UTC) | yes | when the human told us |
| `source` | string | yes | **which signup path collected the answer** — a `capture_paths` id: `"signup-form"` (web) or `"onboard-api"` (`POST /api/v1/onboard/start`) |
| `surface` | string | yes | **what the human said sent them** — a `sources` id from [`discovery-sources.json`](discovery-sources.json) (`"chatgpt"`, `"hn-reddit"`). Lowercase kebab id, never a display label |
| `verbatim` | string \| null | yes | full free-text answer when the question allowed one. On the signup capture this is the buyer's **literal prompt** — the highest-value field in the layer, and the only first-party source of real buyer vocabulary |
| `contact_ref` | string \| null | yes | opaque reference back to the person (tenant id, hashed handle) |
| `tier` | string \| null | yes | the tenant's subscription tier at capture (`"trial"`, `"indie"`). `null` when the capturing path could not resolve one. Stored, not joined at read time — a tenant's tier moves, and "what tier did AI-sourced signups start on" is a question about the past |

`source` and `surface` are two different questions and are never blended: one
is *how we asked*, the other is *what they answered*. The API path is the one
that captures the AI-native users this programme is about, so a report that
could not separate the paths could not tell whether it is capturing anything.

**`tier` is not in the natural key.** A later tier change is the same capture,
re-stated: re-emitting appends a version to the same entity rather than minting
a second one, exactly as a layer-1 conversion does.

### The discovery-source vocabulary

[`discovery-sources.json`](discovery-sources.json) is **generated** from
[`tooling/geo/geo-core/src/discovery.rs`](../../../tooling/geo/geo-core/src/discovery.rs)
and is the authority for both `source` and `surface`. Three sides speak it and
none of them can import the others:

| side | file |
|---|---|
| web form + its route handler | [`apps/web/src/lib/geo-discovery-sources.ts`](../../../apps/web/src/lib/geo-discovery-sources.ts) |
| `/api/v1/onboard/start` validation | [`apps/control-plane/geo_selfreport.go`](../../../apps/control-plane/geo_selfreport.go) |
| `geo report` layer 4 | `geo-core` |

Each asserts against the committed JSON in its own test suite. The failure this
prevents is quiet: if one side wrote `"ChatGPT"` and another `"chatgpt"`, the
report would show two channels where there is one and the AI-sourced share —
the headline number of the layer — would be silently halved.

```bash
cd tooling/geo && cargo test -p geo-core -- --ignored regenerate_discovery_contract
```

**An `id` is never renamed.** A rename splits a historical series in two with
no way to stitch it back. Add entries; do not re-letter them. `ai: true` marks
the entries that count toward the AI-sourced share — including `other-ai`, on
purpose, because an assistant we have not named is still an assistant.

**Privacy:** `contact_ref` is never a raw email address, and `verbatim` is
user-submitted free text — see the privacy note in the layer-4 section of
`docs/runbooks/GEO_MEASUREMENT.md`. GEO telemetry is a trend timeline, not a
place to accumulate PII. Do not extend this payload with identifying fields.

[`examples/geo.selfreport.captured.json`](examples/geo.selfreport.captured.json)

---

### `geo.experiment.started` — layer 5

One optimization iteration opened. Shares an entity with its `scored` event.

| field | type | required | meaning |
|---|---|---|---|
| `schema_version` | integer | yes | `1` |
| `started_at` | string (RFC 3339, UTC) | yes | when the iteration was opened |
| `experiment_id` | string | yes | stable id, shared by every event in the lifecycle |
| `iteration` | integer | yes | iteration within the experiment, from 1 |
| `hypothesis` | string | yes | what we think the change will do, in one sentence |
| `target_layer` | string | yes | which layer the hypothesis aims at (`"referral"`, `"crawl"`, `"sov"`, `"interrogation"`, `"selfreport"`) |
| `changes` | array of string | yes | what was actually changed — paths, surfaces, files |
| `baseline_score` | number \| null | yes | target metric before the change, when known |

[`examples/geo.experiment.started.json`](examples/geo.experiment.started.json)

---

### `geo.experiment.scored` — layer 5

One optimization iteration scored. This is the single metric the optimization
loop reads back out of Core.

| field | type | required | meaning |
|---|---|---|---|
| `schema_version` | integer | yes | `1` |
| `scored_at` | string (RFC 3339, UTC) | yes | when the score was computed |
| `experiment_id` | string | yes | the experiment this scores |
| `iteration` | integer | yes | iteration scored |
| `metric` | string | yes | which metric was scored (`"sov.mention_rate"`) |
| `score` | number | yes | the score |
| `window_start` | string (RFC 3339, UTC) | yes | start of the measurement window, inclusive |
| `window_end` | string (RFC 3339, UTC) | yes | end of the measurement window, exclusive |
| `verdict` | string | yes | what the loop decided. Vocabulary belongs to prompt 027 |
| `notes` | string \| null | yes | anything a human should read before trusting the verdict |

[`examples/geo.experiment.scored.json`](examples/geo.experiment.scored.json)

## Reading the family back

```
GET /api/v1/events/query?event_type_prefix=geo.&since=<rfc3339>&until=<rfc3339>
```

`geo report` does exactly this and tallies per layer — see
[`docs/runbooks/GEO_MEASUREMENT.md`](../../runbooks/GEO_MEASUREMENT.md).

## Adding an event

1. Add the variant to `GeoEventType` and a payload struct in `geo-core`.
2. Add its natural key to `GeoEvent::idempotency_key`.
3. Add a canonical example to `geo-core/src/samples.rs`.
4. Put it in a layer in `geo-cli/src/report.rs`.
5. Regenerate `examples/` (command at the top of this file).
6. Document it here.

Steps 3 and 5 are load-bearing: `the_examples_directory_holds_exactly_the_contract`
fails if `examples/` and `GeoEventType::ALL` disagree, so an event cannot be
added on one side only.
