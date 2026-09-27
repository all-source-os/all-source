# Partnerships operator workspace

Open **Admin → Partnerships** (`/partnerships`). This tracks commercial channels
for AllSource as an event-store database: relevant VC platforms, family offices,
accelerators and corporate pilots. It is separate from customer-retention
Outreach and design-partner applications.

## Daily work

1. Search before creating a firm. Website hostname is its stable identity.
2. Inspect dated sources, limits and scoring provenance. Priority is fit +
   channel leverage + access, out of nine, not a win probability. Paid demand is
   separate. Unscored and unchecked mean unknown.
3. Keep the next action and stage current. Record exact drafts, verified sends,
   inbound replies and uncertain outcomes under Messages. Saving does not send.
4. Use **Copy research task** to invoke `$allsource-partnerships` in Codex with
   this repository open. The repo-local skill is in
   `.agents/skills/allsource-partnerships/SKILL.md`; restart skill discovery if
   the current session predates its creation.
5. Review recipient, channel and exact text before approving an external send.
   Historical approval never authorises a new message. Check uncertain sends
   before retrying; honour opt-outs with **Do not contact**.

The skill can research, draft, send through available authenticated connectors or
browser controls after approval, and record proof. It is not an automatic mailing
service, scheduler, login bypass or installed set of provider credentials. Jev
scoring requires an actual compatible configured evaluator; absent one, the score
remains unknown or explicitly human-assessed.

## Import previous research

Use **Research & send workflow → Import private research and send history**.
Choose a reviewed `{ "records": [...] }` JSON file, check the organisation names,
then explicitly import. Matching hostnames are rejected. Each record saves
independently, so the result shows exact created/not-imported counts.

The Rust `tooling/partnership-import` converter accepts an existing evidence
packet, Jev results and a Markdown send ledger, and writes a private staging file:

```console
cargo run --manifest-path tooling/partnership-import/Cargo.toml -- \
  /private/path/evidence.json /private/path/jev-judgments.json \
  /private/path/send-ledger.md .local/partnerships-import.json
```

The converter performs no network requests. It preserves exact messages, actual
model scores, confirmation evidence and timestamp precision; missing legacy
message proof stays in notes, not a fabricated sent record. Review generated
records before importing. The optional use-case test validates them without
persisting them (`PARTNERSHIP_IMPORT_PATH=<absolute file>` with
`go test ./internal/application/usecases -run TestPartnershipPrivateImportValidation -v`
from `apps/control-plane`). Private staging is ignored by both Git and Docker.

## Storage and recovery

CP admin JWT middleware protects all three routes:

- `GET /api/v1/admin/partnerships`
- `PUT /api/v1/admin/partnerships` (`record`, `expected_revision`)
- `GET /api/v1/admin/partnerships/:id/history`

The same-origin admin BFF forwards the httpOnly session; tokens never enter page
JavaScript. Core stores revisions under tenant `admin-partnerships`, event
`partnership.record_saved`, entity `partnership:<hostname>`. Existing operator
backup, access and retention controls apply. No browser-local database or public
research payload is bundled. This is operator-only, not an end-customer feature.

Revision conflicts retain the form draft. Read current history, reconcile, and
save against the new revision. Recorded nondraft interactions cannot be removed
or rewritten by ordinary edits. Add corrections to notes. Do-not-contact cannot
be lifted through this workflow. If future retention/legal deletion is needed,
handle it through the existing privileged data-governance process, not a bulk
delete button or event-history rewrite.

On a network timeout, read current record/history first. Never retry an external
message just because its local record failed to save. If storage is unavailable,
the UI reports an error instead of claiming the register is empty. Core scans
are bounded at 50,000 revisions and fail closed beyond that bound; introduce a
dedicated projection before approaching it. Current per-firm limits: 50 sources,
200 interactions. Score/source snapshots are dated research, not live demand.

## Verification

Run the Control Plane suite, admin type-check/build, Bun unit tests and
`apps/admin/e2e/partnerships.spec.ts`. Browser tests use synthetic data and mocked
API responses, not live outreach. Production rollout requires both Control Plane
and admin. Verify live authentication and a real imported record separately from
mocked browser checks. Logo and shared theme are not changed by this feature.
