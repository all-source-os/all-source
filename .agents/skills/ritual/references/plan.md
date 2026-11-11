# Plan

Turn the approved design into stories an agent can finish one at a time. Output:
`docs/plans/YYYY-MM-DD-<slug>-plan.md` committed on the flow's branch, then beads
([beads.md](beads.md)).

The plan is its own file. Never edit the approved design to add it: any change to the design's
bytes reads as `changed since approval` and sends the flow back to Spec.

## Gate first

```bash
cos ritual check <epic>
```

Exit 0 means approved and unchanged. `unapproved`, `changed since approval` or `missing design`
means go back to Spec. Without `cos`, compare `git show <branch>:<design> | shasum -a 256` with the
epic's newest `ritual-approved:` digest and confirm `cn show <epic>` reports it approved.

## Questions

Ask what the design leaves open, one set at a time, with lettered options so the user can answer
"1A, 2C". Two are required every time:

**Quality gates, in two tiers.**

```
Which commands validate the whole codebase, once, when every story is done (epic-level)?
   A. the repo's CI entrypoint (make ci / task ci / bun run ci)
   B. bun typecheck && bun lint
   C. cargo test
   D. Other

Which checks apply to single stories (story-level)?
   A. UI stories: verify in the browser (Playwright or the repo's e2e runner)
   B. Backend stories: call the endpoint and check the response
   C. Migration stories: the column or table exists after the migration
   D. Other
```

Epic-level gates run once: intermediate stories may leave the type checker red for the next one
to fix, and running the whole suite per story spends the agent's context.

**Test layers (the testing trophy).**

```
Which layers does this epic need?
   A. Unit only
   B. Unit + contract
   C. Unit + contract + integration
   D. Unit + contract + integration + frontend + E2E

Which behaviours block merge?
   A. Background logic (bugs a user never sees)
   B. Data filtering and query correctness
   C. Orchestration
   D. All of these
```

## The plan file

`docs/plans/YYYY-MM-DD-<slug>-plan.md`, committed on the flow's branch, indexed, and named on the
epic with `ritual-plan: <path>`:

```markdown
# <Name>: plan

Design: docs/plans/YYYY-MM-DD-<slug>-design.md (approved, sha256 <digest>).

### Quality gates

#### Epic-level (once, when every story is done)
- the repo's CI entrypoint

#### Story-level
- UI stories: verify in the browser
- Backend stories: call the endpoint and check the response

### Testing trophy

#### Unit
- [ ] [Logic] <behaviour> (file: <path>)
#### Contract
- [ ] [Handler] <endpoint> returns <shape>
#### Integration
- [ ] [Flow] <cross-boundary behaviour>
#### Frontend
- [ ] [Component] <behaviour>

Block merge: <tests>. Block release: <tests>. Follow-up allowed: <tests>.

### Stories

#### US-001: <title> [Schema | Backend | UI | Integration]
As a <user>, I want <capability> so that <benefit>.

- [ ] Test: <behaviour> (file: <path>), written first and failing
- [ ] <verifiable criterion>
- [ ] <story-level gate, if one applies>
- [ ] All tests green: <command>

Mark each item [x] as you complete it. Only close when all are checked.

### Functional requirements
- FR-1: The system must …

### Non-goals
- …

### Open questions
- …
```

## Writing stories

- One story fits one agent session. If its change needs more than two or three sentences to
  describe, split it.
- Order by dependency: schema, then backend, then UI, then the views that aggregate.
- Every criterion is a checkbox an agent can verify: a file, a command's output, a response, a
  rendered state. "Works correctly", "good UX" and "handles edge cases" are not criteria.
- Every story names its own tests, written first. No "add tests" story at the end.
- UI stories carry a contract test for the data shape they consume.
- Epic-level gates never appear inside a story.

Then cut the beads ([beads.md](beads.md)).
