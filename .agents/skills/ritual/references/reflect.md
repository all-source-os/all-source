# Reflect

What the flow cost, what it taught, and whether the design still matches the code. Output:
`docs/reports/YYYY-MM-DD-<slug>.md`, named on the epic as `ritual-report:`, then the epic closed.

## The report

```markdown
# <Name>: what shipped

Date: YYYY-MM-DD. Epic: <id>. PR: <url>.

## Shipped
What a user can now do, and the evidence.

## Graph
Beads planned, beads added during Build, and the edges that were wrong. A bead added late is a
missed edge in Plan.

## Gates
Which gates caught what. A gate that never failed on this flow is worth a look: can it fail?

## Drift
Where the code departs from the design, and which one was updated.

## Next
Follow-up work as new epics or beads, never as notes.
```

Report measured facts (commands, counts, the timeline from `cn show <epic> --toon`), never
estimates.

## Drift check

Ask the user to run `/egm:egm-drift-report` against the design, or compare the design's
interfaces with the code by hand. Fix the design in the same change.

## Close

```bash
cn task edit <epic> --append-description "ritual-report: docs/reports/YYYY-MM-DD-<slug>.md" --toon
cn done <epic> --toon --reason "Shipped in <PR>; report at <path>"
```
