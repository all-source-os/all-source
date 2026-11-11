# Bead actionability policy

Use this reference when creating beads or auditing an existing graph.

## Test

An open bead is actionable only when an agent can start now and finish every
acceptance criterion using available repository state and authorized tools.

Agent-executable work includes code, tests, documentation, analysis from
available data, deployment already authorized by the task, browser verification,
and preparation of a decision or external-action packet.

Non-bead state includes:

- founder or human decisions, confirmation, approval, or signatures;
- legal, safety, tax, or specialist review performed by another person;
- purchases, payments, budget authorization, account creation, and paid tests;
- credentials, secrets, verification codes, 2FA, login recovery, and DNS changes
  the agent cannot perform with existing authorized access;
- outreach, posting, submission, partnership contact, or third-party replies;
- store, platform, regulator, or vendor review and approval;
- physical-device-only steps when no device-control path is available;
- elapsed time, future traffic, backlinks, customers, payments, conversions,
  signed agreements, or other external outcomes;
- a request to wait, monitor without a live polling handle, or revisit later.

## Where external actions live

Prefer an existing domain-specific file such as launch readiness, legal review,
domain setup, payment setup, mobile release, outreach, or evidence runbook.

When no suitable file exists, create `docs/manual-and-external-actions.md` with:

```markdown
# Manual and external actions

## <stable action ID>: <action>

- Source bead: `<id>`
- Owner: founder | named external role | platform
- Trigger: exact state that makes action relevant
- Action: one concrete manual or external act
- Evidence to record: receipt, URL, decision, review, credential presence, or result
- Agent follow-up after evidence exists: concrete next repository/tool action, or none
- Product/release impact: what remains unavailable while pending
- Status: pending | completed | declined | obsolete
```

Do not copy secrets, private correspondence, personal data, or proprietary review
material into this file. Link private evidence by safe reference only.

## Audit dispositions

Inspect every open and in-progress bead, not only `cn ready` output.

### Keep

Keep when all criteria are agent-executable now. `blocked_by` may contain only
task IDs whose deliverables are genuine prerequisites.

### Rewrite

For mixed beads, move manual/external criteria to docs. Rewrite title,
description, and acceptance criteria around remaining agent-owned artifact.
Preserve product scope and dependencies between executable tasks.

Example:

- Wrong: `Obtain independent legal approval`.
- Right bead: `Prepare versioned legal-review packet and correction workflow`.
- External-action record: `Qualified reviewer performs review and returns decision`.

### Migrate and close

For external-only beads:

1. Record pending action in correct docs file.
2. Check no executable implementation work remains.
3. Remove downstream dependency only by completing migration record, not by
   pretending external result exists.
4. Mark bead done with reason: `Tracking migrated to <path>; external outcome is
   pending and not claimed complete.`
5. Rewrite any downstream bead that would become ready but still depends on that
   external outcome. Usually it also belongs in docs until result arrives.

### Create later

When external evidence arrives, read docs record and create a new bead only if
there is concrete follow-up work. Include evidence reference, exact input, output,
tests, and acceptance criteria. Do not reopen a waiting placeholder.

## Graph audit

After migration:

1. `cn list --all --format json` and inspect every non-done task.
2. Verify every `blocked_by` value resolves to another bead ID.
3. Search titles, descriptions, and unchecked criteria for non-bead state above.
4. Confirm every ready bead has an immediate first action and finish condition.
5. Confirm docs records cover every migrated manual/external action.
6. Re-run audit after new beads are created.

False positives require judgment. Text that says a feature must *model* human
approval is executable; a criterion requiring an actual human approval is not.
Text forbidding outreach or secrets is a safety boundary, not a dependency.
