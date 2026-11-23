# #7183 evidence and reproducer

Read FINDINGS.md first. provider-analysis.json indexes every captured request and native step. evidence/ retains exact provider body files, native workflow/compaction messages, hooks, readiness barriers and source checks. The original provider body hashes remain valid; CHECKSUMS.json covers every included file.

## Verify retained evidence offline

Run `bun verify.ts` in this extracted directory. This requires no API key and makes no network requests. It verifies provider-body hashes/statuses, native checkpoint step counts and fixture workflow scores. It does not rerun the model.

## Run new isolated trials

Use a disposable /tmp/allsource-omo-compaction root. Keep upstream source at source/, this runner/observer set at the root, fixtures at provider/, and an OpenCode 1.18.22 binary at runtime/opencode. No native binary or compiled plugin bundle is redistributed in this kit.

1. Clone the upstream repository, fetch PR #7183 and use detached commit 93d44a0760448f7f07d2795c6239d51db5ab6576. Install locked source dependencies with lifecycle scripts disabled. The retained bundle used Bun 1.4.3; record a new bundle digest if rebuilding differs.
2. Build/stage from source/:

```text
bun build packages/omo-opencode/src/index.ts --outdir dist --target bun --format esm --external zod
cp -R packages/shared-skills/skills dist/skills
```

3. Prepare a disposable native SDK seed, for example `npm install --ignore-scripts --prefix /tmp/allsource-omo-compaction/dependency-seed @opencode-ai/plugin@1.18.22`. It must contain package.json, package-lock.json and node_modules. Set QA_DEPENDENCY_SEED to that absolute directory. No prior evaluator profile or session database is needed.
4. Set OPENROUTER_API_KEY securely in the environment. No local-profile credential fallback is included. Model requests use that account; offline verification does not.
5. Put c1/c2 fixture JSON under provider/ and run serially, each under a separate outer deadline no greater than 285 seconds:

```text
QA_PAIR=c1 QA_SLOT=c1a QA_SKILL_TEXT=on bun provider-run.ts
QA_PAIR=c1 QA_SLOT=c1b QA_SKILL_TEXT=off bun provider-run.ts
QA_PAIR=c2 QA_SLOT=c2a QA_SKILL_TEXT=off bun provider-run.ts
QA_PAIR=c2 QA_SLOT=c2b QA_SKILL_TEXT=on bun provider-run.ts
```

Use fresh slot directories. On macOS wrap each command with gtimeout -k 5 285; on Linux use timeout -k 5 285. No builds or trials should run concurrently. Readiness has a 20,000 ms cap. Second compaction confirms native cooldown suppression; third follows a real 62,000 ms wait. Reload remains blocked after initial native loading. Parent owns/reaps each server.

Runner portability changes are limited to environment-only API key loading and a caller-provided native dependency seed. The readiness function and core dispatch/reload observers are unchanged; PORTABILITY.json records hashes and checks. Syntax/static checks and retained-evidence verification pass. These portability edits have not been rerun against a live model. Request and phase capture use the existing TypeScript/Bun plugin API directly; rewriting those callbacks in Rust would test another implementation.

New evidence appears in provider/evidence/. Run the included verifier from that provider/ directory, with QA_SOURCE_DIR and QA_EVIDENCE_DIR if needed. It recomputes scores and body coverage without relying on copied aggregate success claims.
