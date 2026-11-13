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
segment or partitioned Parquet file. The scope requires an exact tenant and a
nonempty list of exact entity IDs. It has no network endpoint or automatic
production execution path.

- WAL entries must deserialize and pass the engine's checksum validation. Other
  tenants and streams retain their event records and sequence numbers.
- Parquet event files and compaction snapshots retain their Arrow schema and all
  columns while removing matching entity rows. Tenant identity comes from the
  partition path. Another tenant's archive is rejected.
- The replacement is written privately, synced, atomically renamed and followed
  by a directory sync. Source identity and digest are checked before replacement.
  An error before rename leaves the original intact. A retry after rename is
  idempotent.
- A synced sequence floor prevents erased WAL tails from causing sequence reuse
  after recovery. Recovery must use a binary that understands this floor.
- Symlink ancestors, symlink files and hard-linked source files are rejected.
  Inputs have a 64 MiB file limit and a 128 MiB declared decoded-archive limit.
  Every operation takes one caller-owned deadline.

`allsource-erase` inventories declared local copy sets before mutation. It handles
valid quarantined WAL records, retained archive originals and snapshots, compressed
Core backups and their metadata. Matching tenant feed and version indexes are
invalidated. Unknown files, corrupt input, unmounted external copies and overlapping
roots block the operation. A preview digest binds approval to the inspected bytes.

An interrupted-operation marker blocks patched Core startup until the same manifest
has been retried and verified. A separate backup journal repairs the data/metadata
publication gap. Interrupted replacement files are discarded only under a matching
maintenance marker. Reports contain counts and digests, not event payloads.

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
   merely because one event mentions the account. Shared-record redaction requires
   separate application support.
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
   manifest and a fresh inventory digest. Never manually remove maintenance markers
   to make Core start. A different manifest is rejected while work is unfinished.
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
caches or copies omitted by the operator. Pre-erasure archive-repair receipts retain
their original fingerprints; rollback through those receipts must fail after file
contents change. Do not restore an old original to make a receipt match.

## Verification

Seventy-seven tests pass: nine per-file erasure tests, nine command/copy-set tests,
seven WAL durability tests and 52 backup/feed/checkpoint/archive recovery regressions.
Scoped Core Clippy, formatting and whitespace checks pass. Fixtures cover repeated Core restarts, restored
backups, exact tenant/stream isolation, idempotent retry, erased-tail sequence
preservation, corrupt-input refusal, stale previews, interrupted data/metadata
publication, unfinished-operation startup refusal and private error output.
Synthetic tests read or change no production files. Cargo reports existing
future-compatibility warnings in `nix` and `proc-macro-error2`.

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
