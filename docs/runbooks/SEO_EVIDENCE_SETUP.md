# Runbook — SEO and AI-search evidence sources

Every measurement source behind `tooling/seo-evidence`, what it answers, and the
exact condition that unblocks it. Two of the eight need nothing and already run.

`seo-evidence doctor` is the authority on current state — this file explains how
to change it. Never quote a source as unavailable from this document; run the
command.

```bash
cargo run --manifest-path tooling/seo-evidence/Cargo.toml -- doctor
```

## State as of 2026-10-20

| Source | Answers | State | Blocked on |
|---|---|---|---|
| Headless Chrome render | is the content in the DOM, or behind JavaScript | **running** | nothing — uses local Chrome |
| `treg` SERP + backlinks | ranked keywords, positions, referring domains, Domain Authority | **running** | nothing — treg injects the provider key |
| Search Console | queries and pages *this property* is served for | setup required | service-account key + property access |
| GA4 Data API | what organic sessions do after they land | setup required | service-account key + property Viewer |
| PageSpeed Insights | lab Lighthouse scores, per-URL field CWV | setup required | a Google Cloud API key |
| Chrome UX Report | origin-level field CWV | setup required | a Google Cloud API key |
| Bing Webmaster | Bing impressions, clicks, position | setup required | site verification + API key |
| Google Business Profile | map pack, reviews, local pack | **not applicable** | AllSource has no physical premises or service area |

Google Business Profile is listed so nobody re-opens it as a gap. A GBP listing
requires a business location or service area that a customer can visit or be
served at; a software product has neither, and creating one would be a policy
violation rather than a measurement win.

## Why `treg` instead of DataForSEO and Moz accounts

Both were on the original blocked list. Neither needs an account: `treg` holds
the provider credentials and injects them server-side, metered per call from a
prepaid balance. The 2026-10-20 baseline cost **$0.083** in total and the Domain
Authority figure in it was served by Moz.

```bash
# ranked keywords, positions, volumes — $0.001 for the overview, $0.06 for 50 rows
treg call spyfu.google.domain.overview --query domain=all-source.xyz
treg call spyfu.google.domain.ranked_keywords --query query=all-source.xyz --query pageSize=50

# Domain Authority, referring domains, spam score — $0.013, served by Moz
treg call treg.web.backlinks.summary --data '{"target":"all-source.xyz"}'

# every backlink with anchor and first-seen date — $0.009
treg call seranking.web.backlinks.list --query target=all-source.xyz --query limit=50
```

Check `treg balance` first. A run of the four calls above costs under ten cents;
`spyfu.google.domain.ranked_keywords` is priced per row, so pass `pageSize`
explicitly — its default is 5 and an unbounded pull is the only way to spend
real money here.

## Search Console — `GSC_SERVICE_ACCOUNT_JSON`

The property is already verified: the google-site-verification token is in
`apps/web/src/app/layout.tsx` and `robots.txt` carries the `Sitemap:` line. What
is missing is *API* access, which is a separate grant.

1. In Google Cloud console, pick or create a project and enable the **Google
   Search Console API**.
2. Create a service account. No roles are needed — this one is used as an
   identity, not for Cloud permissions.
3. Create a JSON key for it and download the file.
4. **Add the service account's own email as a user on the Search Console
   property.** Search Console → Settings → Users and permissions → Add user →
   paste `<name>@<project>.iam.gserviceaccount.com`, permission **Full** (the
   `searchAnalytics` method is refused at Restricted).
5. Put the whole JSON file in the repo secret `GSC_SERVICE_ACCOUNT_JSON`.

Step 4 is the one that is always skipped, and skipping it produces a 403 whose
message names the property rather than the missing grant.

**Unblocked when:** `seo-evidence collect --source gsc` reports `observed` or
`observed_empty`. `observed_empty` is a legitimate end state for a property with
no impressions yet — it means the pipeline works and the site has no traffic,
which are different problems.

## GA4 — `GA4_SERVICE_ACCOUNT_JSON`, `GA4_PROPERTY_ID`

The site tag already ships: `NEXT_PUBLIC_GA_MEASUREMENT_ID=G-3347JLG51K` is
wired through `components/google-analytics.tsx`.

1. Enable the **Google Analytics Data API** on the same Cloud project.
2. Reuse the Search Console service account, or make a second one.
3. GA4 → Admin → Property access management → add the service-account email
   with the **Viewer** role.
4. `GA4_PROPERTY_ID` is the **numeric** property id from GA4 → Admin → Property
   details. It is not the `G-` measurement id; passing the `G-` id returns a 400
   that reads like a malformed request.

**Unblocked when:** `seo-evidence collect --source ga4` reports `observed`.

## PageSpeed Insights — `PAGESPEED_API_KEY`

The anonymous quota is **zero**. A keyless call answers:

```
HTTP 429  "quota_limit_value": "0"
```

So this cannot be worked around by auditing fewer URLs.

1. Same Cloud project → enable the **PageSpeed Insights API**.
2. APIs & Services → Credentials → Create credentials → API key.
3. Restrict the key to the PageSpeed Insights API.
4. Store as `PAGESPEED_API_KEY`.

**Unblocked when:** `seo-evidence collect --source psi` reports `observed` with a
non-null `lab_scores.performance`.

## Chrome UX Report — `CRUX_API_KEY`

Enable the **Chrome UX Report API** on the same project. The same API key works
if you do not restrict it to one API; otherwise create a second key.

CrUX answers **404 for an origin below its public sampling threshold**, and the
tool records that as `observed_empty`, not as a failure — "too little traffic to
be assessed" is a measurement, and for this site it is the likely answer.

**Unblocked when:** `seo-evidence collect --source crux` reports `observed` or
`observed_empty`.

## Bing Webmaster Tools — `BING_WEBMASTER_API_KEY`

1. Sign in at <https://www.bing.com/webmasters> and add `https://www.all-source.xyz`.
   Importing from Search Console is the fastest verification path and reuses the
   grant above.
2. If verifying by meta tag instead, set the build-time env var
   `NEXT_PUBLIC_BING_SITE_VERIFICATION` on the `allsource-web` Fly app and
   redeploy. The root layout emits `msvalidate.01` only when it is set.
3. Settings → API access → API key → generate.
4. Store as `BING_WEBMASTER_API_KEY`.

**Unblocked when:** `seo-evidence collect --source bing` reports `observed` or
`observed_empty`.

## Rendered DOM — already running

Uses whatever Chrome is on the machine; `CHROME_PATH` overrides the search.

Chrome frequently exits **non-zero after a successful `--dump-dom`**, so the
collector judges a run by the bytes on stdout and not by the exit status. A run
that reports failure with a plausible DOM size is a bug in the check, not in
Chrome.

**What it is for:** comparing rendered schema and title against the static
response. On 2026-10-20 the two were identical on `/docs`, which is what
licenses treating a static-HTML audit as a proxy for the real page on this site.
Re-check it after any change to client-side rendering, because that assumption
is what the cheap audits rest on.

## Adding the secrets

```bash
env -u GITHUB_TOKEN gh secret set GSC_SERVICE_ACCOUNT_JSON < service-account.json
env -u GITHUB_TOKEN gh secret set GA4_SERVICE_ACCOUNT_JSON < service-account.json
env -u GITHUB_TOKEN gh secret set GA4_PROPERTY_ID
env -u GITHUB_TOKEN gh secret set PAGESPEED_API_KEY
env -u GITHUB_TOKEN gh secret set CRUX_API_KEY
env -u GITHUB_TOKEN gh secret set BING_WEBMASTER_API_KEY
```

An **absent** repo secret interpolates as an **empty string**, not as an unset
variable, so a workflow cannot tell "secret missing" from "secret empty" by
testing for existence. The collector reads every credential through a helper
that treats empty as absent for exactly this reason — do not replace it with
`env::var(...).is_ok()`.

## Turning the workflow strict

`.github/workflows/seo-evidence.yml` runs weekly and uploads the ledger. Once a
source is expected to be configured, add it to the strict list so a revoked
credential fails the run instead of quietly shrinking the report:

```yaml
- run: cargo run --manifest-path tooling/seo-evidence/Cargo.toml -- doctor --strict
```

`doctor --strict` exits 2 if any source is still awaiting setup, so turn it on
only when all of them are meant to be live.

## Drift

The workflow compares each run against the committed baseline and fails on a
**regression**, defined as a source that moved down the ranking
`observed > observed_empty > failed > setup_required`, or disappeared.

```bash
cargo run --manifest-path tooling/seo-evidence/Cargo.toml -- \
  drift --before docs/reports/baselines/seo-evidence-2026-10-20.json --after ledger.json
```

Losing an observation is the failure this catches. A run whose credential was
revoked produces a ledger full of `setup_required` and no error at all, which
reads as a clean report until something compares it with the run before it.
