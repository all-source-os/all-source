# voice-demo

Proves the prime-backed **voice file** works against the real `allsource-prime`
binary, using only existing `prime_*` MCP tools (no server changes).

The viral pattern (Ruben Hassid): run yourself through a ~100-question interview,
hand-compress the transcript into a ~4k-token markdown "voice file," paste it into
any AI tool so output sounds like you. The weakness we exploit: that markdown is a
dead blob — it goes stale, it's one undifferentiated lump, it can't be queried by
relevance, and it has no history.

This harness records ~12 voice facets as embedded `voice` nodes, then shows:

1. **Facets recorded** — `prime_stats` proves the 12 facets are durably stored
   (WAL + Parquet), not a transcript in a file.
2. **Compressed voice file** — `prime_index` is meant to be the auto-generated,
   always-current equivalent of the hand-compressed 4k-token markdown.
3. **Relevant slice per task** — `prime_recall` / `prime_context` pull only the
   facets relevant to a writing prompt, instead of pasting the whole file.
4. **History** — `prime_history` shows the voice has provenance and can
   time-travel. A static blob can't.

It then produces a **voice-ON vs voice-OFF** comparison for the same prompt:
the recalled facets are injected into the ON arm; the OFF arm gets nothing.

## Run

```bash
cd tooling/voice-demo
python3 voice_demo.py
# or point at a specific binary / reuse the bench fastembed cache:
FASTEMBED_CACHE_PATH=../mammoth-bench/.fastembed_cache \
  PRIME_BIN=~/.cargo/bin/allsource-prime python3 voice_demo.py
```

Requires `allsource-prime >= 0.21.3` (text-on-embed). Drives the binary over
stdio JSON-RPC against a throwaway temp `--data-dir`, exactly like
`tooling/mammoth-bench/bench.py`. Never touches your real `.prime/` store.

Output is written to `RESULTS.md` (committed — it's the proof artifact).

## Honest finding

`prime_recall` and `prime_stats` return the correct voice slice and counts.
Fixed in `allsource-prime` 0.21.6 (commit `5083017`): `prime_index` returns the
populated compressed voice file (12 nodes, 5 domains, 77 tokens), and
`prime_context` L2 returns hybrid recall. `/voice export` (the `prime_index`
markdown bridge) is documented in `plugin/mammoth/commands/voice.md`. See
`docs/proposals/PRIME_VOICE_FILE.md` § "4. Convention vs. server (honest separation)"
and `RESULTS.md`.
