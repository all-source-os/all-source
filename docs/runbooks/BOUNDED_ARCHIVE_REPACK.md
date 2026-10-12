# Bounded archive output and recovery

Core's strict archive reader accepts finalized files up to 32 MiB. Flush and
compaction stage all replacement parts, including their footers, before
publication. Oversized individual rows are refused. Aggregate archive limits
remain unchanged: 256 MiB compressed, 128 MiB decoded JSON text and 600,000 rows.
Splitting a file does not make an archive above an aggregate limit admissible.

## Publication and crash recovery

Multipart output has a durable pending-generation marker before its first
rename. Strict readers refuse incomplete publication; tolerant reads exclude
uncommitted parts. A returned publication failure withdraws owned renamed parts.
Boot recovery removes pending generations while their WAL or source inputs
remain recoverable. A completed generation becomes readable only after all
parts are durable. Compaction removes its inputs after publication succeeds.

Compaction's read-only storage handle does not run boot cleanup against another
writer's staged files. Compaction, repack and rollback share a process mutation
guard. Per-directory OS locks fence archive replacement across processes.

## Oversized existing files

Every tenant sweep checks oversized files before the small-snapshot threshold
and minimum candidate count. Repair streams Arrow batches without converting
them into new events or applying retention. Schema, event IDs and versions are
preserved. Repair charges actual decoded JSON text using the strict reader's
accounting and refuses excessive rows, bytes or elapsed work.

The original is retained as `<source>.original.<generation>`. A durable JSON
receipt records original and replacement fingerprints before publication.
Retries validate an existing receipt and finish or roll back incomplete work.
Backups and receipts are outside the `.parquet` reader namespace.

Replacement names preserve raw-versus-snapshot classification. Receipts retain
the original file age for time-based selection; each partition's receipts are
indexed once per sweep. Failed repairs or age receipts leave their files intact
while healthy groups can compact, and the sweep still reports the error.
Local cold-archive writes use unique filenames so groups sharing a timestamp
range cannot replace one another.

## Operator commands

Retain a volume backup and stop Core before operator maintenance. The binary is
included alongside `allsource-core` in community and enterprise images.

```console
allsource-repack-archive inspect /data/storage/TENANT/MONTH/snapshot.parquet
allsource-repack-archive repack /data/storage/TENANT/MONTH/snapshot.parquet
allsource-repack-archive rollback /data/storage/TENANT/MONTH/snapshot.parquet.repair.GENERATION.json
```

`inspect` reads Core's Parquet metadata without changing files. Declared
uncompressed bytes are diagnostic metadata, not proof of the actual decoded-text
budget. `repack` prints its receipt and counters; its operation budget is 240
seconds. Command wrappers must terminate and reap it within 300 seconds.

Rollback validates the original and each surviving replacement before mutation.
It restores the byte-identical original and retains replacements with a
`.parquet.rolled-back` suffix. Repeating rollback is safe. Changed originals,
changed replacements, occupied destinations or replacements consumed by later
compaction cause refusal. Restoring an oversized original also restores its
strict-read refusal; rollback is recovery of original data, not a read-budget
override.

After maintenance, restart Core and exercise a strict retained-entity read,
conditional append with the actual latest version, stale-version refusal and a
fresh reload. Record exact binary revision, receipt fingerprints, response codes
and measured durations. A successful repack alone does not prove tenant-wide
admission or application delivery.
