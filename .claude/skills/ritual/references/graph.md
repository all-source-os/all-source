# Graphs

Harness engineering is designing the steps, the gates and the evidence an agent works inside.
The ritual uses two graphs to do it.

## The work is a graph

A flow's beads and their `blocked_by` edges form a dependency graph. Plan it as one:

- **Find the roots first.** What can start with nothing done? Usually schema, contracts, test
  fixtures.
- **Each edge is a real prerequisite.** "B uses A's output" is an edge. "B comes after A in the
  document" is not.
- **Keep the ready set wide.** Independent beads with no edge between them can run in parallel,
  one agent each, all on the same branch.
- **No edge to a person.** A human or external state is never a node (see `actionability.md`).
- **Read it back:** `cn list --parent <epic> --depth all --format json`, then check every `blocked_by`
  resolves, nothing is circular, and the ready set (`cn ready --toon`) is what you expect.

`cos flows` and the cos Flows tab draw each epic's graph with its status, the ready set marked.
A bead added during Build is a missed node; Reflect counts them.

## Context is a graph

When the repo has `graphify-out/` (run the `graphify` skill to build it), Think and Spec query it
before reading files: which modules the idea touches, what depends on them, where the boundaries
are. Cite the graph's nodes in the brief and design so a reviewer can follow them.
