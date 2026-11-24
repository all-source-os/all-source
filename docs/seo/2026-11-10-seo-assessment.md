# AllSource SEO assessment — 2026-11-10

## Search and proposed improvement

Observed at 10:07 UTC: search results for `event sourcing schema evolution upcasting immutable events` included [Microsoft's event sourcing pattern](https://learn.microsoft.com/en-us/azure/architecture/patterns/event-sourcing), which describes upcasting older event schemas during deserialization. Results for `agent memory compaction persistent context` included [OpenAI's memory and compaction example](https://developers.openai.com/cookbook/examples/agents_sdk/building_reliable_agents_memory_compaction). These results describe intent, not search volume or ranking performance.

Draft recommendation: in the schema-evolution guide, explain that an upcaster transforms the representation read by an application and that missing historical facts cannot be safely inferred merely from a newer schema. Link this explanation to the existing historical-contract example. Check the deployed page and existing surrounding text before editing to avoid duplication. The missing-fact caution is an inference from the distinction between historical evidence and schema transformation, not a claim from Microsoft.

## Status and evidence limits

The proposed text is a draft; no product content was changed or published in this assessment. The prior correction and its [HTML preview](https://github.com/all-source-os/all-source/blob/main/docs/archive/2026-11-23/seo/2026-11-09-schema-preview.html) and [report](https://github.com/all-source-os/all-source/blob/main/docs/seo/2026-11-09-schema-review.md) landed in [PR #402](https://github.com/all-source-os/all-source/pull/402), merged November 9. Its preview matched the proposed example in the prior check; visual rendering and link navigation were not verified.

At 10:07 UTC, the web fetch tool returned internal errors for the production article, schema guide, and sitemap. Therefore current live content, canonical, sitemap inclusion and indexing are unverified. The [agent-memory article](https://www.all-source.xyz/blog/agent-memory-after-compaction) returned 404 on November 8; that observation does not establish its current status. Search Console data was unavailable.

The `codex-seo` workflow distinguishes observed evidence from inference and does not treat search results or static checks as proof of indexation. Treg `catalog_get` on November 10 listed `treg.google.keywords.trends` at $0.0012–$0.015 per hit, with possible additional waterfall charges. No trend endpoint was called; no trend or volume is asserted.

## Release blocker

The product checkout is on `main`, contains other owners' uncommitted changes, and is behind its local `origin/main`. PR #402 is merged, so its branch is no longer eligible for new commits. No fresh branch was authorized in this heartbeat. GitHub CLI could not connect; the connected GitHub search found no open schema-evolution PR, which does not establish whether another owner would authorize sharing any other branch. Do not overwrite existing work or push to trunk. A new branch requires explicit permission in the current message. This report is uncommitted and not yet linked from the docs index because that index contains another owner's uncommitted changes.
