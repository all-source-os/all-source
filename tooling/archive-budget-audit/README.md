# Archive budget audit

Read-only operator diagnostic for strict archive rollout compatibility. It
opens filesystem entries and Parquet footers directly, without constructing
Core, replaying WAL, running cleanup, changing archives or decoding data pages.
Output contains aggregate counts only: no tenant identifiers, paths, schema,
column statistics, event content or raw decoder errors.

```console
cargo test --manifest-path tooling/archive-budget-audit/Cargo.toml
cargo clippy --manifest-path tooling/archive-budget-audit/Cargo.toml --all-targets -- -D warnings
cargo run --manifest-path tooling/archive-budget-audit/Cargo.toml -- /path/to/storage
docker buildx build --platform linux/amd64 --output type=local,dest=/tmp/archive-audit-binary tooling/archive-budget-audit
```

The standalone workspace has its own lockfile and no Core dependency. Its
six input dimensions match Core `ArchiveReadLimits` at source `9a653099`:
100,000 entries, 50,000 files, 32 MiB per file, 256 MiB compressed total,
256 MiB declared uncompressed row-group bytes, 250,000 row-group rows. Counters
use checked arithmetic; a tenant exceeding a policy cap appears in the report.
Keep these defaults aligned when changing Core's policy.

The diagnostic itself refuses after 300,000 entries, depth eight, a 60-second
cooperative deadline or a footer larger than 1 MiB. Malformed footers, negative
counts, observed symlinks, unreadable entries and unattributed legacy flat
Parquet files refuse the whole report instead of returning partial success.
Files changing size/modification time during a footer read also refuse. These
checks do not provide a filesystem snapshot or forcibly cancel blocked I/O.

`system` is the reserved platform archive already known to exceed file/byte
caps. It receives filesystem counts only, avoiding hundreds of thousands of
unnecessary footer reads on the shared production machine. Its unmeasured row
and decoded-byte figures are omitted, never reported as zero. Other tenant
names are not disclosed; only aggregate totals, per-tenant maxima and counts
above each policy cap are returned.

Metadata describes declared sizes, not exact allocator RSS, actual decoded
payload size, payload integrity or cold decode latency. Footer metadata may
contain column statistics; the tool neither prints them nor reads data pages.
A corrupt-data-page fixture deliberately still passes the metadata scan, while
corrupt footers refuse. Successful output is compatibility evidence only.

Tests cover aggregate/privacy/no-mutation behavior, every policy dimension,
bad/oversized metadata, corrupt data pages, bounded traversal, symlinks and
legacy attribution. Runtime invocation is operator-only; it is not exposed by
an HTTP route or included in the production application image.
