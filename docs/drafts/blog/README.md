# Blog drafts (unpublished)

These `.mdx` posts are **not** in the content directory of the web app
(`apps/web/content/`), which the Fly app `allsource-web` builds, so they do **not**
appear on www.all-source.xyz. They are kept here for review.

To publish one, move it back:

```bash
git mv docs/drafts/blog/<slug>.mdx apps/web/content/<slug>.mdx
git commit -m "content(blog): publish <slug>"
git push origin main
fly deploy . --config apps/web/fly.toml --remote-only --ha=false
```

## Current drafts — Prime Hound launch set

- `graphify-vs-allsource-prime.mdx` — engineering; Graphify vs Prime (truthful-today, publish-ready).
- `prime-hound-living-knowledge-graph.mdx` — product; Prime Hound vision (carries an explicit "roadmap, not GA" banner).
- `one-graph-for-code-and-agent-memory.mdx` — use-cases; code-graph + agent-memory in one store. Cross-links the prime-hound post, so publish the two together (or neither) to avoid a dead link.
