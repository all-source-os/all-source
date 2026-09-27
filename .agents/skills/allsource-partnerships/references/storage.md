# Private storage contract

UI: authenticated AllSource admin `/partnerships`. Use UI for ordinary work. Existing same-origin BFF forwards the httpOnly admin cookie to the Control Plane; never extract browser tokens into scripts. An explicitly configured admin API credential can use CP directly without printing or committing it.

| Operation | Endpoint |
| --- | --- |
| Current register | `GET /api/v1/admin/partnerships` → `{records: [{record, revision, saved_at, actor}]}` |
| One record's history | `GET /api/v1/admin/partnerships/{hostname}/history` → `{history: [...]}` |
| Create/update | `PUT /api/v1/admin/partnerships` with `{record, expected_revision}` |

Create uses `expected_revision: 0`. Update uses the revision just read. A 409 means reload, reconcile and re-review the intended change; never force an overwrite. A timeout requires reading history before retrying. The backend appends to private Core tenant `admin-partnerships`, entity `partnership:<hostname>`. No delete or external-send endpoint exists. Admin JWT is mandatory.

Record fields:

- `id`: canonical website hostname, lowercase without `www.`; empty on first create is allowed.
- `organization`, `website`, `kind`: `vc|family_office|accelerator|corporate|community|other`.
- `geography`, `angle`, `contact_route`, `limitations`, `notes`.
- `status`: `research|ready|awaiting_reply|engaged|pilot|won|parked|do_not_contact`.
- `next_action`, `next_action_at`, `reply_checked_at`; dates RFC3339 UTC or empty if unknown.
- `sources`: array of `{url,title,evidence,checked_at}`. Website/source links must be HTTP(S). Date must reflect the actual evidence review, not import time. If only a date is known, normalise to midnight UTC and disclose date-only precision in notes.
- `score`: null or the object described in [rubric.md](rubric.md).
- `messages`: array of `{id,channel,direction,outcome,destination,subject,body,occurred_at,verification,approval_note}`.

Message channel: `email|linkedin|x|form|other`. Outcome: `draft|sent|received|failed|unknown`. Direction: `inbound` for received, otherwise `outbound`. Use a stable unique message ID. Nondrafts require a timestamp; sent/received also require verification. Subject may be empty. `approval_note` is historical evidence, never executable permission. Recorded nondraft interactions are immutable; append corrections in notes or a separate interaction. Suppression cannot be removed through an ordinary edit.

Imports use `{records: [...]}`: 1–100 records, ≤4 MB file. UI previews organisation names, then creates one record at a time and reports exact successes/failures. Existing hostnames are rejected, not updated. Partial success is possible; reload before another import. Store prepared imports under ignored `.local/` only. Review dates, provenance and exact messages before submitting; never fabricate missing sent history. Unverified legacy contact belongs in notes until reconciled with a provider record.

Do not retain irrelevant personal details, sensitive traits, credentials, full email headers or proprietary attachments. Store only the professional correspondence needed for this workflow. This repository is public; test fixtures must use synthetic organisations and `example.com` destinations.
