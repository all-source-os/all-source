# Scoped local event erasure

## Authority and release status

The FormLab owner approved targeted storage-erasure implementation and synthetic
verification. Production deletion is not authorized. This work does not establish
complete account-data erasure or authorize a public compliance declaration.

## Implemented boundary

The owner selected operator-run maintenance and approved one AllSource branch/PR.
The startup seam keeps erasure preflight in `erasure_maintenance`; the existing
store constructor delegates through a fallible entry point. Projection registration,
storage initialization and replay remain in the existing constructor. Extracting
those blocks is outside this scoped erasure change.

`infrastructure::persistence::erasure` rewrites one explicitly selected local WAL
segment or partitioned Parquet file. The scope requires an exact tenant and
explicit entity deletions or exact-event content replacements. It has no network
endpoint or automatic production execution path.

- WAL entries must deserialize and pass the engine's checksum validation. The
  checksum covers only sequence, timestamp and event ID, so a corrupted tenant,
  entity or payload field that still parses is not detected, and such a record
  is matched by its corrupted value. Other tenants and streams retain their
  event records and sequence numbers.
- Parquet event files and compaction snapshots retain their Arrow schema and all
  columns while removing matching entity rows. Tenant identity comes from the
  partition path. Another tenant's archive is rejected.
- The replacement is written privately, synced, atomically renamed and followed
  by a directory sync. Source identity and digest are checked before replacement.
  An error before rename leaves the original intact. A retry after rename is
  idempotent.
- A synced sequence floor prevents erased WAL tails from causing sequence reuse
  after recovery. Recovery must use a binary that understands this floor.
  Erasing a quarantined WAL file writes its floor inside `quarantine/`, which
  recovery does not read; quarantine is never replayed, so no sequence is reused.
- Symlink ancestors, symlink files and hard-linked source files are rejected.
  Archive and backup inputs have a 64 MiB file limit; backup metadata, receipts
  and the manifest have a 1 MiB limit. WAL segments have an 80 MiB limit: the
  default 64 MiB rotation size plus 16 MiB for the final line. A store configured
  with a larger rotation size, a single event above 16 MiB, or a segment that grew
  after a failed rotation is refused, not truncated. Archives have a 128 MiB
  declared decoded limit.
  Archive rewrites keep the source file's Parquet compression codec.
  Every operation takes one caller-owned deadline.

`allsource-erase` inventories declared local copy sets before mutation. It handles
valid quarantined WAL records, retained archive originals and snapshots, compressed
Core backups and their metadata. Matching tenant feed and version indexes are
invalidated. Unknown files, corrupt input, unmounted external copies and overlapping
roots block the operation. A preview digest binds approval to the inspected bytes.

An interrupted-operation marker blocks patched Core startup and backup restore
until the same manifest has been retried and verified. A separate backup journal
repairs the data/metadata publication gap. Interrupted replacement and metadata
temporaries with this tool's exact UUID names are discarded as derived files,
with or without a marker; other `.erasure-*.pending` names are refused.
Reports contain counts, digests and directory names, not event payloads.

All writers, replicas and backup jobs must be stopped during maintenance. File locks
exclude competing maintenance commands; they do not stop running Core processes.
These operations do not provide a multi-file transaction or live-store cache
invalidation. Run with the Core filesystem owner and keep consumers stopped after
any failure.

## Operator procedure

This procedure requires separate authorization for the specific production request.
The current approval covers implementation and synthetic tests only.

1. Resolve the exact tenant and exclusively owned entity streams through the
   application's ownership rules. Do not erase a shared program or conversation
   merely because one event mentions the account. For a shared stream, resolve
   individual events and approve replacement payloads/metadata under the
   application's ownership rules; use a version-2 manifest as described below.
2. Inventory every replica, retained backup, volume snapshot and cold-tier copy.
   Restore external copies into isolated local directories or retire them under
   an approved retention procedure. Any remaining external copy blocks completion.
3. Install a Core binary containing the startup barrier and sequence-floor support
   on every recovery path. Stop Core writers, query services, consumers and backup
   jobs. Record that they are stopped; `--writers-stopped` is an operator attestation,
   not a remote service-control command.
4. Prepare a private, mode-0600 manifest using canonical absolute directories.
   `backup_dir` must be present: use `null` only after confirming that this copy has
   no local backup directory. `external_copies` must also be present and empty.

   ```json
   {
     "version": 1,
     "tenant_id": "synthetic-tenant",
     "entity_ids": ["synthetic-account", "synthetic-workout"],
     "copies": [{
       "wal_dir": "/srv/erasure-fixture/wal",
       "storage_dir": "/srv/erasure-fixture/storage",
       "backup_dir": "/srv/erasure-fixture/backups"
     }],
     "external_copies": []
   }
   ```

5. Run `allsource-erase preview /private/path/manifest.json`. Review its counts and
   `inventory_sha256`; preview does not change stored events. Then run
   `allsource-erase apply /private/path/manifest.json <inventory_sha256> --writers-stopped`.
   The command has a 240-second deadline and refuses a stale preview.
6. If interrupted, leave services stopped. Retry preview and apply with the same
   manifest and a fresh inventory digest. A crash before the first marker is
   published can leave a UUID-named temporary with no marker; the retry discards
   it. Never manually remove maintenance markers to make Core start or restore a
   backup. A different manifest is rejected while work is unfinished.
7. Require `phase: "verified"`, `matches: 0` and `local_copies_verified: true`.
   Verify recovery through Core and the backup restore engine before restoring
   service. Clear or rebuild query-service caches and reset affected feed consumers
   to the new tenant-feed epoch; never replay a pre-erasure copy.
8. Record the non-payload receipt against the request. Keep the private identity
   manifest only under the application's approved audit-retention policy. Mark the
   account request complete only after associated application records and every
   retained copy have also been resolved.

Verification covers logical files reachable through the declared copy inventory.
It makes no claim about physical disk blocks, provider-retained snapshots, external
caches or copies omitted by the operator.

The `__system` directory under a storage root is excluded by design. It holds the
system store: users, tenants, API keys, configuration, subscriptions, quotas,
billing metadata and audit records, several of which carry tenant identifiers and
account data. None of it is changed by this command, so an account's user record,
keys and audit trail remain after erasure until the application removes them
through the system streams. The preview and receipt list the directory in
`excluded_system_dirs`.
A system data directory set with `ALLSOURCE_SYSTEM_DATA_DIR` outside the storage
root is not inventoried and the manifest has no field for it; the operator must
establish that directory's contents separately. The tenant name `__system` is
rejected.

Verification also checks that removals and replacements match the preview's
counts. The post-apply scan rereads the same bytes the rewrite used, so it cannot
detect a record the classifier misses. Pre-erasure archive-repair receipts retain
their original fingerprints; rollback through those receipts must fail after file
contents change. Do not restore an old original to make a receipt match.

## Exact-event shared-record redaction

Version-1 manifests retain their deletion-only behavior. Version 2 also accepts
`redactions`, an array of records with these fields:

| Field | Required value |
|---|---|
| `event_id` | Exact UUID resolved through the store's query engine |
| `entity_id` | Exact shared stream containing that event |
| `expected_content_sha256` | Canonical digest of the approved original payload and metadata |
| `payload` | Complete replacement JSON payload |
| `metadata` | Complete replacement metadata, or `null` to remove it |

Use `erasure_redaction::content_sha256` for the digest. It hashes the JSON pair
`[payload, metadata]` after recursively sorting object keys. The replacement must
have a distinct digest. Every manifest is bounded by the existing 1 MiB limit.
Duplicate event IDs and overlap between a redacted stream and an erased stream
are rejected. With only redactions, `entity_ids` may be empty.

Event UUID, tenant, entity, event type, timestamp and version remain unchanged.
Only payload and metadata are replaced in WAL, Parquet, snapshots, retained
originals and backups. Other owners' events remain unchanged. A stale source hash,
wrong stream or target absent from the declared inventory rejects preflight.
Already-replaced copies are accepted on retry; remaining original copies still
must match the approved source hash. The receipt counts pending replacements in
`redactions`; final verification requires both `matches: 0` and `redactions: 0`.

The engine does not infer ownership or decide what to anonymize. The application
must supply replacements that still replay into valid shared records and contain
no departing user's personal content. Preserve reusable program structure and
other clients' fields. Validate replay through the application's projections as
well as Core before declaring account fulfilment complete.

## Verification

Thirteen per-file erasure tests (`local_erasure_tests`) and seventeen
command/copy-set tests (`erasure_maintenance_tests`) pass, with 271 persistence
library tests (WAL, backup, feed, checkpoint, archive and system store). Core
Clippy with `-D warnings` and formatting pass. Fixtures cover repeated Core
restarts, restored backups, exact tenant/stream isolation, idempotent retry,
erased-tail sequence preservation on data-WAL startup and system checkpoint
recovery, corrupt-input refusal, stale previews, interrupted data/metadata
publication, temporaries left before the first marker, foreign markers, backup
restore refusal while a marker is present, WAL segments above the archive byte
cap, codec preservation, the excluded system store, unfinished-operation startup
refusal and private error output.
Synthetic tests read or change no production files. Cargo reports existing
future-compatibility warnings in `nix` and `proc-macro-error2`.

The shared-record follow-up reran all 30 per-file/maintenance tests and adds six
passing `erasure_redaction_tests`. These cover two Core restarts, backup restore,
unchanged neighboring owners/tenants, all five local copy forms, partially applied
redaction, stale hashes, missing targets, wrong streams, ambiguous manifests and
canonical JSON hashes. The persistence-library rerun passes 269 tests with two
explicitly ignored: 305 passing tests total. Scoped Core library/test Clippy passes with `-D warnings`.
The owner approved publishing the shared-record follow-up through a fresh branch
and pull request. Production execution remains outside that authorization.

## Remaining account-erasure boundary

Before this can fulfill an application account-deletion request:

1. The application must resolve and persist an exact ownership manifest, including
   associated records in other streams, without deleting unrelated owners' data.
2. The operator must establish the real deployment's complete copy inventory,
   including remote cold tiers and volume snapshots. Local inventory handling is
   tested; no production inventory or retention claim has been verified.
3. External query-service caches must be purged or rebuilt, and consumers must not
   replay pre-erasure copies. Local tenant feed/version invalidation is tested.
4. The workflow must verify zero remaining matches through the relevant engines,
   including restored backups, and record completion without retaining payloads.

The owner selected operator-run maintenance. Fully online erasure and broader
replica/backup coordination remain outside scope.
