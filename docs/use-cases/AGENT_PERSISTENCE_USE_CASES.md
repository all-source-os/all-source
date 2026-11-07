# Agent Persistence Use Cases — AllSource as the Agent Database

> **Reference**: [llms.txt](../../apps/web/public/llms.txt) · [Agent auth flow](./AGENT_TEAM_AUTH.md) · [Projection worker](./custom-projections.md)

AI agents need durable state across sessions, coordination between instances, and an audit trail of every decision. AllSource is purpose-built for this: immutable events, time-travel queries, 469K events/sec ingest, 11.9μs queries. Every pattern here works with plain HTTP — no SDK required.

---

## UC-1: Agent Memory — Persist Learned Facts Across Sessions

**Actor**: Any stateless LLM agent (Claude, GPT-4, custom)
**Problem**: Agent learns something (user preference, domain fact, prior attempt outcome) but loses it when the process exits.

### Event schema

```json
POST /api/v1/events
{
  "event_type": "agent.memory.stored",
  "entity_id": "agent-{agent_id}",
  "payload": {
    "key":       "user_preference.timezone",
    "value":     "America/New_York",
    "source":    "user_stated",
    "confidence": 1.0
  }
}
```

```json
POST /api/v1/events
{
  "event_type": "agent.memory.invalidated",
  "entity_id": "agent-{agent_id}",
  "payload": {
    "key":    "user_preference.timezone",
    "reason": "user corrected"
  }
}
```

### Session startup flow

```
Agent starts
  → GET /api/v1/events/query?entity_id=agent-{id}&event_type=agent.memory&sort=asc
  → fold events: apply stored, remove invalidated
  → working memory reconstructed
  → continue conversation with full context
```

### Why events, not a key-value store

Storing facts as events preserves the history of what the agent believed and when. Time-travel queries (`?before=<timestamp>`) let you reconstruct the agent's world model at any prior point — invaluable for debugging incorrect decisions. A key-value store gives you only the current value.

---

## UC-2: Multi-Agent Task Queue via `chronis`

**Actor**: Orchestrator agent + N worker agents, all sharing one team tenant
**Problem**: Orchestrator needs to dispatch work; workers need to claim tasks without double-assignment; human operators need visibility.

### Setup

All agents configure `chronis` against the same team tenant:

```toml
# .chronis/config.toml (each agent has a unique api_key)
mode    = "remote"

[sync]
remote_url = "https://api.all-source.xyz"
api_key    = "eyJ..."   # dedicated agent key from Settings → Team → Agent Keys
```

### Orchestrator: decompose and enqueue

```bash
# Orchestrator breaks a goal into tasks
cn add "Fetch competitor pricing data"  --type task --priority p1 --id fetch-pricing
cn add "Analyse pricing vs our SKUs"    --type task --priority p2 --id analyse-pricing \
       --blocked-by fetch-pricing
cn add "Draft pricing recommendation"   --type task --priority p2 --id draft-rec \
       --blocked-by analyse-pricing
```

Each `cn add` emits a `task.created` event to the team tenant. The `blocked_by` graph is stored in event payloads — workers that call `cn ready` only see tasks whose blockers are `done`.

### Worker: claim and complete

```bash
# Worker polls for available work
cn ready          # returns only unblocked open tasks

# Worker claims and works
cn claim fetch-pricing --agent-id worker-a

# ... executes the fetch ...

cn done fetch-pricing
```

`task.claimed` and `task.completed` events land in the shared stream. The next worker calling `cn ready` now sees `analyse-pricing` as available.

### Human visibility

The dashboard query `GET /api/v1/events/query?event_type=task&sort=desc` shows every agent action in chronological order. Any event can be expanded to see the full payload including `agent_id`.

---

## UC-3: Reasoning Trace — Full Audit Trail of Agent Decisions

**Actor**: Any agent that calls tools or makes decisions with consequences
**Problem**: Agent took an action that caused an incident. What did it observe, what did it decide, why?

### Event types

| Event | Emitted when |
|---|---|
| `agent.observation` | Agent reads external state (API response, file contents) |
| `agent.reasoning` | Agent produces a plan or chooses an action |
| `agent.tool_call.started` | Agent invokes a tool |
| `agent.tool_call.completed` | Tool returns a result |
| `agent.tool_call.failed` | Tool throws or times out |
| `agent.decision` | Agent reaches a conclusion |

### Ingest examples

```json
POST /api/v1/events
{
  "event_type": "agent.tool_call.started",
  "entity_id":  "run-{run_id}",
  "payload": {
    "tool":      "bash",
    "input":     "git push origin main --force",
    "agent_id":  "claude-sonnet-4-6",
    "run_id":    "run-{run_id}"
  }
}
```

```json
POST /api/v1/events
{
  "event_type": "agent.tool_call.completed",
  "entity_id":  "run-{run_id}",
  "payload": {
    "tool":      "bash",
    "exit_code": 0,
    "duration_ms": 1240
  }
}
```

### Incident reconstruction

```
GET /api/v1/events/query?entity_id=run-{run_id}&sort=asc
→ complete timeline of every observation, tool call, and decision in order
→ time-travel to any point: ?before=<timestamp-of-incident>
```

Reasoning steps and tool calls are correlated by `entity_id = run-{run_id}`. A single query reconstructs the full agent session without any joins.

---

## UC-4: Idempotent Tool Execution — Avoid Double-Running Side Effects

**Actor**: Agent running in an unreliable environment (network retries, process restarts)
**Problem**: Agent crashed after sending an email but before recording success. On restart it sends the email again.

### Pattern: write-before-execute

```
1. Agent prepares tool call
2. POST agent.tool_call.started  (entity_id = run-{run_id}, tool = "send_email", idempotency_key = sha256(args))
3. Execute the tool
4. POST agent.tool_call.completed OR agent.tool_call.failed
```

### Restart guard

On process start (or after a crash), before executing any tool:

```
GET /api/v1/events/query?entity_id=run-{run_id}&event_type=agent.tool_call.started
→ for each started event, check if a corresponding completed or failed event exists
→ if started but no completed/failed: tool was in-flight when crash occurred
  → check idempotency_key against tool's own idempotency mechanism before re-executing
```

This gives the agent a server-authoritative record of what it already did — no local state needed.

---

## UC-5: Multi-Agent Coordination — Shared World Model

**Actor**: A team of specialized agents (researcher, writer, reviewer) working on a long-running task
**Problem**: Each agent needs to know what the others have done without polling each other directly.

### Event stream as shared memory

All agents write to the same team tenant. Each subscribes to the other agents' output events:

```
researcher  → POST agent.finding     (entity_id = project-{id})
writer      → POST agent.draft       (entity_id = project-{id})
reviewer    → POST agent.review      (entity_id = project-{id})
```

### Live subscription via WebSocket

```
GET wss://api.all-source.xyz/api/v1/events/stream?event_type=agent.&entity_id=project-{id}
Authorization: Bearer <api_key>
```

Each agent receives a push notification when a peer emits an event. No polling, no message broker needed.

A long-running agent can send a 60-second ES256 assertion signed with a registered client key in place of `<api_key>` (SDK `Es256AssertionProvider`, or `ALLSOURCE_CLIENT_*` for `allsource-mcp`). The SDKs sign a fresh one for each WebSocket connect. See [Registered client keys](../guides/REGISTERED_CLIENT_KEYS.md).

### State reconstruction at any point

```
GET /api/v1/events/query?entity_id=project-{id}&sort=asc&before=<timestamp>
→ fold events to reconstruct shared world model at any prior moment
```

A new agent joining mid-project replays the event history to catch up before taking its first action.

---

## UC-6: Agent Self-Healing — Learn from Past Failures

**Actor**: Long-running agent with repetitive tasks (nightly report, periodic sync)
**Problem**: Agent repeatedly fails on the same input. Without a history it retries the same approach forever.

### Failure events

```json
POST /api/v1/events
{
  "event_type": "agent.task.failed",
  "entity_id":  "agent-{agent_id}",
  "payload": {
    "task_type":   "generate_report",
    "input_hash":  "sha256({input})",
    "error":       "timeout after 30s",
    "attempt":     3,
    "strategy":    "default"
  }
}
```

### Pre-execution failure check

Before attempting a task, agent queries its own failure history:

```
GET /api/v1/events/query?entity_id=agent-{id}&event_type=agent.task.failed&limit=50&sort=desc
→ if input_hash matches recent failures: escalate or switch strategy
→ if strategy "default" failed 3 times: try strategy "chunked" instead
```

### Progressive strategy selection

```
attempt 1 → strategy: "default"
attempt 2 → strategy: "retry_with_backoff"  (after seeing 1 failure event)
attempt 3 → strategy: "chunked"              (after seeing 2 failure events)
attempt 4 → strategy: "escalate_to_human"   (after seeing 3 failure events)
```

The failure history is durable across process restarts. Each agent instance that starts up inherits the full failure context of its predecessors.

---

## UC-7: Agent Provisioning — Self-Service Onboarding

**Actor**: An agent that needs its own AllSource tenant at startup with no human involvement.

### One-shot onboarding

```
POST https://api.all-source.xyz/api/v1/onboard/start
Content-Type: application/json

{"email": "my-agent@example.com", "name": "My Agent"}

→ 201 { "api_key": "eyJ...", "tenant_id": "my-agent-at-example-com", "tier": "free", "events_quota": 100000 }
```

The agent stores `api_key` in its own configuration and immediately begins writing events. No dashboard login, no admin approval, no activation email. Free tier: 100K events/month.

### Upgrade path

When the agent exceeds the free tier, the orchestrating system can upgrade the tenant via the billing API — the `api_key` stays the same, the quota increases. The agent sees no interruption.

---

## Quick Reference — Agent Event Naming Conventions

| Namespace | Use for |
|---|---|
| `agent.memory.*` | Facts the agent learns and recalls |
| `agent.reasoning.*` | Plans, decisions, chain-of-thought summaries |
| `agent.tool_call.*` | Tool invocations and their outcomes |
| `agent.task.*` | Unit of work lifecycle (mirrors `task.*` from chronis) |
| `agent.observation.*` | External state the agent reads |
| `agent.finding.*` | Conclusions the agent reaches |

Use `entity_id` to group events by logical scope:

| `entity_id` | Groups |
|---|---|
| `agent-{agent_id}` | All events for a single agent instance |
| `run-{run_id}` | All events for one execution session |
| `project-{project_id}` | All events for a shared multi-agent project |
| `task-{task_id}` | All events for one chronis task |

---

## Related Docs

- [llms.txt](../../apps/web/public/llms.txt) — Machine-readable API quick-start (fetch and parse at agent startup)
- [Agent team auth](./AGENT_TEAM_AUTH.md) — How to provision agent keys, key lifecycle, permissions matrix
- [Custom projections](./custom-projections.md) — Build live read models over agent event streams with `ProjectionWorker`
- [Server-side projections](./SERVER_SIDE_PROJECTIONS_USE_CASES.md) — Query Service built-in projections
