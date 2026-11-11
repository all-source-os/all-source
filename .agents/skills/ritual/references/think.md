# Think

Turn an idea into a brief worth designing. Output: `docs/plans/YYYY-MM-DD-<slug>-brief.md` and a
Chronis epic that names it.

## Two paths

- **Here, in conversation:** follow the process below.
- **EGM:** ask the user to run `/egm:egm-brainstorm`. When it finishes, move `concepts_brief.md`
  to the dated brief path and index it.

## Process

1. Read the project first: its docs index, recent commits, the code the idea touches. If the repo
   has `graphify-out/`, query the graph for the modules involved (see `graph.md`).
2. Ask one question per message, multiple choice where possible. Cover purpose, constraints, and
   what "done" means for a user.
3. Propose two or three approaches with their trade-offs. Lead with the one you recommend and say
   why.
4. Remove anything the goal does not need.

## The brief

```markdown
# <Name>: brief

Date: YYYY-MM-DD.

## Problem
Who has it, what it costs them now.

## Goal
What changes for a user when this ships. One paragraph.

## Approaches
The two or three considered, the one chosen, and why the others lost.

## Constraints
What must stay true: performance, security, compatibility, the stack.

## Unknowns
Each with its confidence tier (rock, sand, water) and what would move it up.
```

## The epic

Create it when the brief exists, so the flow is visible from the start. Create it in the repo's
main checkout: if `git rev-parse --git-common-dir` and `git rev-parse --git-dir` differ, you are in
a linked worktree whose store dies with it. A repo without `.chronis/` runs `cn init` first.

```bash
cn task create "<Name>" --type epic --toon \
  --description "$(cat <<'EOF'
<one-paragraph goal from the brief>

ritual-brief: docs/plans/YYYY-MM-DD-<slug>-brief.md
EOF
)"
```

Then the next step is Spec ([spec.md](spec.md)).
