# Beads

One bead per story, under the approved epic, wired as a dependency graph. Tracker: Chronis
(`cn`). beads (`bd`) and beads-rust (`br`) are deprecated: never create or update work with them.

## Before cutting

1. `cn` is on `PATH`; if not, stop and tell the user (`cargo install chronis`).
2. A repo with `.beads/` and no `.chronis/` still runs the old tracker: suggest
   `cn migrate-beads --toon` and wait for the user.
3. `cos ritual check <epic>` exits 0 (see `plan.md`).
4. Audit the open and in-progress beads already in the repo for actionability
   ([actionability.md](actionability.md)) before adding more.
5. Read [graph.md](graph.md): the beads are a graph, not a list.

## Detect the gates' commands

Prefer a `Taskfile.yml` or `Makefile` target (`task ci`, `make ci`). Otherwise the runtime's own
runner: `bun` (read the version from `packageManager`, see `repo-notes.md`), `cargo`, `mix`, `go`.
Use the package manager the lockfile names.

## Create

Always single-quote the heredoc delimiter so backticks and `$` stay literal. Pass `--toon` on
every `cn` call.

```bash
# The epic exists since Think. Put the epic-level gates on it:
cn task edit <epic> --toon --append-description "$(cat <<'EOF'

## Epic quality gates (run once, when every bead is done)
- [ ] the repo's CI entrypoint passes
- [ ] testing trophy complete: every layer in the plan file covered
EOF
)"

# One bead per story, story-level criteria only:
cn task create "US-001: <title>" --toon --parent <epic> --priority p1 \
  --description "$(cat <<'EOF'
As a <user>, I want <capability> so that <benefit>.

## Acceptance criteria
- [ ] Test: <behaviour> (file: <path>), written first and failing
- [ ] <verifiable criterion>
- [ ] All tests green: <command>

Mark each item [x] as you complete it. Only close when all are checked.

Work organization: part of epic <epic>, branch <type>/<slug>. Commit on that one branch.
No PR for this bead alone. Do not squash, amend or force-push the branch; the orchestrator
collapses it once at the end.
EOF
)"

# Wire the graph: the first argument is blocked by the second.
cn dep add <US-002 id> <US-001 id> --toon
```

`--blocked-by <id>` on `cn task create` sets the edge at creation instead.

## One feature, one branch, one PR

Every bead of an epic lands on the same short-lived branch and merges as one PR when the feature
works end to end. Size is never a reason to split a feature into several PRs. Separate PRs only
for features that share no end-to-end path. Parallel agents converge on the same branch, each in
its own worktree with a detached HEAD, pushing with `git push origin HEAD:<branch>`.

The branch is the one Spec cut and named on the epic (`ritual-branch:`); stamp it into every bead.
A bug flow that skipped Spec cuts and names it here, using the repo's own branch and PR title rule
(see the org's notes, `repo-notes.md`, for ticket keys).

## After cutting

Re-audit the graph: every `blocked_by` resolves to a bead id, every ready bead has a first action
and a finish condition, and no criterion waits on a person. Then show the user the ready set
(`cn ready --toon`) and the graph, and move to Build ([build.md](build.md)).
