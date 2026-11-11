# Build

Work the ready beads on the one branch until the feature works end to end.

## Setup

1. Work in the flow's worktree, on the branch Spec cut (`ritual-branch:` on the epic). A bug flow
   that skipped Spec cuts it now (`git worktree add -b <branch> <path> origin/<trunk>`) and names
   it on the epic. Seed the worktree with the repo's dependencies and a cloned build directory, so
   a cold build does not time out a gate.
2. Confirm the branch is live before each session's first commit:
   `gh pr list --head <branch> --state all --json number,state`. `MERGED` means dead: branch fresh
   from the trunk.

## The loop

```bash
cn ready --toon                      # open and unblocked
cn claim <id> --toon                 # one bead at a time per agent
# implement: the test first, failing; then the code; then the test green
cn done <id> --toon --reason "<what landed, the commit, the test command and its result>"
```

For each bead:

- Work through its criteria in order, verify each (run the command, read the output), and tick it
  in the description: read `cn show <id> --toon`, change only that checkbox, and write the full
  text back with `cn task edit <id> --description - --toon`.
- Close it only when every box is ticked. "Done" for a bead is committed and pushed to the
  feature branch, not a PR of its own.
- A finding outside the bead that the feature needs is a new bead under the same epic, wired into
  the graph. Never park it.
- When a bead turns out to wait on a person, move the external action to docs and close the bead
  with that reason (see `actionability.md`).

## When every bead is done

Run the epic-level gates from the epic's description. A compile, a lint or a `--no-run` build is
not a test run, and a test filter that matched nothing proves nothing: read the test count. If a
gate fails, cut a fix bead and loop.

Then Review ([review.md](review.md)).

## In cos

`cos` can hire `builder` per bead: it spawns the agent in its own worktree, watches it, and
judges it from the diff, the checks and the scope, never from the agent's summary.
