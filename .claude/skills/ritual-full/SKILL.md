---
name: ritual-full
description: "The ritual flow (Think, Spec, Plan, Build, Review, Ship, Reflect on a Chronis epic) with every tool on by default: EGM for brainstorm, draft, review and drift; a Semantic ISA section in every design; two independent fresh-context reviewers; cos hires per step; graphify context; claude-guard hooks. Runs a tool preflight first and stops on anything missing. Triggers on: ritual full, full ritual, full flow, plan this feature with every tool, ritual with egm, ritual with semantic isa."
depends_on: [ritual, semantic-isa]
---

# ritual-full

The same flow as [ritual](../ritual/SKILL.md): same steps, same files, same `ritual-*` lines on
the epic, same gates. Read that skill first; this one states only what changes. Every tool below
is **on by default**. A missing tool blocks the flow until it is installed or the user says
"continue without <tool>"; never skip one silently. Zero to hero, with install and a worked
example: [docs/ritual-zero-to-hero.md](../../docs/ritual-zero-to-hero.md).

## Preflight (first thing, every flow)

Run each check, print one table, then stop if any row is `missing`:

| Tool | Check | Install when missing |
|---|---|---|
| Chronis `cn` | `cn --help` exits 0; `.chronis/` in the main checkout | `cargo install chronis`, then `cn init` in the repo |
| EGM plugin | `claude plugin list` shows `egm@egm` enabled | `claude plugin marketplace add google/elephant-goldfish-model` then `claude plugin install egm@egm`, new session |
| semantic-isa | `~/.claude/skills/semantic-isa/SKILL.md` exists | `cp -R skills/semantic-isa ~/.claude/skills/semantic-isa` from this repo |
| cos | `cos ritual check --help` exits 0 | clone `decebal/chief-of-staff-v2`, run `task setup` in it |
| graphify | `graphify --help` exits 0; `graphify-out/` in the repo, else build it | `uv tool install graphifyy` then `graphify install` |
| claude-guard | `claude-guard --help` exits 0; `~/.claude/settings.json` registers `claude-guard infra-guard` and `claude-guard bash-hygiene` | `cargo install --path gates/rust/claude-guard`, then `skillsync hooks install` |
| skills | `brainstorming`, `pr-proof`, `systematic-debugging` under `~/.claude/skills/` | `cp -R skills/<name> ~/.claude/skills/<name>` |

Report: `preflight: cn ok · egm ok · semantic-isa ok · cos ok · graphify ok · claude-guard ok · skills ok`,
with `missing (<install command>)` in place of `ok` where it applies. EGM skills set
`disable-model-invocation: true`, so for every `/egm:` step below you give the user the exact
command and wait for its output file.

## What is on, per step

| Step | On by default | Hands over (in addition to ritual's) |
|---|---|---|
| Think | user runs `/egm:egm-brainstorm`; `graphify query` on the modules the idea touches | `concepts_brief.md` moved to the dated brief path; graph nodes cited |
| Spec | user runs `/egm:egm-draft`; a **Semantic ISA** section; review by `/egm:egm-review` **and** a hired `ritual-goldfish`; resolve with `/egm:egm-resolve` or `/egm:egm-review-loop`; `cos ritual check` | design with ISA section; two review files |
| Plan | one bead per ISA sink whose done-when is its VERIFY test; graph read back ([graph.md](../ritual/references/graph.md)) | plan with the ISA test matrix as trophy rows |
| Build | `cos agents spawn --role builder` per ready bead, own worktree; claude-guard hooks on | ISA matrix rows as real tests |
| Review | `cos agents spawn --role reviewer`; sink-to-VERIFY check | code review naming each sink's VERIFY at file:line |
| Ship | `pr-proof` evidence; one PR, one commit ([ship.md](../ritual/references/ship.md)) | the evidence report |
| Reflect | user runs `/egm:egm-drift-report` on the design; new-sink check | report under `docs/reports/` |

## Spec: the Semantic ISA section

Every design gets a `## Semantic ISA` section, written with semantic-isa's DESIGN mode
([SKILL.md](../semantic-isa/SKILL.md), [template](../semantic-isa/references/isa-template.md))
and scaled to the change:

1. **Side-effect inventory:** every file write, network call, API, message, deploy and data write
   the change adds or touches. One row each, with reversibility.
2. **Instructions** in the five cores, each with a governance property and a typed binding
   (closed `input_schema` and `output_schema`).
3. **Taint:** sources (user input, fetched content, file reads, model output) and **sinks**.
4. **VERIFY before every sink:** a stated, deterministic check. "The model judged it safe" is not
   one.
5. **Violations** as recoverable exceptions: evidence recorded and the recovery path.
6. **ISA test matrix:** per sink, tainted input refused and VERIFY-cleared input admitted.

A change with no side effects says so in one line with the evidence (`no file, network, API,
message, deploy or data write: <the diff's scope>`). The Goldfish reviewers check this section
like any other; an inventory that misses a side effect visible in the code is a blocking finding.

## Spec: two reviewers, one approval

The two reviews run in separate fresh contexts and never see each other's output. Write each as
its own review file (format in [spec.md](../ritual/references/spec.md)). Approval needs both clean
against the same design digest; point `cos ritual approve --review` at the later one and name the
other in the design's review history. Resolve findings, then re-run both. The approval is the
user's: `cos ritual approve` in their terminal, or, when they tell you to approve in the
conversation, the same command with `--yes` and their instruction quoted in your report
([spec.md](../ritual/references/spec.md#approve)). Agents never approve on their own initiative.

## Review: sink-to-VERIFY check

For each sink in the design's ISA table, the reviewer finds the VERIFY in code and its test:
`rg -n '<sink call>'` lists every call site, and each one is reached only through the named check.
A sink with no VERIFY, or a call site that bypasses it, is a blocking finding. A sink in the code
that the design does not list is drift: back to Spec.

## Reflect: new sinks

Diff the shipped code's side effects against the design's inventory. A new sink, or one whose
VERIFY changed, goes in the report's Drift section and in the design, in the same change.

## Everything else

Finding the step, the `ritual-*` lines, where the epic and files live, the gates, bugs skipping
Think and Spec, and per-org notes ([repo-notes.md](../ritual/references/repo-notes.md)) are as in
[ritual](../ritual/SKILL.md).
