# How Might We: An AI-Empowered Inbox on AllSource

*Status: NOT PLANNED as of 2026-09-16 — closed won't-do (#291). The P0 wedge is code-complete on main and keeps working: the `email.*` event contract, the Nylas and Resend connectors behind the `EmailProvider` trait, the CP webhook, the Prime ingester, and the `inbox_recall_thread` / `inbox_draft` verbs. What stopped is P1–P5 and the GA path — the live-mailbox dogfood, triage/send, multi-tenant grants, and the Google CASA / Microsoft verification reviews. Reopen against a real user of the inbox, not as a platform showcase.*

*Original status: DECISION + DESIGN proposal. Settled inputs locked (see §2 Framing). Not yet scheduled.*
*Date: 2026-06-14*
*Author: all.source team*

---

## 1. TL;DR

- **Verdict: GO (CONDITIONAL on x402 enablement + per-tenant OAuth-app verification).** Build the inbox as a satellite of the Prime workspace product, not a standalone mail client.
- **Chosen email API: Nylas Email API v3.** It is the only evaluated provider that genuinely backs a *real* two-way mailbox (read the user's existing Gmail/Microsoft/IMAP inbox **and** send as them) through one unified, webhook-rich, OAuth-per-mailbox API with US/EU residency.
- **One-sentence architecture:** A provider-agnostic Nylas webhook terminates at the Go Control Plane (auth, tenant resolve, allowance check), which fans each message into Core as durable `email.*` events; a stateless Prime ingester folds those events into `interaction` nodes (linked to person/org/thread), made recallable by hybrid vector+graph score; Claude Code drives triage/draft/send/recall over MCP verbs.
- **The only substantial new code** is a connector app + ingestion glue + four MCP verbs + a thin graph view. Everything else (event store, graph/recall, auth/billing) is **reuse**.
- Founder-dogfood slice ships first as the wedge (single tenant, one mailbox); the same code path generalizes to multi-tenant GA because it rides existing per-tenant primitives.

---

## 2. Problem & Framing

### What this is

"An AI inbox on AllSource, driven from Claude Code, visible in the Prime graph" means:

1. **Email is durable Core data.** Every received/sent/triaged/replied/archived message is a Core event (WAL+Parquet+DashMap). Core *is* the database; we do not stand up a side mail store.
2. **Email is visible as graph.** Each message becomes a Prime `interaction` node, auto-linked to the `person`/`org` it involves and the `thread` it belongs to — so the inbox inherits recall, neighbors, and the contact timeline for free.
3. **The control point is the terminal.** Claude Code triages, drafts, sends, and recalls thread context through MCP tools. There is **no bespoke mail-client UI** — the visible surface is the Prime workspace graph plus a thin "interactions" view over it.

### Why now

The Prime Workspace-CRM plan (`docs/plans/2026-06-10-prime-workspace-crm-design.md`) already reserves the exact attachment point: the **`interaction`** node ("Folk activity — channel email/meeting/call, ts, summary", plan line 64) wired to people/orgs via **`involves`** and **`mentions`** edges (lines 69-72), feeding the **contact-timeline projection** (line 82) and the recall box via **interaction-summary embeddings** (line 77). The plan's **P2** row scopes it explicitly: *"Email/calendar sync — Gmail/GCal ingest → `interaction` nodes (NEW; reuse the MCP Gmail/GCal integration pattern)"* (lines 114-115). The inbox is therefore not a new product — it is the email half of an already-validated workspace wedge.

### Relationship to the workspace plan

The workspace plan locks **"Prime is the system of record and the brain"** (Decision 1, lines 23-24) and the satellite model — *"Notion connects as an optional two-way sync satellite; Folk imports once"* (lines 14-15). The inbox follows the same orbit law: **email is an inbound-only event source into the graph.** We never make a second mail database; emails write through Core as events for provenance/time-travel, identical to Folk's "import once" framing. This doc is the design for plan-row **P2's email leg**, made productizable and billing-aware. P0 (the `interaction` node + `involves`/`mentions` edges) is its hard prerequisite — and per RECON, that prerequisite needs **zero engine changes** (see §3).

---

## 3. Build vs Integrate

### 3.1 Email-API decision matrix

Scores are 1–5 (5 = best fit for *a multi-tenant AI inbox that reads a real mailbox AND sends as the user*). Internal-reuse verdicts (§3.3) are verified against the codebase; the external email-API axes rest on the RESEARCH bundle and the per-provider doc URLs footnoted below. Each contested axis carries a source footnote so the score is auditable rather than asserted.

| Axis | **Nylas v3** | Resend | Postmark | Bidirectional-sync provider (TBD) | Raw IMAP/SMTP on Core |
|---|---|---|---|---|---|
| Time-to-first-email | 4 (hosted OAuth, managed refresh) | 5 (API key only) | 5 (API key only) | 4 | 1 (build sync/MIME/refresh) |
| **Read + send (real mailbox)** | **5** (true bidirectional) [^nylas-msg] | 1 (send + inbound-parse only) [^resend] | 2 (MX-delegated inbound parse + Messages API retrieval; still no OAuth into the user's existing mailbox) [^postmark] | 5 (research: `can_back_real_inbox=true`) | 4 (protocol-level, you build it) |
| Multi-provider reach | 5 (Gmail/MS/Yahoo/iCloud/IMAP, one model) [^nylas-msg] | 0 (N/A) | 0 (N/A) | 5 (research class = `bidirectional_sync`; concrete provider coverage not in research) | 4 (universal, unnormalized) |
| Multi-tenant / billing fit | 5 (grant=tenant boundary, per-account price) | 2 (account-level, route by `to` yourself) | 3 (server-per-tenant, no per-tenant invoice) | 4 (per-account; pricing not in research) | 2 (all DIY) |
| Data ownership / privacy | 4 (SOC2/ISO/HIPAA, US+EU isolated; but a content sub-processor) | 2 (US-only, stores inbound) | 2 (US-only) | 3 (compliance posture not in research) | 5 (you hold everything) |
| Deliverability | 5 (sends as the user, inherits their reputation) | 5 (curated pool, warmup) | 5 (best transactional placement) | 4 | 2 (you tune SPF/DKIM/IP) |
| Lock-in | 3 (provider in mail path) | 3 | 3 | 3 | 1 (none) |
| Ongoing cost | 3 (~$2/acct/mo, linear; ~$2k/mo @1k mailboxes — list-price estimate, confirm before treating as a budget number) | 4 (volume) | 4 (volume) | 4 (marginal per-account rate not in research) | 5 (free APIs, you pay infra) |

[^nylas-msg]: Nylas v3 Messages API (read + send across Gmail/Microsoft/Yahoo/iCloud/IMAP behind one model): https://developer.nylas.com/docs/v3/email/ — confirm the exact read/send/scope behaviour against the current page before build.
[^resend]: Resend supports transactional send and inbound parsing only; no OAuth into a user's existing mailbox: https://resend.com/docs.
[^postmark]: Postmark inbound is MX-delegated inbound parse with Messages-API retrieval of received mail, but no OAuth/IMAP into the user's *existing* mailbox: https://postmarkapp.com/developer/user-guide/inbound — hence `can_back_real_inbox=false` despite the higher read score vs. Resend's CLI-poll model.

**Recommendation: Nylas Email API v3 (RESEARCH score 9/10).** It is the only off-the-shelf option that satisfies the *literal* ask — read a tenant's existing inbox across providers **and** reply as them — behind one unified API, with a signed webhook stream, hosted OAuth + managed token refresh, message+thread granularity, US/EU data residency, and per-connected-account pricing that maps 1:1 onto our per-tenant billing. (For the webhook event names this design assumes, see §4.6 — treat them as per-provider-docs, not as settled fact.) Its v3 **Agent Accounts** (Nylas-hosted `name@company.com` mailboxes surfaced as ordinary grants) are pitched for "one agent identity per customer" — exactly the AI-inbox shape, and the natural home for the founder-dogfood mailbox. **Caveat: Nylas Agent Accounts are still Beta** (https://developer.nylas.com/docs/v3/getting-started/agent-accounts); if they are not GA-ready when P0 lands, P0 uses the founder's real grant instead (see open question 5 and the P0 row / Risks).

**Runners-up, ranked:**
1. **Bidirectional-sync provider (TBD, RESEARCH score 8/10)** — the closest substitute class: another `bidirectional_sync` provider the research flags as `can_back_real_inbox=true`. The research entry carries no vendor name, price, provider-coverage, or compliance detail, so those specifics are **TBD — not in research**. Keep this class as the **cost-driven swap target** *if* a candidate's marginal per-account rate at fleet scale proves materially cheaper than Nylas — confirm a named vendor and its real marginal rate before treating that as the swap trigger; the swap is not yet justified by a hard number.
2. **Direct Gmail API + Microsoft Graph (DIY, 7/10)** — fully bidirectional, free API, max residency control, but you own two auth flows, custom MIME, token refresh, and push/pull sync. The fallback if we ever refuse a third party in the mail path.
3. **Postmark / Resend (3/10 as the connector)** — best-in-class *transactional send + inbound-parse*, structurally unable to read a user's existing mailbox (no OAuth/IMAP into existing mail). Optional **outbound/bulk leg** of a hybrid stack later; never the inbox layer.

> **Provider-agnostic by construction.** The connector exposes an internal `EmailProvider` trait (fetch_message, list_thread, send, register_webhook, verify_signature). Nylas is the first implementation; the TBD bidirectional-sync provider and a DIY Gmail/Graph path are drop-ins. Nothing downstream of the Control Plane webhook knows the provider name — Core events and Prime nodes are provider-neutral. This keeps the verdict defensible *and* the lock-in axis honest.

### 3.2 Why NOT raw IMAP/SMTP on Core (the rejected alternative)

Raw IMAP/SMTP-on-Core is explicitly rejected. IMAP/SMTP are stateful, per-provider-quirky, push-poor protocols (no managed webhooks, hand-rolled IDLE, per-provider auth, custom MIME, token refresh, backfill cursors). Putting that machinery *inside* Core would violate the architecture law twice over: it bloats the event store with connector concerns it must never own, and it has Core terminate external credentials and traffic — but **Core never authenticates** (all public auth terminates at the Control Plane). The connector is an isolated app that talks to a managed provider over the network and forwards *parsed* events inward **through the Control Plane** (never directly to Core); Core stays a clean event store. We buy the sync problem (Nylas) rather than build it on the wrong layer.

**Connector isolation spec (explicit).** The connector is a **standalone app** whose only inward reach is the Control Plane HTTP surface — it has **no `path` dependency on `apps/core`** (or any other app's crate) and its Dockerfile contains **no `COPY apps/<other-app>/`**. If it needs Core/event types, it consumes the public SDK / HTTP API, not a crate path-dep. (This is deliberately *unlike* `apps/prime-mcp`, which path-depends on `apps/core` and COPYs `apps/core/` into its build — that coupling is the anti-pattern the connector must not repeat.) The connector either lives as its own app or inside the Control Plane; it never imports another app's source.

### 3.3 Per-layer build-vs-reuse verdict

| Layer | Verdict | Why (grounded in RECON) |
|---|---|---|
| **Connector (email ingress/egress)** | **INTEGRATE (Nylas) + thin new app** | No inbound email connector exists anywhere today (RECON: CP only has *outbound* SMTP; QS "webhooks" are *outbound* deliveries). Net-new, but isolated and provider-pluggable. |
| **Event store** | **REUSE Core, as-is** | `email.received/sent/triaged/replied/archived` are all valid `EventType` strings (lowercase dot-namespaced) and map onto the fixed envelope with no schema change. |
| **Graph / recall** | **REUSE Prime, zero engine change** | `node_type` is a free-form string; schemas are optional (no registered schema = any shape passes); templates are descriptive, not enforced. `interaction` "just works" by calling `prime_add_node {type:"interaction", ...}`. |
| **Auth / tenanting / billing / ingress** | **REUSE Control Plane (+ one NEW security item)** | The `/api/v1/webhooks/` prefix is already auth-exempt; HMAC-verify like the LemonSqueezy webhook; resolve tenant from payload; meter usage exactly like the mature x402 allowance subsystem; store the per-tenant grant in Core config like the CDP wallet — **but** CP-side envelope encryption of that grant before `SetConfig` is a **NEW** build item (Core config is plaintext KV), gated at P3. See §7. |
| **AI** | **REUSE Claude (via MCP) + Prime recall** | Triage/draft are Claude calling MCP verbs; recall is `prime_recall`/`prime_context`. No new model, no new inference service. |
| **UI** | **REUSE Prime graph view + minimal** | Visible surface is the workspace graph: contact-timeline projection + neighbors + recall box already exist. New = a thin "interactions" filter/render, not a mail client. |

**The only substantial new code is: (1) the connector app, (2) the ingestion glue (webhook → Core events → Prime nodes/edges/embeds), (3) four MCP verbs (triage/draft/send/recall-thread-context), and (4) a thin graph view.** Everything load-bearing — durability, multi-tenancy, auth, billing, recall — already exists and is reused.

---

## 4. Architecture

### 4.1 Inbound data flow

```
                    ┌─────────────────────────────────────────────────────────────┐
                    │  Nylas v3 (grant per tenant mailbox; manages provider OAuth) │
                    └───────────────────────┬─────────────────────────────────────┘
                                            │ POST signed webhook (message created /
                                            │ updated; event names per Nylas docs), HMAC-signed
                                            ▼
        ┌──────────────────────────────────────────────────────────────────────────┐
        │  CONTROL PLANE (Go/Gin) — single public ingress; Core never authenticates │
        │  POST /api/v1/webhooks/email                                              │
        │   1. verify HMAC (EMAIL_WEBHOOK_SECRET)        ← LemonSqueezy pattern     │
        │   2. resolve tenant from grant_id → tenant_id  ← Core config lookup       │
        │   3. allowance check (events quota + email/x402 meter, fail-closed)       │
        │   4. mint 60s tenant-scoped delegation JWT     ← delegation.go pattern    │
        └───────────────────────┬──────────────────────────────────────────────────┘
                                │ forwardRequest → POST /api/v1/events (tenant injected)
                                ▼
        ┌──────────────────────────────────────────────────────────────────────────┐
        │  CORE (Rust event store) — WAL-first, durable, immutable                  │
        │  email.received  { entity_id = message_id, tenant_id, payload, metadata } │
        │  (content + envelope both durable here — never Postgres, never a side DB) │
        └───────────────────────┬──────────────────────────────────────────────────┘
                                │ Prime ingester reads Core events (single Prime writer)
                                ▼
        ┌──────────────────────────────────────────────────────────────────────────┐
        │  PRIME (graph/recall engine — STATELESS over Core)                        │
        │  prime_add_node {type:"interaction", properties:{conversation_id, ...}}   │
        │  prime_add_edge interaction --from--> person                              │
        │  prime_add_edge interaction --part_of--> thread                           │
        │  prime_embed {id, text: subject + snippet}   (384-dim fastembed)          │
        └───────────────────────┬──────────────────────────────────────────────────┘
                                │ recall + neighbors + contact-timeline projection
                                ▼
        ┌──────────────────────────────────────────────────────────────────────────┐
        │  VISIBLE SURFACE: Prime workspace graph (interactions on contact timeline)│
        │  + CLAUDE CODE via MCP (triage / recall-thread-context)                   │
        └──────────────────────────────────────────────────────────────────────────┘
```

### 4.2 Outbound data flow

```
  Claude Code  ──inbox_draft(thread_id, intent)──►  draft event (email.drafted) in Core
       │                                                   │
       │  human/agent reviews draft (in terminal)          │
       ▼                                                   ▼
  inbox_send(draft_id) ──► Control Plane ──► (allowance check: send meter) ──►
       Nylas send (through the user's own mailbox; lands in their Sent folder)
                                   │
                                   ▼
                    email.sent event in Core (entity_id = message_id)
                                   │
                                   ▼
                    Prime: interaction node (direction:"outbound") + edges + embed
```

Send rides the **same** delegation + metering path as ingest; the only new surface is the provider `send` call and an `email.sent` event. Nylas sends *through the user's own provider*, so deliverability is the user's mailbox reputation (no ESP warmup) — ideal for conversational replies, explicitly **not** a bulk-blast engine.

### 4.3 Core event shapes

The Core envelope is fixed (RECON): server-assigned `id` (UUID) + `timestamp` + per-entity monotonic `version`; caller supplies `event_type`, `entity_id`, `tenant_id` (injected by the gateway, never client-trusted), `payload`, optional `metadata`. `entity_id` = the canonical **message id** so all lifecycle events for one message replay in `(timestamp, version)` order.

```jsonc
// email.received  (POST /api/v1/events, via the Control Plane delegation JWT)
{
  "event_type": "email.received",
  "entity_id":  "<provider_message_id>",       // stable key; lifecycle events share it
  "tenant_id":  "<resolved by Control Plane>",  // never client-supplied
  "payload": {
    "thread_id":   "<provider_thread_id>",      // == conversation_id on the Prime node
    "subject":     "Re: Q3 renewal",
    "from":        { "name": "Dana Lee", "email": "dana@acme.com" },
    "to":          [{ "email": "founder@all-source.xyz" }],
    "cc":          [],
    "snippet":     "Following up on the renewal terms…",
    "body":        "<full message body — inline in this payload>",  // see note below
    "received_at": "2026-06-14T09:12:00Z",
    "folder":      "inbox",
    "labels":      ["important"]
  },
  "metadata": {
    "provider":        "nylas",
    "grant_id":        "<nylas grant>",
    "idempotency_key": "<provider delivery/message id>"  // dedupe key (see §4.6)
  }
}
```

> **Body content is inline, not a blob reference.** Core today has **no blob/attachment subsystem** — there is no content-addressed store to reference. The full message body therefore lives **inline in the `email.received` payload**, which is exactly what makes it durable: the event (body included) is written to the WAL (CRC32) and flushed to Parquet (Snappy) like any other Core event. This is what satisfies the LAW invariant "email content is durable Core events" — the content *is* the event. The connector fetches the body from the provider (it is not in the thin webhook notification) and inlines it before posting the event. Two follow-on items, neither blocking content durability: (1) large bodies / attachments — if a future tier needs to store multi-MB attachments efficiently, design a **Core content/blob subsystem** then (a genuinely NEW work item, not present today); for P0–P3, inline bodies are bounded by ordinary email size and fine. (2) the "what NOT to send to an LLM" policy (§7) still applies — the body is durable in Core but is **not** put into the embedding text or shown to the agent by default.

```jsonc
// email.sent  (after an outbound send succeeds)
{
  "event_type": "email.sent",
  "entity_id":  "<provider_message_id of the sent message>",
  "tenant_id":  "<tenant>",
  "payload": {
    "thread_id":   "<provider_thread_id>",
    "in_reply_to": "<message_id being replied to>",
    "subject":     "Re: Q3 renewal",
    "to":          [{ "email": "dana@acme.com" }],
    "snippet":     "Happy to extend the renewal…",
    "sent_at":     "2026-06-14T09:40:00Z",
    "direction":   "outbound"
  },
  "metadata": { "provider": "nylas", "grant_id": "<grant>", "draft_id": "<draft uuid>" }
}
```

`email.triaged` / `email.replied` / `email.archived` share the same `entity_id` (the message id) and carry a small `payload` (e.g. `{ "label": "needs-reply", "by": "claude" }`). Latest state for a triage view is a single ordered read: `GET /api/v1/events/query?entity_id=<message_id>&order=desc&limit=1`.

### 4.4 Prime node / edge schema

`node_type` is a free-form string and schemas are optional, so `interaction` needs no engine change. We model the email as an `interaction` node, the conversation as a `thread` node, and reuse the existing `contact` template for the people involved.

```jsonc
// interaction node  (prime_add_node)
{
  "type": "interaction",
  "properties": {
    "channel":         "email",
    "direction":       "inbound",              // or "outbound"
    "subject":         "Re: Q3 renewal",
    "snippet":         "Following up on the renewal terms…",
    "message_id":      "<provider_message_id>",
    "conversation_id": "<provider_thread_id>", // ← enables prime_context L1 thread scoping
    "from":            "dana@acme.com",
    "to":              ["founder@all-source.xyz"],
    "received_at":     "2026-06-14T09:12:00Z",
    "tenant_id":       "<tenant>",             // stamp explicitly (single-store caveat, §4.7)
    "domain":          "inbox"
  }
}
```

```text
Edges (prime_add_edge, endpoints are entity_ids node:<type>:<id>):

  interaction --from-->     person (sender)            // reuse plan's `involves`
  interaction --to-->       person (each recipient)
  interaction --about-->    org    (sender's company)  // reuse contact template works_at->organization
  interaction --part_of-->  thread (conversation node)
  interaction --mentions--> person|org|deal            // plan's auto-extracted `mentions`

  thread node: { type:"thread", properties:{ conversation_id, subject } }
    → prime_neighbors(thread, relation:"part_of", direction:"incoming")
      lists every email in the thread.
```

**Recall paths (all already implemented):**
- **Semantic:** `prime_embed {id:<interaction>, text:<subject + snippet>}` then `prime_recall {text:"emails about the renewal"}` → hybrid score = `0.5·cosine + 0.3·proximity + 0.2·recency` (weights normalize to 1.0; vector hits seed at depth 0, graph BFS expands neighbors, MMR re-rank for domain diversity).
- **Thread-scoped:** `prime_context {tier:"L1", conversation_id:<thread_id>}` filters nodes on `properties.conversation_id` and expands 1-hop — the whole thread's interactions + neighbors.
- **Person-scoped:** `prime_neighbors(<person>, relation:"from", direction:"incoming")` → every email from that person; the contact-timeline projection orders interactions by ts for free.

> **RECON gap honored:** `prime_recall` returns edges only as a *count* (the edge list is always empty in the facade). So thread/person/org edges are surfaced via `prime_neighbors` / `prime_context` (which *do* return edges), not via `prime_recall`. The MCP verb design reflects this split.

### 4.5 Claude Code MCP tools

Four thin connector verbs, plus the existing 19 `prime_*` tools for recall. The connector verbs forward through the Control Plane (auth/tenant/allowance); they never hold Core auth themselves.

```jsonc
// inbox_triage — classify + label an incoming message (writes email.triaged)
{ "name": "inbox_triage",
  "input": { "message_id": "string", "label": "needs-reply|fyi|spam|archive",
             "reason": "string?", "by": "claude|human" } }

// inbox_draft — compose a reply draft grounded in recalled thread context (writes email.drafted)
{ "name": "inbox_draft",
  "input": { "thread_id": "string", "intent": "string",
             "tone": "string?", "include_context": "boolean" } }   // pulls prime_context L1

// inbox_send — send a reviewed draft as the user (writes email.sent)
{ "name": "inbox_send",
  "input": { "draft_id": "string", "confirm": "boolean" } }        // confirm gate, never auto-send

// inbox_recall_thread — return the thread's interactions + neighbors for grounding
{ "name": "inbox_recall_thread",
  "input": { "thread_id": "string", "top_k": "number?" } }         // wraps prime_context + prime_neighbors
```

Recall/search reuse `prime_recall`, `prime_context`, `prime_neighbors`, `prime_search`, `prime_history` directly — no new verbs. `inbox_recall_thread` is sugar that composes `prime_context {tier:L1}` with `prime_neighbors(thread, part_of, incoming)` so an agent gets the full conversation in one call.

### 4.6 Dedupe (provider redelivery)

Nylas exposes a webhook stream (the exact event names — e.g. a "message created" vs. "message updated/flag-changed" trigger — are per the current Nylas v3 webhook docs, not asserted here as fact). **Assume at-least-once delivery** and that the same message can be redelivered, and design dedupe accordingly. Core ships an `ExactlyOnceRegistry` (`check_idempotency` + `extract_idempotency_key` on `metadata.idempotency_key`, 24h TTL, LRU-bounded) but **it is NOT wired into the live `POST /api/v1/events` path today** (verified: `check_idempotency` is invoked only inside `exactly_once.rs`'s own tests; the live API only reads `.stats()`). Two options:

- **(a) Determinism via `entity_id` + `expected_version` — first-ingest dedupe only:** set `entity_id = message_id` and use `expected_version` optimistic concurrency, so a *replayed first `email.received`* hits `VersionConflict` and is dropped idempotently. *Works today, no Core change.* **Chosen for P0.** **Scope caveat:** this only protects the *first* event for an entity. Subsequent same-entity lifecycle events (`email.triaged`/`replied`/`archived`) are NOT made idempotent by `expected_version` alone — a redelivered triage could append a duplicate. Lifecycle-event idempotency therefore relies on either (i) the connector keying on the provider message-id + lifecycle-action and not re-emitting, or (ii) wiring Core's `ExactlyOnceRegistry` into ingest (option b). Until (b) lands, the connector-side guard below is the lifecycle dedupe.
- **(b) Wire the idempotency registry into ingest** (atomic with the version lock). The general dedupe answer (covers lifecycle redelivery, not just first-ingest). Cleaner long-term; a small, well-bounded Core change. **Deferred to P4 hardening.**

Connector-side, dedupe the create/update pair (and lifecycle redeliveries) by keying on the provider message id + action before emitting, so a flag-change update does not produce a second `email.received` and a redelivered lifecycle event does not produce a duplicate `email.triaged`.

### 4.7 Single-writer / multi-tenant caveats (from RECON, honored)

- **Prime is single-writer per data-dir** (`prime.lock`; a second instance is a read-only replica). The email ingester must write through **one** Prime writer (or via `--mode http` / `--sync-to`), never open the data-dir concurrently.
- **Prime events ingest with `tenant_id:None`** in single-store. Multi-tenant email separation requires **stamping `properties.tenant_id` on every interaction node** (done explicitly above) and filtering `full_graph()` by it.
- **CP allowance delta + keyedMutex are exact only on one machine** (single Fly CP today). An email meter inherits the same scale-out bound as x402; `CONTROL_PLANE_MULTI_INSTANCE` is the documented guard.

---

## 5. AI Capabilities

| Capability | Mode | Mechanism |
|---|---|---|
| **Triage** ("needs-reply / fyi / spam") | **Agent-driven** | Claude calls `inbox_triage`; reasoning grounded by `inbox_recall_thread` (prior context for this sender/thread). Writes `email.triaged`. |
| **Draft reply** | **Agent-driven** | Claude calls `inbox_draft`; pulls `prime_context {L1, conversation_id}` + sender's `prime_neighbors` to ground the reply in real history. Human/agent reviews in terminal. Writes `email.drafted`. |
| **Send** | **Agent-driven, confirm-gated** | `inbox_send {confirm:true}`. Never auto-sends. Writes `email.sent`. |
| **Recall thread context** | **Agent-driven** | `inbox_recall_thread` → `prime_context` + `prime_neighbors`. The grounding primitive for the three above. |
| **Embedding new interactions** | **Background (server projection)** | Ingester calls `prime_embed {text: subject+snippet}` on each new interaction; no agent in the loop. Makes mail recallable by meaning. |
| **Entity extraction / linking** (sender→person, body→`mentions`) | **Background, with agent assist** | Deterministic email→person resolution (match `person.emails`) at ingest; `mentions` edges are the plan's "auto-extracted" path (LLM pass deferrable). |
| **Contact-state collapse** (repeated mail → current contact) | **Background projection** | `prime_define_projection` on `contact` (e.g. `last_contact_at:last_write`, `tags:merge_array`) + `prime_project_node`. |
| **Smart suggestions** ("haven't replied to X in 5d") | **Background → surfaced** | Derived from graph + temporal recency; surfaced in the workspace, actioned by the agent. P5-style. |

**Everything agent-facing is grounded in recall** — the agent never reasons over an email in isolation; `inbox_recall_thread` supplies the conversation + neighbor context first. Background work (embed, extract, project) is pure server projection over Core events, no agent.

---

## 6. Product & Pricing

The inbox is a **billing-aware satellite of the Prime workspace product**, riding the existing 5-tier ladder (Self-Host / Indie / Studio / Scale / Enterprise) and x402 allowances. The entitlement numbers are single-sourced from `TierQuotaMap` (`apps/control-plane/internal/domain/entities/subscription.go`); cite `docs/proposals/PRICING_EXPOSURE_PLAN.md §2`.

| Inbox usage | Maps to meter | Numbers (Indie / Studio / Scale / Ent) |
|---|---|---|
| **Email persisted** (each `email.*` event) | **events/mo quota** (`EventsQuota`, `HasQuota`) | 500K / 5M / 50M / unlimited events |
| **AI triage + draft calls** (per-email agent invocations) | **x402 allowance** (`HasX402Allowance`) | 50K / 500K / 5M / unlimited included calls, then **$0.0001/call** overage |
| **Outbound send volume** | *no native meter today* — see below | n/a |

- **Events and x402 are independent meters** (RECON): a tenant can have events-quota left but owe per-call x402 once triage allowance is spent. AI triage is the natural x402-priced surface — and because **Free/Self-Host are hard-403'd from any x402 priced route** (`tierAllowedForX402`, fail-closed), **AI inbox triage is automatically a paid-tier-only feature** with the existing upgrade prompt pointing at `/billing`. That is the right monetization shape.
- **To price AI triage with zero new enforcement code:** register the triage route in `apps/control-plane/config/x402-pricing.json` (amount in USDC 6-decimal units; `"100"` = $0.0001). The existing `QuotaGatedMiddleware` tier→allowance→quota→payment pipeline gates it.
- **Send volume has no modeled meter** (RECON gap, stated not invented). Two options: (a) add a `SendQuota` field to `TierQuotas`/`QuotaMetadata` + a dimension in `QuotaGatedMiddleware`, or (b) treat each send as an x402-priced route so it draws from the x402 allowance/overage. **Recommend (b)** for GA — no schema change, reuses the gate. **Assumption:** until then, send is bounded only by the provider's per-mailbox quota (Nylas/Gmail/Graph), which is acceptable for the founder-dogfood and conversational-reply use case.
- **Metering is event-sourced, not in-memory** (RECON): each processed message emits a Core event (e.g. `email.message.ingested` / `x402.allowance.consumed`), reconciled per billing period by a `SyncX402UsageUseCase`-style reconciler on the scheduler tick — model the email reconciler on it.
- **Self-Host** gets the connector code but no included x402 allowance — they run their own Nylas account and bear that cost directly, consistent with the tier's "runs own" framing.

**Marketing copy** lives in `apps/web/src/lib/config.ts` (`siteConfig.pricing`); live prices come from `/api/v1/billing/catalog`. Keep `siteConfig.pricing` and `TierQuotaMap` in sync; never hand-edit a price into both.

**Satellite framing:** the inbox is to the workspace product what Folk's activity feed is to the CRM — a satellite that *feeds* the graph, not a separate SKU. It does not get its own pricing page; it is a capability of the Prime workspace tiers, gated by the same entitlements.

---

## 7. Security & Privacy

- **OAuth scopes (least privilege):** Nylas hosted OAuth per mailbox grant. Request `gmail.modify` only if the agent must move/label/send; prefer `gmail.readonly` + a scoped send path where the product allows. Microsoft needs `offline_access` for refresh. **We own the Google CASA security review + Microsoft app verification** spanning all tenants (a real, named prerequisite — see Risks).
- **Per-tenant credential isolation (deliberate divergence from the "secrets in Postgres" norm — called out, not waved away):** the LAW puts connector OAuth secrets / webhook config in PostgreSQL (operational metadata). **This deployment has no Postgres** — CP and Core are event-sourced — so connector secrets live in **Core config** instead, keyed `connector:email:<tenant_id>:grant`, the *same pattern already in production* for `agent:<tenant>:cdp_wallet` and `user:<userID>:core_api_key`. That is a conscious divergence from the literal LAW wording, justified by the no-Postgres topology, not a loophole. CP reads it back via a `CoreWalletLookup`-style adapter. **Core config is plaintext KV** (verified: no at-rest encryption of config values exists in Core today). Therefore **CP-side envelope encryption of the grant blob before `SetConfig` is a REQUIRED, NEW build item — not existing reuse.** It must land **before any tenant grant ships**, gated at **P3** (the multi-tenant-grants phase): a plaintext `grant_id` must never reach Core config in prod. Nylas itself holds and auto-refreshes the *provider* OAuth tokens, so we never store Google/Microsoft refresh tokens — a deliberate blast-radius reduction, but it does **not** remove the need to encrypt the Nylas `grant_id`/connector secret CP-side.
- **Encryption / durability at rest in Core:** email content and envelope are durable Core events (WAL CRC32 + Parquet Snappy). The full body is **inline in the event payload** (Core has no blob/attachment store — §4.3), so it inherits the same WAL+Parquet durability as every other event; there is no separate blob to protect. The *connector-secret* blobs are encrypted CP-side per the REQUIRED P3 item above. Email content is **never** in PostgreSQL — PostgreSQL is operational metadata only, and this stack has no Postgres at all (CP/Core are event-sourced).
- **Content-retention policy:** retention follows the tier's `retention_days` (Indie 14d / Studio 90d / Scale 365d / Ent forever). Because email events are immutable and append-only, "deletion" is a tombstone event + retention sweep, not a mutation — provenance is preserved while honoring the policy.
- **What NOT to send to an LLM by default:** never auto-feed full bodies, attachments, or full recipient lists into a model. The agent sees **subject + snippet + recall context** by default; full body is pulled only on explicit `include_context` / `inbox_draft` with a body fetch, and attachments are never embedded or sent to the model without an explicit request. Only `subject + snippet` go into the embedding text — PII-heavy fields (full headers, raw addresses beyond the resolved person) stay out of the vector. Nylas is a **content sub-processor in every tenant's mail path** — tenants must accept it in the DPA; that is a documented trust dependency, not a hidden one.

---

## 8. Phased Path

Mirrors the workspace plan's P0–P5 style; each phase has one "what it proves."

| Phase | Scope | What it proves |
|---|---|---|
| **P0 — Founder-dogfood wedge** | One mailbox, single tenant. **Nylas Agent Accounts are Beta** — if not GA-ready when P0 lands, P0 uses the **founder's real grant** instead (the pipeline is identical either way; see open question 5). Connector app + webhook → CP → Core `email.received`/`email.sent` → Prime `interaction` nodes + edges + embed. `inbox_recall_thread` + `inbox_draft` MCP verbs. Dedupe via `entity_id`+`expected_version` (first-ingest only — §4.6). | **Email is durable Core data, visible as graph, and recallable from Claude Code** — end to end, on one mailbox. |
| **P1 — Triage + send loop** | `inbox_triage` + `inbox_send` (confirm-gated). Contact-timeline projection over interactions. Thin "interactions" view in the workspace graph. | **A human/agent can triage, draft-with-context, and reply as themselves** without leaving the terminal. |
| **P2 — Billing-aware single-tenant** | Wire the email meter: events quota for persistence, x402 allowance for triage calls; register the triage x402 route; CP allowance check at the webhook (fail-closed); event-sourced reconciler on the scheduler tick. | **The feature meters cleanly against the 5-tier model** and is paid-tier-gated. |
| **P3 — Multi-tenant grants** | Per-tenant grant onboarding via hosted OAuth; **build CP-side envelope encryption of the grant blob before `SetConfig`** (REQUIRED NEW item — gate: no plaintext grant ships) then write `connector:email:<tenant>:grant` to Core config; stamp `properties.tenant_id` on every node; single Prime writer in HTTP mode. | **Two tenants' inboxes stay isolated and their grants are encrypted at rest** — separate grants, separate events, separate graph scope. |
| **P4 — Send metering + hardening** | Add the send meter (x402-priced route, option b); wire Core's idempotency registry into ingest (option b dedupe); Google CASA + Microsoft verification complete. | **Outbound is bounded and billed; redelivery is dedup'd WAL-side; verification clears prod.** |
| **P5 — GA polish** | Smart suggestions ("unreplied N days"), entity-resolution/dedup hardening, optional second `EmailProvider` impl (the TBD bidirectional-sync provider) for cost, optional Postmark outbound leg for bulk. | **The inbox is a durable, multi-tenant, recall-first satellite at parity with the workspace wedge.** |

**Ship P0 first.** The founder-dogfood slice on one mailbox proves the entire pipeline (provider → CP → Core → Prime → Claude Code) with the smallest surface; everything after is metering, isolation, and parity on the *same* code path.

---

## 9. Risks, Open Questions, and the Decision

### Risks (highest first)
1. **OAuth-app verification gate (biggest risk to the call).** We own the **Google CASA security review + Microsoft app verification** covering all tenants. This is a multi-week, externally-gated dependency that blocks multi-tenant GA (not P0/dogfood, which can use a single grant). If verification stalls, GA slips regardless of code readiness.
2. **Nylas in the mail data path.** A content sub-processor touching every tenant's mail — a compliance/trust dependency tenants must accept. Mitigated by US/EU residency + SOC2/HIPAA, but it is a real third party in the path. The provider-agnostic trait keeps a DIY-Graph exit available.
3. **Cost scales linearly** (~$2/account/mo list-price estimate; ~$2k/mo at 1k mailboxes pre-discount — confirm before budgeting). Mitigated *in principle* by swapping in the TBD bidirectional-sync provider behind the same trait — but the swap is **not yet justified by a confirmed cheaper marginal rate**; a named vendor and its real fleet-scale price must be verified first (§3.1).
4. **Nylas Agent Accounts are Beta.** P0's "agent identity per customer" home is a Beta Nylas feature (https://developer.nylas.com/docs/v3/getting-started/agent-accounts). The verdict must not silently depend on it: if it is not GA-ready, **P0 falls back to the founder's real grant** — same pipeline, no architecture change (open question 5). The risk is to the *convenience* of the dogfood mailbox, not to the design.
5. **Per-grant provider rate limits** (Gmail/Graph quotas, ~200 msgs/day on free tier) cap per-tenant throughput; requires backoff. Acceptable for conversational replies, **not** bulk.
6. **Single-writer Prime + single-instance CP** bound multi-tenant scale-out until the documented HTTP-writer / multi-instance work lands.

### Open questions
- Send-volume metering: add a `SendQuota` dimension, or price send as an x402 route? (Recommend x402 route — no schema change.)
- ACL scope of a privately-synced inbox: are a user's interactions scoped to that user, or shared in the tenant workspace? (Plan defines node-level ACL in P0 but not the inbox-specific rule.)
- CP-side envelope encryption of the grant blob is **decided, not open** — it is a REQUIRED NEW build item gated at P3 (§7). The only open sub-question is *when*: land Core-side config encryption first, or ship CP-side envelope encryption only? (Plan: CP-side for P3; revisit Core-side later. Either way no plaintext grant ships.)
- Wire Core's idempotency registry into ingest now, or rely on `expected_version` first-ingest determinism + connector-side lifecycle dedupe through GA? (P0 uses determinism + connector keying; registry at P4 — §4.6.)
- **Nylas Agent Accounts are Beta** — confirm GA-readiness before P0, else use the founder's real grant for the dogfood mailbox (the fallback is explicit and pipeline-identical; this is the one place the verdict touches a Beta feature).

### Decision

**GO — CONDITIONAL.** Build the AI inbox as a Prime-graph satellite on **Nylas Email API v3**, integrating the connector while reusing Core (events), Prime (graph/recall), and the Control Plane (auth/tenant/billing) unchanged. Ship the founder-dogfood P0 wedge first. The conditions on GA are: (1) Google CASA + Microsoft app verification cleared, and (2) x402 enabled in prod with the triage route priced. P0 through P2 carry neither condition and can start immediately.

**The single biggest risk to the call is the externally-gated OAuth-app verification** — it does not threaten the architecture or the dogfood wedge, but it is the long pole for multi-tenant GA, so verification must start in parallel with P0.

---

## 10. Invariants Honored

| Architecture LAW | How this design honors it |
|---|---|
| **Core IS the database; email events AND content are durable Core events; never route around Core** | All `email.*` events (received/sent/triaged/replied/archived) and content (full body **inline in the `email.received` payload** — Core has no blob store) live in Core's WAL+Parquet. The event *is* the content. No side mail store. §4.1, §4.3. |
| **PostgreSQL = operational metadata ONLY; never email events/content** | No email data in Postgres (the stack has **no Postgres at all**; CP/Core are event-sourced). Connector OAuth/grant secrets are a **deliberate, called-out divergence**: with no Postgres they live in Core config (same pattern as `cdp_wallet` / `core_api_key`), and must be **CP-side envelope-encrypted before `SetConfig`** — a REQUIRED NEW build item gated at **P3** so a plaintext grant never ships. §7. |
| **Prime = stateless graph/recall over Core; does not ship in Core; email → Core events → interaction nodes** | Prime ingester folds Core events into `interaction` nodes; no Prime-side database; engine unchanged (free-form `node_type`). §3.3, §4.4. |
| **Control Plane owns ALL public auth + billing; Core never authenticates; webhook terminates at CP** | `POST /api/v1/webhooks/email` terminates at CP (HMAC-verify, tenant resolve, allowance check), then a 60s delegation JWT carries the tenant inward. §4.1. |
| **App isolation; connectors talk over the network; a new connector is its own app or in CP** | Connector is a standalone, network-only app that reaches Core/Prime **only via the Control Plane over HTTP**. Hard prohibitions: **no `path` dependency on `apps/core`** (or any other app) and **no `COPY apps/<other-app>/`** in its Dockerfile — if it needs Core types it uses the public SDK / HTTP API, not a crate path-dep. (Note: `apps/prime-mcp` is *not* the model here — it path-depends on `apps/core` and COPYs `apps/core/` into its image; that coupling is exactly what the connector must avoid.) §3.1, §3.2. |
| **Visible surface = Prime workspace graph + Claude Code MCP; NOT a bespoke mail client** | Surface is the workspace graph (interactions on the contact timeline, recall, neighbors) + four MCP verbs. New UI is a thin interactions view, not a mail client. §1, §4.5. |

---

*References: `docs/plans/2026-06-10-prime-workspace-crm-design.md` (the `interaction` node + `involves`/`mentions` + P2 email leg); `apps/prime-mcp/` (allsource-prime v0.22.0, 19 `prime_*` MCP tools — verified against `apps/prime-mcp/src/tools.rs` dispatch arms); `apps/core/src/prime/` (graph/vectors/recall); `apps/control-plane/` (Go/Gin ingress, auth/delegation, x402 allowance, LemonSqueezy webhook pattern); `apps/control-plane/internal/domain/entities/subscription.go` (`TierQuotaMap`); `docs/proposals/PRICING_EXPOSURE_PLAN.md §2`. Email-API evaluation grounded in the RESEARCH bundle (Nylas v3, Resend, Postmark, and an unnamed `bidirectional_sync` provider class — see §3.1 footnotes for the per-provider doc URLs that make each contested axis auditable).*
