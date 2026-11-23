@AceRothstein71 I tested `93d44a0` through the full OMO plugin and captured 73 accepted OpenRouter requests.

With skill reload blocked after the initial model-selected load, two matched pairs ran on → off and off → on:

| Skill reinjection | Compaction 1 | Compaction 2, inside cooldown | Compaction 3, beyond cooldown |
| --- | --- | --- | --- |
| On, both runs | Pass | Fail | Pass |
| Off, both runs | Fail | Fail | Fail |

All baselines passed. Skill-off removed only the dispatched skill block; agent/model/tools recovery and summary injection stayed intact. Readiness barriers completed before all 16 tasks.

Full instructions and the current query appeared in all four fresh skill-on first requests. Both cooldown first requests and all six skill-off first requests lacked them. Summaries retained skill names and used queries, dropping unused map entries. This adds provider-input traces to the existing cooldown review.

These are manual compactions of a synthetic skill with source-scoped tools and one `minimax/minimax-m3.1-flash:free` route on OpenCode 1.18.22. Provider-internal handling remains unobserved.

I build AllSource and previously commented on #7764. A sanitised reproducer, provider bodies and hashes are ready to share. Would they help your regression tests?
