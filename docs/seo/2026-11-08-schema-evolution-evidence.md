# Schema evolution example: evidence and status

Checked 2026-11-08. Public search for event-sourcing schema evolution and upcasting returned [Microsoft's event-sourcing guidance](https://learn.microsoft.com/en-us/azure/architecture/patterns/event-sourcing), which discusses event versioning, upcasters and replay tests. This supports an informational implementation intent; it does not establish search volume or ranking. The existing AllSource schema-evolution guide already covers this topic, so improving its example is preferable to creating a duplicate page.

The local example described deriving a currency from a recorded contract version but did not include that version in its v1 event. The revised example includes an illustrative contract reference and explicitly leaves currency unknown if retained contract evidence is unavailable. This is illustrative data, not a claim that a specific AllSource contract field or lookup API exists. Confirm the rendered production guide and its semantics with the product owner before release.

treg `catalog_search` and `catalog_get` for Google keyword trends returned a routed endpoint priced $0.0012–$0.015 per hit, with possible charges for attempted providers. No no-cost route was verified and no trend call was made. No trend metric is claimed.

The 2026-11-07 [agent-memory guide](https://www.all-source.xyz/blog/agent-memory-after-compaction) returned a 404 in the browser on 2026-11-08. Its source exists on the fetched main branch; this does not establish the cause of the production response. Runbooks disagree about Fly and Vercel as the web deployment path, so the active production owner must establish the deployed artifact and release path before a deployment decision. Paid gates and unknown revenue are unchanged.

No AllSource PR was open when checked on 2026-11-08. Publication of the correction remains unverified.
