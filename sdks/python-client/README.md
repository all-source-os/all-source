# allsource-client

Python SDK for the [AllSource Event Store](https://all-source.xyz).

## Install

```bash
pip install allsource-client
```

## Authentication

AllSource accepts API keys and, for services, registered client keys (below). Get an API key from your dashboard at [all-source.xyz](https://all-source.xyz) (Settings → API Keys) or mint one via `POST /api/v1/auth/api-keys`. The SDK sends the key as a bearer token.

```python
import os
from allsource_client import AllSourceClient

client = AllSourceClient(
    api_key=os.environ["ALLSOURCE_API_KEY"],
    base_url="https://api.all-source.xyz",
)
```

### Registered client keys (ES256)

Registering a key, computing its kid and rotating it: [the registered client keys guide](https://github.com/all-source-os/all-source/blob/main/docs/guides/REGISTERED_CLIENT_KEYS.md).

A service that talks to the hosted gateway can authenticate with a P-256
keypair instead of a long-lived API key. An operator registers the public key
on your tenant and gives you its `kid`; the client sends a 60-second ES256
assertion signed with the private key. Registration is operator-only: see
[the client key runbook](https://github.com/all-source-os/all-source/blob/main/docs/runbooks/CLIENT_KEY_ROTATION.md) and
[the decision record](https://github.com/all-source-os/all-source/blob/main/docs/decisions/2026-11-07-tenant-clients-registered-es256-keys.md).

```bash
pip install 'allsource-client[assertion]'   # adds the cryptography package
openssl ecparam -name prime256v1 -genkey -noout | openssl pkcs8 -topk8 -nocrypt -out client-key.pem
```

```python
import os
from allsource_client import AllSourceClient, Es256AssertionProvider

with open(os.environ["ALLSOURCE_CLIENT_SIGNING_KEY_PATH"], "rb") as f:
    provider = Es256AssertionProvider.from_pkcs8_pem(
        f.read(),
        client_id=os.environ["ALLSOURCE_CLIENT_ID"],
        tenant=os.environ["ALLSOURCE_TENANT_ID"],
        kid=os.environ["ALLSOURCE_CLIENT_KID"],      # optional: defaults to the key's RFC 7638 thumbprint
        audience="https://api.all-source.xyz",       # must match the gateway's ALLSOURCE_ASSERTION_AUDIENCE
    )

client = AllSourceClient(credential_provider=provider)
```

The assertion carries `iss` = client id, `sub` = tenant, `aud`, `iat`, `exp`
(60 s after `iat`) and a fresh `jti`. The provider reuses one assertion until
15 s before it expires and is safe to share between threads and clients. Role
and scopes come from the registration, not the token.

`credential_provider` accepts any callable that returns a bearer string, so
you can supply your own token source. The client calls it on every request
attempt, retries included, and sends only `Authorization: Bearer <token>`.
`AllSourceAsyncClient` also accepts an `async def` provider. Pass exactly one
of `api_key` and `credential_provider`.

### Refusals

A refused credential raises `AllSourceError` with `status_code == 401` and
`reason` set to the gateway's reason, such as `asserted_unregistered`,
`asserted_expired` or `asserted_signature`.

After the gateway's `ALLSOURCE_LEGACY_KEY_SUNSET`, a managed key minted by
the old onboarding flow that has no registration record is refused with
`LegacyKeySunsetError` (`reason == "legacy_key_sunset"`). Dashboard keys keep
working. Register a client key as above, or create a dashboard key.

```python
from allsource_client import LegacyKeySunsetError

try:
    client.health()
except LegacyKeySunsetError as err:
    print(err)  # tells you to register a client key or create a dashboard key
```

## Quickstart

```python
from allsource_client import AllSourceClient

client = AllSourceClient(api_key="as_your_key", base_url="https://api.all-source.xyz")

# Ingest an event
event = client.ingest("user.signup", "user-123", {"plan": "pro", "source": "web"})
print(f"Created event {event.id}")

# Query events
result = client.query(event_type="user.signup", limit=10)
for e in result.events:
    print(f"{e.timestamp} {e.entity_id}: {e.payload}")
```

## Latest state without history

```python
from allsource_client import NotModified

# Folded state of one entity; history is omitted unless history=True.
state = client.entity_state("skill:abc")
print(state.current_state, state.version)

# Conditional read: NotModified when nothing changed since the last ETag.
again = client.entity_state("skill:abc", if_none_match=state.etag)
assert isinstance(again, NotModified)
```

### Views

A view is a declarative reducer. AllSource folds it from your events and
stores one row per key.

```python
client.define_view("ops_status", {
    "source": {"prefixes": ["ops.probe."]},
    "key": "{payload.probe}:{day:timestamp}",
    "ops": [
        {"on": "ops.probe.observed", "when": {"payload.ok": True}, "increment": {"ok": 1}},
        {"on": "ops.probe.observed", "max": {"last_seen_at": "timestamp"}},
    ],
})

row = client.latest("ops_status", "probe-a:2026-10-27")   # ViewLatest, ViewNotFound or NotModified
for row in client.latest_many("ops_status", prefix="probe-a:"):
    print(row.key, row.state)
```

`sync` keeps a local copy current: it reads every row once, then follows the
changes feed, long-polling when caught up.

```python
from allsource_client import SyncCheckpoint, SyncDelete, SyncReset, SyncUpsert

for item in client.sync("ops_status", checkpoint=stored_revision):
    if isinstance(item, SyncReset):
        rows.clear()
    elif isinstance(item, SyncUpsert):
        rows[item.key] = item.state
    elif isinstance(item, SyncDelete):
        rows.pop(item.key, None)
    elif isinstance(item, SyncCheckpoint):
        stored_revision = item.revision
```

### Event feed and read models

For folds a view cannot express, read the feed and commit rows yourself:

```python
from allsource_client import EpochChangedError, ReadModelCommit

page = client.feed(cursor=cursor, prefixes=["skill."], limit=100)
model = client.read_model("skills")
current = model.page(prefix="skill:")
model.commit(ReadModelCommit(
    expected_revision=current.revision,
    checkpoint=page.next_cursor,
    changes={"skill:abc": {"status": "published"}, "skill:old": None},
))
```

`EpochChangedError` means the feed generation changed: rebuild from position 0
of `current_version` and commit with `reset=True`.

## Async Usage

```python
import asyncio
from allsource_client import AllSourceAsyncClient

async def main():
    async with AllSourceAsyncClient(api_key="as_your_key") as client:
        event = await client.ingest("order.placed", "order-456", {"total": 99.99})
        events = await client.query(entity_id="order-456")
        print(f"Found {events.count} events")
        async for item in client.sync("ops_status", follow=False):
            print(item)

asyncio.run(main())
```

## API Reference

### `AllSourceClient(api_key=None, base_url="https://api.all-source.xyz", timeout=30.0, *, credential_provider=None)`

Pass exactly one of `api_key` (a static key, sent as `Authorization: Bearer <key>`)
and `credential_provider` (a callable returning the bearer, called on every
request attempt). `AllSourceAsyncClient` takes the same arguments; its
`credential_provider` may also return an awaitable.

| Method | Description |
|--------|-------------|
| `ingest(event_type, entity_id, data)` | Ingest a single event |
| `ingest_batch(events)` | Ingest multiple events |
| `query(event_type, entity_id, event_type_prefix, since, until, order, limit, offset)` | Query events |
| `query_events(..., as_of, payload_filter, if_none_match)` | Query events; `EventList.entity_version` and `etag`, or `NotModified` |
| `query_and_fold(folder, ...)` | Query events and fold them client-side |
| `get_events_by_entity(entity_id)` | Get events for an entity |
| `get_events_by_type(event_type)` | Get events by type |
| `entity_state(entity_id, as_of, history, if_none_match)` | Folded entity state, or `NotModified` |
| `feed(cursor, prefixes, entity_id, limit)` | One event-feed page |
| `read_model(name).page(prefix, after, limit, revision)` | One read-model page |
| `read_model(name).commit(commit)` | Commit read-model rows, returns the new revision |
| `define_view(name, definition)` / `get_view` / `delete_view` / `list_views` | Manage views |
| `latest(view, key, if_none_match)` | One view row, `ViewNotFound` or `NotModified` |
| `latest_many(view, prefix, limit)` | Iterate every row of a view |
| `view_changes(view, after, limit, wait)` | Rows changed after a revision |
| `sync(view, checkpoint, wait, prefix, page_limit, changes_limit, follow)` | Snapshot then follow a view; honours `Retry-After` |
| `list_prime_projections()` / `define_prime_projection(...)` | Prime projections |
| `project_node(node_id)` / `node_field_provenance(node_id, field)` | Prime nodes |
| `health()` | Check API health |

`AllSourceAsyncClient` has the same methods, all returning coroutines;
`latest_many` and `sync` are async iterators.

### Credentials

| Name | Description |
|------|-------------|
| `Es256AssertionProvider(private_key, client_id, tenant, kid=None, audience=..., *, clock=time.time)` | ES256 assertion signer; needs the `assertion` extra |
| `Es256AssertionProvider.from_pkcs8_pem(pem, client_id, tenant, kid=None, audience=...)` | Build from a PKCS#8 PEM P-256 key; refuses a bad key or empty field up front |
| `provider.bearer()` / `provider()` | Current assertion, re-minted within 15 s of expiry |
| `provider.kid` | The `kid` sent in the header |
| `jwk_thumbprint(public_key)` | RFC 7638 thumbprint of a P-256 public key |
| `AllSourceError.reason` | Gateway refusal reason, or `None` |
| `LegacyKeySunsetError` | `401 legacy_key_sunset`; subclass of `AllSourceError` |

## Development

```bash
pip install -e ".[dev]"
pytest
mypy src/
```
