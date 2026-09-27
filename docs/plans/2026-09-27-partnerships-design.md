# Partnerships workspace

## Purpose

The founder needs one admin workspace for commercial channel exploration: VCs,
family offices, accelerators and corporate pilots. These are potential routes to
customers, not investors being asked to fund AllSource. The product is an event
store database for event sourcing; agent memory is one workload.

## Chosen shape

Add `/partnerships` alongside, not inside, the existing at-risk customer outreach
screen. A searchable ranked register opens a record with research evidence,
limitations, score provenance, exact message history and a next action. Operators
can create and update records, record messages/outcomes, import reviewed JSON and
copy a task prompt for the repository's research/outreach skill.

A read-only document shelf would not track outcomes. A new bulk-mail engine would
duplicate existing authenticated channels. This workspace tracks work; the skill
researches, drafts, obtains current approval, sends through an available connector
or browser and records verified results. No scheduler or provider credentials are
introduced. A saved draft or old approval never authorises a new external send.

## Data and privacy

Control Plane owns admin-only `/api/v1/admin/partnerships` routes and stores
versioned snapshots in Core's `admin-partnerships` operator stream. One canonical
website hostname identifies each organisation. Expected revisions prevent lost
updates. Historical non-draft messages cannot be silently edited or removed;
corrections belong in notes/new entries. Lists paginate the underlying event
stream and fail closed on incomplete reads. No browser local-storage database.

Real research and correspondence are imported through the authenticated admin
API, never embedded in client bundles, public fixtures or Git. Private migration
files live under ignored `.local/`. Tests use fictional records. Public docs and
the skill contain workflow/schema, not real sent message bodies or mailbox IDs.

## Interaction and evidence

Use the existing admin shell, blue semantic tokens, readable text, visible focus,
mobile stacking and bounded line lengths. Preserve the logo. Rank scores are
evidence-snapshot sorting aids, not conversion probabilities. Unknown scores,
unchecked replies and unverified sends remain unknown. Sent is not delivered,
read, interested or paid. A contact can be suppressed with Do not contact.

Imports are explicit, create-only and per-record: show counts and conflicts;
never overwrite newer state on retry. Mutations acknowledge success only after
Core persists them. Storage errors and stale edits remain actionable on screen.

## Verification

Go tests exercise validation, private stream scoping, pagination, revisions,
duplicate creation and immutable message history. Admin unit tests cover ranking,
unknown outcomes and import parsing. Browser tests cover create/edit/import,
failure states, mobile overflow and approval-gated skill handoff. No tests send
external messages. Build and type-check the admin app; run scoped Go tests before
push. Deployment and real-data import must be reported separately from local QA.
