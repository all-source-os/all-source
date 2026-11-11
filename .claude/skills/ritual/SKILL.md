---
name: ritual
description: "The one flow from an idea to a shipped change: Think, Spec, Plan, Build, Review, Ship, Reflect, with a file handed between each step and a Chronis epic as the flow's anchor. Use for any planned work: designing a feature, writing a spec or PRD, cutting beads, working the ready beads, reviewing, shipping, or a retro. Triggers on: plan this feature, brainstorm, spec out, write a design, create a prd, create beads, convert prd to beads, work the beads, what step is this in, ship it, reflect, retro."
---

# ritual

One flow for planned work. A flow is one Chronis epic; every step hands the next one a file,
and the epic's description names each file. `cos`
([chief-of-staff](https://github.com/decebal/chief-of-staff-v2), optional) reads every flow on
the machine and shows which step it is in (`cos flows`); every step also works without it.

This skill routes. It names the step, loads that step's reference, and stops at each gate.

**Requires** Chronis (`cn`, `cargo install chronis`). **Optional:** EGM, `cos`, graphify,
claude-guard; [ritual-full](../ritual-full/SKILL.md) turns them all on. Install and a worked
example: [docs/ritual-zero-to-hero.md](../../docs/ritual-zero-to-hero.md).

## Find the step

Run `cos flows --json` when `cos` is installed: each flow carries its repo, epic and inferred
step. Without `cos`, read the epic: `cn show <epic> --toon` and
`cn list --parent <epic> --depth all --toon`.

| Step | Reference | You are here when |
|---|---|---|
| Think | [think.md](references/think.md) | there is an idea and no brief, or no epic yet |
| Spec | [spec.md](references/spec.md) | a brief exists and the epic is not approved |
| Plan | [plan.md](references/plan.md), [beads.md](references/beads.md) | the epic is approved and has no beads |
| Build | [build.md](references/build.md) | beads are open or in progress |
| Review | [review.md](references/review.md) | every bead is done and no clean review is newer than the last change |
| Ship | [ship.md](references/ship.md) | the newest review is clean |
| Reflect | [reflect.md](references/reflect.md) | the PR merged (`ritual-shipped:`) and no report is named |

`drift` means the design changed after approval: go back to Spec for a new review and approval.

A bug fix with a clear reproduction skips Think and Spec: investigate with
`systematic-debugging`, then Build. A one-line or mechanical change needs no flow at all.

Report the step in one line before acting: `ritual: <repo> <epic> is at <step> because <fact>`.

## What the steps hand over

| Step | Writes | Names it on the epic as |
|---|---|---|
| Think | `docs/plans/YYYY-MM-DD-<slug>-brief.md` | `ritual-brief: <path>` |
| Spec | the flow's branch; `…-design.md` and `…-design-review.md` committed on it | `ritual-branch: <branch>`, `ritual-design: <path>`, then `ritual-approved: …` (written by `cos ritual approve`, on the user's decision) |
| Plan | `…-plan.md` (PRD, gates, testing trophy) on the branch; beads under the epic | `ritual-plan: <path>` |
| Build | commits on the branch | the beads, closed with a reason |
| Review | `…-code-review.md`, or a review on the PR | `ritual-review: <path or PR> clean` or `… findings` |
| Ship | the PR and its evidence (`pr-proof`) | `ritual-pr: <url>`, then after the merge `ritual-shipped: <commit>` |
| Reflect | `docs/reports/YYYY-MM-DD-<slug>.md` | `ritual-report: <path>` |

Name a file with `cn task edit <epic> --append-description "ritual-<key>: <value>" --toon`; the
newest line of each key wins. An epic with no `ritual-` line is a legacy flow from before the
ritual, and `cos flows` reads its step from its beads alone. Every new file under `docs/` gets a
line in the repo's docs index in the same commit.

## The gates

- **Spec → Plan:** `cos ritual check <epic>` exits 0. Without `cos`, the epic shows `approved`
  in `cn show` and the design is unchanged since its `ritual-approved:` digest. Never cut beads
  for an unapproved design. The approval is the user's decision: run `cos ritual approve` (or
  `cn approve` without `cos`) only when the user explicitly tells you to approve in this
  conversation, quote that instruction in your report, and never approve on your own initiative.
- **Build → Review:** every bead done, and the repo's tests ran. A compile, a lint or a
  `--no-run` build is not a test run.
- **Review → Ship:** a reviewer with fresh context returned clean, newer than the last change.
- **Ship:** one PR, one commit, the test plan ticked, pr-guard satisfied.

A human approves the design; nothing else in the flow waits on a person. Manual and external
actions never become beads ([actionability.md](references/actionability.md)).

## EGM

The Elephant-Goldfish Model plugin (`egm@egm`) is the deeper Think and Spec path. Its skills set
`disable-model-invocation: true`, so the user starts them: tell them the exact command
(`/egm:egm-brainstorm`, `/egm:egm-draft`, `/egm:egm-review`, `/egm:egm-resolve`,
`/egm:egm-drift-report`). EGM writes `concepts_brief.md` and `design_draft.md` in the working
directory; move them to the dated paths above and index them.

## Graphs

The work is a graph and should be planned as one: beads with `blocked_by` edges, the ready set
on top. Read [graph.md](references/graph.md) before cutting beads, and use the repo's
`graphify-out/` graph for context in Think and Spec when it exists.

## Repository notes

Repositories differ in version pins, ticket keys, release paths and CI-only steps. Keep those
facts per organisation in `references/<org>.md`, shaped as [repo-notes.md](references/repo-notes.md)
describes, and read it before Build or Ship.

## Where a flow lives

- **The epic lives in the main checkout.** Chronis data is untracked, so every linked worktree has
  its own empty store that dies with it. Before `cn task create`, confirm
  `git rev-parse --git-common-dir` and `git rev-parse --git-dir` print the same directory; if not,
  run it from the main checkout. A repo without `.chronis/` runs `cn init` first.
- **The files live on the flow's branch,** cut at the start of Spec in its own worktree. The
  design, its reviews and the plan are committed there and merge with the feature's PR. `cos`
  reads the design from that branch while it exists.
- `cos ritual check` and `cos ritual approve` find the epic's store from a worktree too: `--repo`
  defaults to the main checkout.

## cos roles

`cos` can hire `ritual-shaper` (Think and Spec drafts) and `ritual-goldfish` (Spec review), then
`builder`, `reviewer`, `release-clerk` and `docs-keeper` for Build, Review, Ship and Reflect. Hire
with `--repo <the flow's worktree>` so the hire's worktree is cut at the flow branch, design
committed. A hired role drafts files but cannot write the repo's task store, so cutting beads
stays interactive. It is a separate process, so a hired Goldfish never sees the author's
conversation.
