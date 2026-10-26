"""PR #366 end-to-end driver: gateway signup -> dashboard key -> view -> QS fold -> SDK reads."""

import json
import os
import sys
import tempfile
import threading
import time
from datetime import datetime, timezone
from pathlib import Path

import httpx

from allsource_client import AllSourceClient
from allsource_client.types import (
    AllSourceError,
    NotModified,
    SyncCheckpoint,
    SyncDelete,
    SyncReset,
    SyncUpsert,
    ViewLatest,
    ViewNotFound,
)

SP = Path(os.environ.get("PR366_STATE", Path(tempfile.gettempdir()) / "pr366-proof"))
CP = "http://127.0.0.1:4901"
QS = "http://127.0.0.1:3902"
SECRETS = SP / "e2e-secrets.json"
LOG = SP / "evidence.jsonl"
VIEW = "ops_status"
DEFINITION = {
    "source": {"prefixes": ["ops.probe."]},
    "key": "{payload.probe}:{day:timestamp}",
    "ops": [
        {"on": "ops.probe.observed", "set": {"probe": "payload.probe", "day": "day:timestamp"}},
        {"on": "ops.probe.observed", "when": {"payload.ok": True}, "increment": {"ok": 1}},
        {"on": "ops.probe.observed", "when": {"payload.ok": False}, "increment": {"failed": 1}},
        {"on": "ops.probe.observed", "max": {"last_seen_at": "timestamp"}},
        {"on": "ops.probe.observed", "set": {"status": "payload.status"}},
    ],
}


def now():
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds")


def record(step, **fields):
    entry = {"at": now(), "step": step, **fields}
    with LOG.open("a") as f:
        f.write(json.dumps(entry, default=str) + "\n")
    print(json.dumps(entry, default=str), flush=True)


def check(step, ok, **fields):
    record(step, passed=bool(ok), **fields)
    if not ok:
        sys.exit(f"FAILED: {step}")


def signup(email, name):
    r = httpx.post(f"{CP}/api/v1/auth/register", json={"name": name, "email": email, "password": "pr366-Proof-pass-1"})
    check(f"signup {email}", r.status_code == 201, status=r.status_code, tenant=r.json().get("user", {}).get("tenant_id"))
    return r.json()["token"], r.json()["user"]["tenant_id"]


def mint_dashboard_key(session, scopes):
    r = httpx.post(f"{QS}/api/api-keys", headers={"Authorization": f"Bearer {session}"},
                   json={"name": "pr366 proof", "scopes": scopes})
    body = r.json()
    check("mint dashboard key", r.status_code == 201, status=r.status_code, scopes=body.get("scopes"),
          key_tenant=body.get("tenant_id"))
    return body["key"], body["tenant_id"]


def setup():
    session_a, tenant_a = signup("ops@pr366.test", "Probe Ops")
    key_a, key_tenant_a = mint_dashboard_key(session_a, ["events:read", "events:write", "projections:read", "projections:write"])
    check("dashboard key belongs to the signed-up tenant", key_tenant_a == tenant_a, tenant=tenant_a, key_tenant=key_tenant_a)
    key_a_ro, _ = mint_dashboard_key(session_a, ["events:read", "projections:read"])
    session_b, tenant_b = signup("other@pr366.test", "Other Tenant")
    key_b, _ = mint_dashboard_key(session_b, ["events:read", "events:write", "projections:read", "projections:write"])
    SECRETS.write_text(json.dumps({"a": key_a, "a_ro": key_a_ro, "b": key_b, "tenant_a": tenant_a, "tenant_b": tenant_b}))


def wait_ready(client, min_revision, deadline_s=120):
    start = time.monotonic()
    while time.monotonic() - start < deadline_s:
        info = client.get_view(VIEW)
        if info.ready and info.revision >= min_revision:
            return info, time.monotonic() - start
        time.sleep(1)
    info = client.get_view(VIEW)
    return info, time.monotonic() - start


def e2e():
    s = json.loads(SECRETS.read_text())
    a = AllSourceClient(api_key=s["a"], base_url=CP)
    raw = httpx.Client(base_url=CP, headers={"Authorization": f"Bearer {s['a']}"})
    day = datetime.now(timezone.utc).strftime("%Y-%m-%d")

    info = a.define_view(VIEW, DEFINITION)
    check("define view through gateway", info.view == VIEW, generation=info.generation, ready=info.ready)

    for probe, ok, status in [("probe-a", True, "up"), ("probe-a", True, "up"), ("probe-a", False, "degraded"), ("probe-b", True, "up")]:
        a.ingest("ops.probe.observed", probe, {"probe": probe, "ok": ok, "status": status})
    a.ingest("billing.invoice.paid", "inv-1", {"probe": "probe-a", "ok": True})
    record("ingested", events=5, note="4 ops.probe.observed + 1 billing event outside the view's prefix")

    info, waited = wait_ready(a, 1)
    check("Query Service folded the view", info.ready and info.revision >= 1, revision=info.revision,
          checkpoint=info.checkpoint, seconds=round(waited, 1))

    row = a.latest(VIEW, f"probe-a:{day}")
    check("latest probe-a", isinstance(row, ViewLatest) and row.state.get("ok") == 2 and row.state.get("failed") == 1
          and row.state.get("status") == "degraded", state=getattr(row, "state", None), etag=getattr(row, "etag", None))

    again = a.latest(VIEW, f"probe-a:{day}", if_none_match=row.etag)
    check("latest with If-None-Match is NotModified", isinstance(again, NotModified), etag=getattr(again, "etag", None))
    r304 = raw.get(f"/api/v1/views/{VIEW}/entities/probe-a:{day}", headers={"If-None-Match": row.etag})
    check("gateway returns 304 with no body", r304.status_code == 304 and r304.content == b"",
          status=r304.status_code, etag=r304.headers.get("etag"), body_bytes=len(r304.content))

    rows = list(a.latest_many(VIEW, prefix="probe-"))
    check("latest_many returns both probes", sorted(r.key for r in rows) == sorted([f"probe-a:{day}", f"probe-b:{day}"]),
          keys=[r.key for r in rows])

    missing = a.latest(VIEW, f"probe-z:{day}")
    check("missing row is ViewNotFound", isinstance(missing, ViewNotFound), ready=getattr(missing, "ready", None))

    snapshot = list(a.sync(VIEW, follow=False))
    upserts = [i for i in snapshot if isinstance(i, SyncUpsert)]
    cps = [i for i in snapshot if isinstance(i, SyncCheckpoint)]
    check("sync snapshot yields rows and a checkpoint", len(upserts) == 2 and cps, upserts=len(upserts),
          checkpoint=cps[-1] if cps else None)
    saved = cps[-1].revision

    seen = []

    def follow():
        follower = AllSourceClient(api_key=s["a"], base_url=CP)
        for item in follower.sync(VIEW, checkpoint=saved, wait=15):
            seen.append((time.monotonic(), item))
            if isinstance(item, SyncUpsert) and item.key == f"probe-c:{day}":
                return

    t = threading.Thread(target=follow, daemon=True)
    t.start()
    time.sleep(2)
    sent = time.monotonic()
    a.ingest("ops.probe.observed", "probe-c", {"probe": "probe-c", "ok": True, "status": "up"})
    t.join(timeout=60)
    hit = [(at, i) for at, i in seen if isinstance(i, SyncUpsert) and i.key == f"probe-c:{day}"]
    check("sync long-poll delivers a new row", bool(hit), seconds_after_ingest=round(hit[0][0] - sent, 2) if hit else None,
          state=hit[0][1].state if hit else None, items=[type(i).__name__ for _, i in seen])
    (SP / "e2e-checkpoint.json").write_text(json.dumps({"after": saved, "revision": a.get_view(VIEW).revision}))

    es = a.entity_state("probe-a")
    check("entity_state has version and no history", not isinstance(es, NotModified) and es.version == 3 and es.history is None,
          version=getattr(es, "version", None), etag=getattr(es, "etag", None))
    es2 = a.entity_state("probe-a", if_none_match=es.etag)
    check("entity_state If-None-Match is NotModified", isinstance(es2, NotModified))

    reserved = raw.put(f"/api/v1/read-models/view_{VIEW}", json={"expected_revision": 0, "checkpoint": {"epoch": 0, "position": 0},
                                                                  "ready": True, "reset": False, "changes": {"x": {"forged": True}}})
    check("tenant cannot write a view's rows directly", reserved.status_code == 403, status=reserved.status_code,
          body=reserved.text[:200])

    ro = httpx.put(f"{CP}/api/v1/views/ro_probe", headers={"Authorization": f"Bearer {s['a_ro']}"}, json=DEFINITION)
    mode = os.environ.get("ALLSOURCE_KEY_AUTHORITY_MODE", "shadow")
    if mode == "enforce":
        check("read-only key cannot define a view (enforce)", ro.status_code == 403, status=ro.status_code,
              body=ro.text[:200])
    else:
        record("read-only key defines a view (scope counted, not enforced)", mode=mode, status=ro.status_code,
               body=ro.text[:200])



def isolation():
    s = json.loads(SECRETS.read_text())
    day = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    b = AllSourceClient(api_key=s["b"], base_url=CP)
    try:
        b.latest(VIEW, f"probe-a:{day}")
        check("tenant B without the view gets view_not_found", False)
    except AllSourceError as err:
        check("tenant B without the view gets view_not_found", err.code == "view_not_found", status=err.status_code)
    b.define_view(VIEW, DEFINITION)
    b.ingest("ops.probe.observed", "probe-q", {"probe": "probe-q", "ok": True, "status": "up"})
    info, waited = wait_ready(b, 1)
    check("tenant B's same-named view folds only B's events", sorted(r.key for r in b.latest_many(VIEW)) == [f"probe-q:{day}"],
          keys=[r.key for r in b.latest_many(VIEW)], seconds=round(waited, 1))
    other = b.latest(VIEW, f"probe-a:{day}")
    check("tenant B cannot read tenant A's row", isinstance(other, ViewNotFound), result=type(other).__name__)
    forged = httpx.get(f"{CP}/api/v1/views/{VIEW}/entities/probe-a:{day}",
                       params={"tenant_id": s["tenant_a"]}, headers={"Authorization": f"Bearer {s['b']}"})
    check("client tenant_id is stripped at the gateway", forged.status_code == 404, status=forged.status_code,
          body=forged.text[:200])


def after_restart():
    s = json.loads(SECRETS.read_text())
    a = AllSourceClient(api_key=s["a"], base_url=CP)
    saved = json.loads((SP / "e2e-checkpoint.json").read_text())
    info = a.get_view(VIEW)
    check("view survives Core restart", info.revision >= saved["revision"], revision=info.revision, ready=info.ready)
    changes = a.view_changes(VIEW, after=saved["after"])
    day = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    check("changes cursor from before the restart still answers", any(c.key == f"probe-c:{day}" for c in changes.changes),
          keys=[c.key for c in changes.changes], revision=changes.revision)
    a.ingest("ops.probe.observed", "probe-a", {"probe": "probe-a", "ok": True, "status": "up"})
    info, waited = wait_ready(a, info.revision + 1)
    row = a.latest(VIEW, f"probe-a:{day}")
    check("fold resumes after restart without double counting", isinstance(row, ViewLatest) and row.state.get("ok") == 3
          and row.state.get("failed") == 1, state=getattr(row, "state", None), seconds=round(waited, 1))


def canaries():
    session, _ = signup("canary-owner@pr366.test", "Canary Owner")
    auth = {"Authorization": f"Bearer {session}"}
    onboard = httpx.post(f"{CP}/api/v1/onboard/start", json={"email": "canary@pr366.test", "name": "Canary"})
    check("onboarding key", onboard.status_code == 201, status=onboard.status_code)
    plain = httpx.post(f"{QS}/api/api-keys", headers=auth, json={"name": "canary plain", "scopes": ["events:read"]})
    expiring = httpx.post(f"{QS}/api/api-keys", headers=auth, json={"name": "canary expiring", "scopes": ["events:read"],
                                                                   "expires_at": "2027-10-27T00:00:00Z"})
    revoked = httpx.post(f"{QS}/api/api-keys", headers=auth, json={"name": "canary revoked", "scopes": ["events:read"]})
    check("canary keys minted", all(r.status_code == 201 for r in (plain, expiring, revoked)),
          statuses=[r.status_code for r in (plain, expiring, revoked)])
    gone = httpx.delete(f"{QS}/api/api-keys/{revoked.json()['id']}", headers=auth)
    check("revoked one canary", gone.status_code == 200, status=gone.status_code)
    good = [{"kind": "onboarding_key", "token": onboard.json()["api_key"]},
            {"kind": "managed_no_expiry", "token": plain.json()["key"]},
            {"kind": "managed_with_expiry", "token": expiring.json()["key"]}]
    (SP / "canaries-good.json").write_text(json.dumps(good))
    (SP / "canaries-bad.json").write_text(json.dumps(good + [{"kind": "revoked_managed_key", "token": revoked.json()["key"]}]))


if __name__ == "__main__":
    {"setup": setup, "e2e": e2e, "isolation": isolation, "after_restart": after_restart, "canaries": canaries}[sys.argv[1]]()
