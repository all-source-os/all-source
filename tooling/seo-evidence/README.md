# seo-evidence

Collects SEO and AI-search measurements from the sources a static HTML audit
cannot reach, and records what each one answered — including the fact that it
answered nothing.

Setup for every source, and the exact condition that unblocks each:
[`docs/runbooks/SEO_EVIDENCE_SETUP.md`](../../docs/runbooks/SEO_EVIDENCE_SETUP.md).
The 2026-10-20 baseline reading:
[`docs/reports/2026-10-20-seo-evidence-baseline.md`](../../docs/reports/2026-10-20-seo-evidence-baseline.md).

## Why it exists

A static audit measures the page, not its reception. This site scores 100/100 on
static HTML while holding Domain Authority 2 and taking two organic clicks a
month. Both facts are true and only one of them is actionable, so the tool that
reports the first must not be the only tool in the loop.

## The state tag

Every entry carries one of four states, and the state travels with the value
instead of being asserted in prose beside it:

| State | Means | Response |
|---|---|---|
| `observed` | measured, value present | read it |
| `observed_empty` | source answered, authoritatively nothing | a finding |
| `setup_required` | credential or binary absent, no value by construction | a chore, with `unblock` attached |
| `failed` | configured and refused, provider's message kept | a broken pipeline |

`observed_empty` and `setup_required` are kept apart because they demand opposite
responses, and because an absence never carries its own explanation: zero rows
from an authenticated Search Console property means "this site has no
impressions", while zero rows from an unauthenticated one means nothing at all.

A `setup_required` entry has no `value` field at all, so a consumer cannot quote
a number out of an entry that has none. There is a test for that.

## Commands

```bash
# Which sources are configured. Calls nothing.
cargo run --manifest-path tooling/seo-evidence/Cargo.toml -- doctor

# Exit 2 if any source is still awaiting setup. For CI, once all are expected live.
cargo run --manifest-path tooling/seo-evidence/Cargo.toml -- doctor --strict

# Query every configured source and write the ledger. Exits 2 only on `failed`.
cargo run --manifest-path tooling/seo-evidence/Cargo.toml -- \
  collect --window-days 28 --out ledger.json

# One source at a time.
cargo run --manifest-path tooling/seo-evidence/Cargo.toml -- collect --source render

# Exit 2 if a source stopped answering.
cargo run --manifest-path tooling/seo-evidence/Cargo.toml -- \
  drift --before docs/reports/baselines/seo-evidence-2026-10-20.json --after ledger.json
```

## Sources

| `--source` | Needs | Answers |
|---|---|---|
| `render` | local Chrome | rendered DOM vs static: schema, title, canonical, robots |
| `gsc` | `GSC_SERVICE_ACCOUNT_JSON` | queries and pages this property is served for |
| `ga4` | `GA4_SERVICE_ACCOUNT_JSON`, `GA4_PROPERTY_ID` | what organic sessions do after landing |
| `psi` | `PAGESPEED_API_KEY` | lab Lighthouse scores, per-URL field CWV |
| `crux` | `CRUX_API_KEY` | origin-level field CWV |
| `bing` | `BING_WEBMASTER_API_KEY` | Bing impressions, clicks, position |

Ranked keywords, backlinks and Domain Authority are **not** here. They come from
`treg`, which holds the provider credentials, needs no account, and is metered
per call — so those are run on demand rather than on a schedule. The runbook has
the four commands and what the last run cost.

## Two things that will bite

**Chrome's exit code lies.** It frequently exits non-zero after a successful
`--dump-dom`. The collector judges a render by the bytes on stdout; a check
reading the exit status will report failure on a perfectly good 119 KB DOM.

**An absent CI secret is an empty string, not an unset variable.** Every
credential is read through `config::env_opt`, which treats empty as absent.
Replacing it with `env::var(...).is_ok()` makes a missing secret look configured
and turns every downstream call into an unexplained 401.

## Not in scope

No recursive crawl, no SERP scraping, no ranking claim from this tool's own data.
Google Business Profile is deliberately absent: a listing needs a physical
location or service area, and a software product has neither.
