# X Thread: chronis TUI + Web Viewer

## Tweet 1 (hook)

Built a task manager where every action is an immutable event.

`cn tui` gives you a ratatui dashboard.
`cn serve` gives you an HTMX web viewer.

Both read from the same event-sourced Core — no database, no ORM, just events and projections.

[attach: chronis.gif or screenshot of TUI]

## Tweet 2 (architecture)

How it works:

Every task action (create, claim, done, approve) is an event ingested into AllSource Core — a Rust event store with WAL + Parquet durability.

Projections rebuild task state from the event stream. Queries hit a DashMap (~12us). Zero SQL.

## Tweet 3 (TUI)

The TUI has two views:

Dashboard — split pane with task table + detail panel
Kanban — three columns: Open | In Progress | Done

vim keys (j/k), Tab to switch views, Enter for timeline detail, c/d/a for claim/done/approve.

Built with ratatui + crossterm.

## Tweet 4 (web)

The web viewer is an embedded Axum server with HTMX.

No React. No npm. No build step. Just ~150 lines of CSS and HTMX auto-refresh.

`cn serve --port 3905`

Dark theme. Kanban board. Task actions. 14KB of JS (HTMX bundled, works offline).

## Tweet 5 (the trick)

The neat part: TUI and web share the exact same TaskRepository trait.

```
repo.list_tasks(None)     // sync, DashMap read
repo.claim_task(id, agent) // async, ingests event
repo.get_task_detail(id)   // timeline from WAL
```

One data model. Two frontends. No glue code.

## Tweet 6 (CTA)

chronis is the local task CLI for the AllSource Chronos monorepo.

Event sourcing isn't just for distributed systems — it's the right abstraction for any workflow with state transitions.

Ship: github.com/all-source-os/chronos-monorepo

---

## Recording instructions

```bash
# From repo root:
asciinema rec --cols 110 --rows 32 docs/demos/chronis.cast -c "bash docs/demos/tui-demo.sh"

# Render to GIF:
agg docs/demos/chronis.cast docs/demos/chronis.gif --theme monokai --speed 1.5

# Or upload to asciinema.org:
asciinema upload docs/demos/chronis.cast
```
