# SDK contract fixtures

Shared request/response pairs for the
[latest-state read contract](../../docs/contracts/latest-state/README.md),
sections 2 to 4. The Go, Python, Rust and TypeScript SDK test suites all read
these files, so the four clients are tested against one set of bodies and a
change to the wire shape is made once, here, next to the contract.

## Rule for every SDK test suite

**Load every `*.json` file in `fixtures/`.** A suite enumerates the directory
and fails when a file has no test that consumes it, so a fixture added for one
SDK cannot be silently skipped by the other three. Never hard-code a list of
fixture names.

For each fixture the test stubs the HTTP transport, calls the SDK method that
the fixture describes, and asserts that:

1. the SDK sent `request.method` to `request.path`;
2. the SDK sent every key in `request.query` with that value (extra query
   parameters the SDK always sends, such as defaults, are allowed only when
   the fixture's description says so);
3. the SDK sent every header in `request.headers` (header names compare
   case-insensitively; the SDK's own `Authorization`, `User-Agent` and
   `Content-Type` are ignored);
4. the SDK sent `request.body` as JSON equal to the fixture value, or no body
   when it is `null`;
5. given `response`, the SDK returned the typed result, `NotModified`, or the
   typed error the description names.

## File format

One file per call. Every file has exactly this shape:

```json
{
  "name": "view_latest",
  "description": "What the call is and what the SDK must return.",
  "request": {
    "method": "GET",
    "path": "/api/v1/views/ops_status/entities/probe-a%3A2026-10-27",
    "query": {},
    "headers": {},
    "body": null
  },
  "response": {
    "status": 200,
    "headers": { "ETag": "\"sha256:c4e2a7f91b3d5e60a8c2f4b6d8e0a1c3\"" },
    "body": {}
  }
}
```

- `name` equals the file name without `.json`.
- `request.query` values are strings, exactly as they appear on the wire.
  A comma-separated parameter such as `prefixes` is one string.
- `request.path` is percent-encoded with every byte outside
  `[A-Za-z0-9-._~]` encoded in a path segment. SDKs differ on whether `:` is
  encoded, so compare paths after percent-decoding each segment.
- `body` is `null` when there is no body (`GET`, `DELETE`, `304`, and the
  `429` here, which Core sends with an empty body).
- `ETag` values are opaque fixture tags in the contract's
  `"sha256:<32 hex>"` form. They are not the hash of the fixture body; an SDK
  never recomputes a tag, it only stores and resends it.

## The sync scenario

`sync(view)` must be derivable from the fixtures alone:

1. `view_entities_page_1`: first snapshot page, `next` is the cursor for
   page 2.
2. `view_entities_page_2`: `after` equals page 1's `next`; `next` is `null`,
   and its `revision` (61) is the watermark.
3. `view_changes`: `after=61`; one upsert and one deletion, new cursor 62.
4. `view_changes_empty_after_wait`: `after=62` with `wait`; nothing changed,
   `caught_up: true`, cursor stays 62.
5. `view_changes_reset`: a stale cursor answers `409 view_reset`; the SDK
   re-runs the snapshot from step 1.
6. `rate_limited_retry_after`: a `429` with `Retry-After: 1`; the SDK waits at
   least one second and repeats the identical request.

After steps 1 to 3 the caller's state is `probe-a:2026-10-26` (version 40),
`probe-a:2026-10-27` (version 62, `ok` 1431) and no `probe-z:2026-10-20`.
