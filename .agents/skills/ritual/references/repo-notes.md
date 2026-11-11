# Repository notes

What Build and Ship must respect in a particular repository or organisation. The flow is the
same everywhere; these facts are not. Keep them in one file per organisation,
`references/<org>.md`, and read the repo's own `CLAUDE.md` first: its rules win.

## What a notes file records

| | What to write down | Why the flow needs it |
|---|---|---|
| Runtime | The package manager and where its version is pinned (`packageManager`, `.nvmrc`, `rust-toolchain.toml`) | Build reads the pin; it never hardcodes a version |
| Monorepo | Workspaces, task runner (Turborepo, Make, Taskfile) | Beads name the right command per package |
| Tests and lint | The commands and where they run (local, CI only) | Each bead's done-when names a check that can fail |
| Ticket key | The tracker prefix and where it must appear (branch, PR title, commit) | Spec cuts the branch, Ship titles the PR |
| Tracker | `.chronis/`, plus any deprecated store still present | Plan creates the epic in the right store |
| Release path | What builds where, which steps run only in CI, which environments are gated | Ship reports dispatched and deployed separately |
| Infra | Cloud, IaC tool, how deploys authenticate | Ship never deploys from a laptop when CI owns it |

## Rules worth writing as rules

- **Read version pins; never hardcode them.** Repos in one organisation often pin different
  versions of the same tool.
- **Put the ticket key where the repo's checks look for it,** in the repo's own format (for
  example `feat/ABC-123-…`, `fix(ABC-123): …`). A title check usually enforces it.
- **Name the steps that only CI may run** (signing, obfuscation, publishing with secrets), and
  verify them by dispatching the workflow, not by reproducing them locally.
- **Record any deprecated tracker or store** and how to migrate from it safely before Plan uses
  the repo.
