# System checkpoints

Core keeps tenants, API keys, config, audit and consumer cursors in the system
store (`$ALLSOURCE_DATA_DIR/__system`). Without a checkpoint every boot reads
the whole system WAL and replays every tenant event: on 2026-11-01 the boot
ran from 17:32:31 to 17:38:05 over 2,852,970 events. A checkpoint lets a boot read one file and only
the WAL written after it.

## What a checkpoint holds

`__system/checkpoint/checkpoint-<covered_seq hex>.json`:

- `covered_seq`: every system event at or below this WAL sequence is in the
  checkpoint.
- The tenant stream folded: tenants, query-usage receipts, egress accounts.
  This is the stream that grows with traffic.
- Every other system event verbatim (keys, config, audit, consumers), so their
  repositories rebuild exactly as before.
- Each WAL segment's name and the length of it that is covered.
- Event counts per domain, so `/health` keeps reporting the whole history.

The body is followed by a footer with its length and CRC-32. The file is
written to a temporary name, fsynced, renamed and the directory fsynced. The
two newest valid checkpoints are kept.

## When one is written

- At boot, once the system repositories are rebuilt and before any request is
  served, from the rebuilt state. The file is written on a background thread.
- Every `ALLSOURCE_SYSTEM_CHECKPOINT_INTERVAL_SECONDS` (default 300; 0 turns the
  timer off), by folding the tenant events since the last checkpoint onto it.
- On a graceful stop, after the HTTP server has drained.

Appends to the system store are stopped only while a checkpoint reads its
sequence, segment lengths and the resident events, not while it folds or
writes.

## How a boot uses it

1. Load the newest checkpoint whose footer, checksum and name agree. A file
   that fails is logged and skipped, never deleted.
2. Read the WAL from where the checkpoint stopped: a covered segment at its
   recorded length is not opened; a longer one is read from that length; a
   segment it did not list is read whole.
3. If the WAL contradicts the checkpoint (a covered segment is shorter, or an
   entry at or below `covered_seq` sits outside the covered bytes), every
   checkpoint is moved to `checkpoint/rejected/` and the whole WAL is replayed.
4. The next WAL sequence is above the WAL and above every checkpoint name on
   disk, rejected ones included, so no covered sequence is handed out twice.
5. The tenant fold is used only if this binary would have written it: the same
   fold format, a decode that loses or invents nothing (it re-encodes to the
   same JSON), and no recorded uninterpreted event type or payload key that
   this binary now reads. Otherwise the tenant stream is replayed from the WAL.

The first boot of an image with checkpoints has none yet. It replays the whole
WAL once, as before, and writes the first checkpoint.

## What keeps the WAL the source of truth

- The WAL is never pruned or truncated by checkpoints. Any doubt about a
  checkpoint is answered by replaying the WAL, which loses nothing.
- Segment lengths are recorded only after every segment and the WAL directory
  are fsynced, so a power loss cannot leave a segment shorter than recorded.
- A failed system WAL append stops checkpoints until the process restarts: its
  bytes may still reach the segment without the event being in memory. A
  capture also refuses whenever the WAL's sequence is ahead of the last event
  held in memory.
- A checkpoint already on disk at a sequence is never overwritten.
- The fold records every tenant event type and payload key this binary applied
  but ignored, so a later binary that reads one replays the stream instead.

## Switches

| Variable | Default | Effect |
|---|---|---|
| `ALLSOURCE_SYSTEM_CHECKPOINT` | `true` | `false`: boot ignores checkpoints and replays the WAL; none are written. The kill switch. |
| `ALLSOURCE_SYSTEM_CHECKPOINT_INTERVAL_SECONDS` | `300` | Periodic checkpoint cadence. |
| `ALLSOURCE_SYSTEM_CHECKPOINT_VERIFY` | `false` (`true` in `apps/core/fly.toml`) | 30 s after a boot that used a checkpoint, replay the whole WAL in the background and compare. On a difference: stop writing checkpoints, wait for any write in flight, move every checkpoint to `checkpoint/rejected/`, and exit with code 70 so the restart replays the WAL instead of serving state built from the bad checkpoint. |

## Rolling out

1. Deploy with `ALLSOURCE_SYSTEM_CHECKPOINT_VERIFY=true`. The first boot still
   replays the whole WAL; look for `Boot system checkpoint written at`.
2. On the next restart, check the boot used it and how long it took:
   `System store recovered: ... checkpoint Some(<seq>)` and the `Stage 1` and
   `Stage 2` timings.
3. About 30 s later, look for `System checkpoint <seq> matches a full replay`.
   If you see `differs from a full replay` instead, the process exits and the
   restart replays the WAL; keep verify on and report the tenants the log
   names. The rejected files stay in `checkpoint/rejected/` for diagnosis.
4. Verify costs one full replay of CPU and memory per boot that used a
   checkpoint. Keep it on at least until a deploy that changes tenant folding
   has logged a match.

## Rolling back

An image without checkpoints lists only `__system/wal/` and replays the whole
WAL, so rolling back loses nothing. Writes made by the older image are picked
up by the next boot of a newer one: the segments it appended to are longer
than the checkpoint recorded, or not listed.

## Disk

The WAL keeps growing as before; checkpoints do not shrink it. Each checkpoint
holds the folded tenant state plus every non-tenant system event, and two are
kept.

## Not covered

- The data WAL and Parquet storage have their own checkpoint
  (`ALLSOURCE_CHECKPOINT_INTERVAL_SECONDS`); this is only the system store.
- A tenant's `updated_at` and its in-memory usage window are stamped with the
  clock during replay, before and after this change. After a checkpoint boot
  they reflect when the checkpoint was folded, not when the process started.
