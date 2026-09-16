# Cold-tier archive

Where events go when retention would otherwise delete them.

## What it is

Per-tenant retention deletes events past their TTL. For tenants where deleting
is too aggressive — audit workloads, compliance, anyone who may need to answer
"what happened on day X" months later — a cold-tier archive sits between
retention and deletion. Compaction hands the dropped events to an
`ArchiveTarget` **before** removing any original Parquet file.

A failed archive returns an error, which short-circuits `compact_tenant`. The
originals stay on disk and the next pass retries. There is no archive-then-
forget path.

## Backends

| Backend | Feature | Use |
|---|---|---|
| `LocalFsArchive` | always built | a second, cheaper volume; tests |
| `S3Archive` | `cold-tier-s3` | AWS S3, Cloudflare R2, MinIO, any S3-compatible endpoint |

The S3 backend is feature-gated so the default build pulls in neither an
object-store client nor a second TLS stack.

```bash
cargo build --features cold-tier-s3
```

## Turning it on

```bash
ALLSOURCE_COLD_STORAGE_URL=s3://my-archive-bucket/allsource/cold
AWS_ACCESS_KEY_ID=...
AWS_SECRET_ACCESS_KEY=...
AWS_REGION=us-east-1
```

`r2://` is accepted as a synonym for `s3://`, because operators write the
scheme they think in. Any other scheme is rejected at startup.

Credentials, region and endpoint come from the standard AWS environment, so
pointing at R2 or MinIO is an env change rather than a code change:

```bash
# Cloudflare R2
AWS_ENDPOINT_URL=https://<account-id>.r2.cloudflarestorage.com
AWS_REGION=auto

# MinIO
AWS_ENDPOINT_URL=http://localhost:9000
AWS_ALLOW_HTTP=true
```

### A set-but-unusable URL is a hard failure

Every other compaction env var falls back to a default on a bad value, because
a wrong interval costs a slow pass. This one does not. `ALLSOURCE_COLD_STORAGE_URL`
decides whether events are copied somewhere before compaction deletes the
originals, so degrading to "no archive" would turn a typo into silent data
loss. A malformed URL, or the variable set on a binary built without
`cold-tier-s3`, stops the process at startup with a message naming the cause.

## Object layout

```
<prefix>/<tenant>/<yyyy-mm>/archive.<tenant>.<from>-<to>.parquet
```

The key is a pure function of `(tenant, from, to)`. That is what makes a retry
idempotent: compaction retrying after a transient failure overwrites the same
key with the same bytes instead of accumulating a second copy and doubling the
bill every time the network blips.

The tree mirrors the live tenant layout, so an operator pulling from cold
storage navigates it the same way.

## IAM policy

The archive only ever writes. It never lists, reads or deletes, so the
credential does not need those verbs:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "AllSourceColdTierWrite",
      "Effect": "Allow",
      "Action": "s3:PutObject",
      "Resource": "arn:aws:s3:::my-archive-bucket/allsource/cold/*"
    }
  ]
}
```

Grant `s3:GetObject` and `s3:ListBucket` to whoever *reads* the archive back,
as a separate principal. A single credential that can both write and delete the
archive defeats the point of having one.

Turn on bucket versioning and a lifecycle rule to Glacier or R2 Infrequent
Access. Versioning is the cheap insurance against a bad retention config
overwriting a window with fewer events than it held before.

## What is verified, and what is not

Six integration tests in `apps/core/tests/cold_tier_s3_tests.rs` drive the real
archive path against an in-memory object store: the deterministic key, that a
retry overwrites rather than duplicates, that two tenants never collide, that
the uploaded object is genuinely Parquet (`PAR1` at both ends), that an empty
window is a no-op, and that the scratch directory does not accumulate encodes.

**Not covered: the S3 wire protocol** — request signing, path-style versus
virtual-host addressing, endpoint overrides. That belongs to `object_store` and
needs a real endpoint. Before trusting this against R2 or a new provider, do
one live round trip:

```bash
docker run -d -p 9000:9000 -e MINIO_ROOT_USER=minio -e MINIO_ROOT_PASSWORD=minio123 \
  minio/minio server /data
# create the bucket, then point a Core build with --features cold-tier-s3 at it:
ALLSOURCE_COLD_STORAGE_URL=s3://test-bucket/cold \
AWS_ACCESS_KEY_ID=minio AWS_SECRET_ACCESS_KEY=minio123 \
AWS_ENDPOINT_URL=http://localhost:9000 AWS_ALLOW_HTTP=true AWS_REGION=us-east-1 \
  cargo run --features cold-tier-s3
# trigger a compaction pass, then confirm the object landed:
aws --endpoint-url http://localhost:9000 s3 ls s3://test-bucket/cold/ --recursive
```

R2 in particular is worth checking rather than assuming: it wants
`AWS_REGION=auto` and rejects some addressing styles S3 accepts.

## Related

- Disk pressure on the hot volume is alarmed separately, see
  [ALERTS.md](./ALERTS.md). A volume that refills immediately after an extend is
  a retention problem, and this is the retention answer for tenants who cannot
  simply delete.
