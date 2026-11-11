# Ritual-full flow adoption

## What changed

- Added the `ritual`, `ritual-full` and `semantic-isa` skills, byte-for-byte from decebal-claude-skills PR #97 head `8af7330`, to `.claude/skills/` and `.agents/skills/`. Each copy matches its source under `diff -r`.
- Added a `## Planning` section to `CLAUDE.md` and to `AGENTS.md`. It names the ritual-full steps, the design-approval rule, the `cos ritual approve` / `cn approve` commands, the `systematic-debugging` route for a reproduced bug, and the vendoring rule.
- Removed the AllStack enrollment: nine `allstack-*` skills from `.claude/skills/` and nine from `.agents/skills/`, the `workflows/` directory (10 files), and `docs/reference/allstack.md`, `allstack-skills.md`, `allstack-catalog.md` with their index lines in `docs/README.md`.
- Repointed two live docs off the removed gates: `docs/runbooks/HOSTED_PRIME_SYNC.md` now names the ritual-full steps (Plan, Review, Build, Ship) instead of the `allstack-*` skills, and `docs/reference/prime-typed-provenance-request.md` points its acceptance at `.github/workflows/ci.yml`.
- Added the Ship/PR rule to the Planning section of `CLAUDE.md` and `AGENTS.md`: a PR only when the user asks for one, else main.

## Kept on purpose

- `docs/plans/allstack-*.md` and the dated plan records. They are history, and their index lines stay.
- `docs/reports/` files that mention AllStack or link to `workflows/*.toml`. They are dated evidence; they are not rewritten.
- The `/.allstack/` line in `.gitignore`. Private proof state may still exist on disk under that path.

## Repo facts

- Default branch `main`. No open or merged PR on `chore/ritual-full` before this change.
- `cn ready --toon` run from the main checkout through `genv -C`: 4 ready rows, one epic (`t-0489`), three non-epic tasks. No ready task is a ritual-full epic yet.
- `git grep -n -E "allstack-(shape|plan|review|qa|ship|recall|handoff|catalog|delivery)"` outside `docs/plans` and `.chronis`: 4 lines in 3 files, all in dated `docs/reports/` records, this report's line 7 included. `docs/runbooks/HOSTED_PRIME_SYNC.md` has 0.
- `git grep` for links to removed `workflows/*.toml` or `reference/allstack*.md` paths: 12 files still link to them, counting this report, all dated plans or reports (`docs/plans/`, `docs/reports/`).

## Checks run (text only, no compile)

- `diff -r` of the three skills against decebal-claude-skills PR #97 head `8af7330`, in both skill directories: 6 of 6 identical, no output.
- `git grep` for `allstack` outside the kept set: matches listed above.
- `cn ready --toon` via `genv -C` on the main checkout: read only.
- `git diff --cached --stat` before commit: checked against the explicit path list.
- `render-agent-docs --check`: not run. The worktree has no `agent-docs.toml`, so there is nothing to render.

## What I have NOT checked

- The `../../docs/ritual-zero-to-hero.md` link in the vendored `ritual` and `ritual-full` skills resolves only in decebal-claude-skills. Kept byte-identical upstream.
- Whether `cos ritual approve` and `--yes` work in this repo. They depend on chief-of-staff-v2#23.
- Whether a full ritual-full flow has run on a real feature here.
- The vendored `ritual-full` preflight tells the agent to run `cargo install --path gates/rust/claude-guard`. That path does not exist in this repo. The skill is vendored and was not edited here; the fix belongs upstream.
- The runbook's `allstack check` command and `ALLSTACK_*` variable names are not verified against current tooling. They are CLI and env names, not skills, and were left as written.
- No code gate was run. This is a text-only change.
