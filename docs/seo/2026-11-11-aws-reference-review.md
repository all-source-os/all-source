# Event sourcing reference correction — 2026-11-11

## Finding and proposed change

**Observed:** At 10:09 UTC, the web fetch of the AWS reference currently specified in `origin/main:apps/web/src/lib/event-sourcing-patterns.ts` redirected to the generic AWS Prescriptive Guidance index. The [specific event sourcing pattern](https://docs.aws.amazon.com/prescriptive-guidance/latest/cloud-design-patterns/event-sourcing-pattern.html) returned the intended article. The shared reference is used by six pattern records in the local `origin/main` snapshot. Production rendering could not be checked.

**Prepared:** Changed the local shared `AWS_EVENT_SOURCING.href` from `https://docs.aws.amazon.com/prescriptive-guidance/latest/cloud-design-patterns/event-sourcing.html` to `https://docs.aws.amazon.com/prescriptive-guidance/latest/cloud-design-patterns/event-sourcing-pattern.html`. The label is unchanged. The GitHub default-branch source was checked again at 13:30 UTC on November 11. [Preview](2026-11-11-aws-reference-preview.html) shows an excerpt of the event replay guide with the proposed link; download and open it locally.

**Observed search intent:** On November 11, results for `event sourcing replay side effects idempotency projections` included [AWS's pattern](https://docs.aws.amazon.com/prescriptive-guidance/latest/cloud-design-patterns/event-sourcing-pattern.html) and [Microsoft's event sourcing pattern](https://learn.microsoft.com/en-us/azure/architecture/patterns/event-sourcing). Both address replay and duplicate or external effects. Search results do not establish demand volume or rankings.

## Verification and status

The replacement URL returned the named AWS article through the web fetch tool. The original URL redirected to the general index. A Node check matched the preview heading, opening sentence and destination to source; `git diff --check` passed for the source edit. Browser rendering and link navigation were not checked. Deployment and publication remain unverified.

Codex SEO distinguishes observed fetches from inferred impact; no indexation or traffic outcome is claimed. The production site and sitemap returned web-tool internal errors during this assessment, so their live contents and canonical status remain unknown. Search Console data was unavailable.

Treg `catalog_get` on November 11 listed the routed Google Trends endpoint at $0.0012–$0.015 per hit with possible additional waterfall charges. No paid call was made and no trend claim is made.

## Blocker

The correction, preview and indexed report are prepared on an isolated non-trunk branch. Production deployment and search indexing remain unverified. Recheck the live page after release.
