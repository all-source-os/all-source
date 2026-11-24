# The Amazing AllSource Demo

**Goal**: A 5-minute demo that makes people say "I need this."

---

## The Demo: "Debug Yesterday's Production Incident"

### The Story

You're an on-call engineer. At 3am, customers reported orders weren't processing. By morning, everything looks fine. Your database shows current state—orders are processing. But what happened at 3am?

**With a traditional database**: You're screwed. Check logs? Hope timestamps are accurate? Good luck.

**With AllSource**: Travel back in time.

---

## Demo Script (5 minutes)

### Act 1: The Setup (30 seconds)

**On screen**: AllSource dashboard showing healthy metrics

> "This is AllSource, our event store. Right now, everything looks fine—469K events ingested today, 11.9μs query latency. But last night at 3am, we had an incident. Let's investigate."

### Act 2: Time Travel (60 seconds)

**Action**: Click "Time Travel" button, select "Feb 11, 3:00 AM"

> "First, let's see what the system looked like at 3am."

**On screen**: Dashboard transforms—shows metrics FROM that timestamp
- Error rate: Spiked to 47%
- Order processing: Flatlined
- Payment events: Still coming in

> "Now we can see it. Orders stopped processing, but payments were still flowing. Customers were being charged for orders that never completed."

**Action**: Click on an affected order entity

> "Let's look at a specific order—order-7834."

**On screen**: Entity timeline showing:
```
02:58:12 - OrderCreated (amount: $149.00)
02:58:13 - PaymentAuthorized
02:58:14 - PaymentCaptured
02:58:15 - InventoryCheckRequested
--- 3:00 AM - No more events ---
```

> "See that? The order got stuck after InventoryCheckRequested. Something happened at 3am that blocked inventory checks."

### Act 3: The Replay (60 seconds)

**Action**: Click "Replay Events" for the inventory service

> "Let's replay the events around that time."

**On screen**: Event stream playing showing:
```
02:59:58 - ConfigDeployed (service: inventory, version: 2.4.1)
02:59:59 - InventoryCheckRequested (order: 7832) ✓
02:59:59 - InventoryCheckCompleted (order: 7832) ✓
03:00:01 - InventoryCheckRequested (order: 7833) ⏳
03:00:01 - InventoryCheckRequested (order: 7834) ⏳
03:00:02 - InventoryCheckRequested (order: 7835) ⏳
--- All stuck ---
```

> "There it is. A config deployment at 2:59:58. Every inventory check after that got stuck. The deployment broke something."

### Act 4: AI Analysis (90 seconds) ⭐ THE WOW MOMENT

**Action**: Open Claude Desktop panel (side-by-side with AllSource)

> "But I don't want to manually correlate all this. Let's ask Claude."

**Type into Claude**:
> "What caused the inventory service issues at 3am on Feb 11?"

**Claude responds** (using MCP tools):
```
I analyzed the events around the incident. Here's what happened:

📊 **Root Cause Analysis**

At 02:59:58, a configuration deployment was pushed to the
inventory service (version 2.4.1).

Immediately after:
- InventoryCheckRequested events continued arriving
- InventoryCheckCompleted events stopped entirely
- 847 orders were affected between 03:00 and 05:30

🔍 **The Specific Issue**

Comparing the config changes:
- Previous: `inventory.db.pool_size: 10`
- New: `inventory.db.pool_size: 0`  ← This broke it

Someone set the database pool to zero, making the service
unable to connect to the database.

📋 **Recommended Actions**
1. Rollback config to version 2.4.0
2. Review 847 affected orders for refund processing
3. Add validation to prevent pool_size < 1

Would you like me to generate the list of affected order IDs?
```

> "Claude didn't just find the bug—it found the exact config line, calculated the impact, and recommended fixes. All because it could query our complete event history."

### Act 5: The Resolution (60 seconds)

**Action**: Ask Claude to generate affected orders

**Claude responds**:
```
Here are the 847 affected orders:

| Order ID | Customer | Amount | Status |
|----------|----------|--------|--------|
| 7833 | alice@... | $89.00 | Stuck |
| 7834 | bob@... | $149.00 | Stuck |
...

I've also created a projection to track these orders.
Run `GET /projections/incident-feb11-affected-orders/state`
to get the latest status.
```

**Action**: Show the projection in the dashboard

> "Claude created a projection that tracks all affected orders. As we process refunds, this updates in real-time. The incident is documented, the impact is tracked, and we have a complete audit trail."

### Closing (30 seconds)

> "That's AllSource. Not just an event store—a time machine for your data. Query any point in history. Replay any sequence. Let AI analyze patterns you'd never find manually.

> 469K events per second. 11.9 microsecond queries. 27 AI tools that give Claude perfect memory of your entire application history.

> [Show signup page] Start free. 50K events per month. No credit card required."

---

## What This Demo Showcases

| Feature | How It's Shown |
|---------|---------------|
| **Time-travel queries** | Dashboard at historical timestamp |
| **Event replay** | Streaming playback of incident |
| **MCP tools** | Claude analyzing events live |
| **Projections** | Auto-created incident tracker |
| **Performance** | Stats visible throughout |
| **Audit trail** | Complete history preserved |

---

## What We Need to Build

### Priority 0: Demo Infrastructure (MUST HAVE)

#### 1. Time Travel UI Component
**File**: `apps/web/src/components/dashboard/time-travel-picker.tsx`

```tsx
// Needs:
// - Date/time picker that sets global "as_of" context
// - Visual indicator when viewing historical data
// - "Return to Present" button
// - Keyboard shortcut (Cmd+T)
```

**Backend**: Already supported via `?as_of=` parameter

#### 2. Demo Data Seed Script
**File**: `apps/query-service/priv/repo/seeds/demo_incident.exs`

```elixir
# Generate realistic incident data:
# - 10,000 normal events (orders, payments, inventory)
# - Config deployment event at specific timestamp
# - 847 "stuck" orders after deployment
# - Resolution events showing fix
```

**Requirements**:
- Deterministic (same data every run)
- Realistic timestamps and patterns
- Named entities for storytelling (order-7834, etc.)

#### 3. Event Replay Component
**File**: `apps/web/src/components/events/event-replay.tsx`

```tsx
// Needs:
// - Play/pause/speed controls
// - Visual timeline scrubber
// - Event-by-event stepping
// - Highlight anomalies (stuck events)
```

#### 4. MCP Demo Integration
**File**: `apps/web/src/components/demo/claude-panel.tsx`

```tsx
// Needs:
// - Side panel showing Claude conversation
// - Pre-loaded demo prompts
// - Simulated responses (for offline demo)
// - "Try it yourself" link to Claude Desktop
```

**Note**: For live demos, use actual Claude Desktop. For recorded/offline demos, simulate responses.

### Priority 1: Enhanced Visualizations

#### 5. Entity Timeline View
**File**: `apps/web/src/components/events/entity-timeline.tsx`

```tsx
// Needs:
// - Vertical timeline for single entity
// - Event details on hover
// - Gap detection (show "no events" periods)
// - Correlation with other entities
```

#### 6. Dashboard Historical Mode
**File**: `apps/web/src/components/dashboard/historical-dashboard.tsx`

```tsx
// Needs:
// - All dashboard widgets respect "as_of" context
// - Visual differentiation (sepia tone? border?)
// - Metrics calculated for that point in time
// - Diff view vs current state
```

#### 7. Projection Creation UI
**File**: `apps/web/src/components/projections/create-projection-dialog.tsx`

```tsx
// Needs:
// - Form to define projection
// - Preview of results
// - "Create from AI suggestion" button
// - Real-time state updates
```

### Priority 2: Demo Polish

#### 8. Guided Demo Mode
**File**: `apps/web/src/components/demo/guided-tour.tsx`

```tsx
// Needs:
// - Step-by-step overlay
// - Highlights UI elements
// - Auto-advances on action
// - Skip/restart controls
```

#### 9. Demo Environment Setup
**File**: `scripts/setup-demo.sh`

```bash
# One-command demo setup:
# - Start services in demo mode
# - Seed demo data
# - Open browser to dashboard
# - Print Claude Desktop config
```

#### 10. Performance Visualization
**File**: `apps/web/src/components/dashboard/benchmark-widget.tsx`

```tsx
// Needs:
// - Live ingestion counter
// - Query latency histogram
// - Comparison vs competitors (subtle)
// - "Run benchmark" button
```

---

## Implementation Plan

### Week 1: Core Demo Infrastructure

| Task | Effort | Owner |
|------|--------|-------|
| Time travel picker component | 2 days | Frontend |
| Demo data seed script | 1 day | Backend |
| Historical dashboard mode | 2 days | Frontend |
| Event replay component | 2 days | Frontend |

### Week 2: MCP Integration & Polish

| Task | Effort | Owner |
|------|--------|-------|
| Claude panel component | 2 days | Frontend |
| Entity timeline view | 2 days | Frontend |
| Projection creation UI | 1 day | Frontend |
| Demo setup script | 1 day | DevOps |
| Guided tour | 1 day | Frontend |

### Week 3: Recording & Distribution

| Task | Effort | Owner |
|------|--------|-------|
| Record demo video | 1 day | Marketing |
| Create GIF snippets | 0.5 day | Marketing |
| Embed in landing page | 0.5 day | Frontend |
| Write demo script docs | 0.5 day | Docs |
| Test with external users | 2 days | All |

---

## API Requirements

### Existing (✅ Ready)

- `GET /api/events?as_of={timestamp}` - Time travel
- `GET /api/entities/{id}/events` - Entity history
- `GET /api/analytics/correlation` - Pattern detection
- `GET /api/projections/{id}/state` - Projection state
- All 27 MCP tools

### Needs Implementation

| Endpoint | Purpose |
|----------|---------|
| `GET /api/dashboard/stats?as_of={ts}` | Historical dashboard metrics |
| `POST /api/demo/seed` | Trigger demo data seeding |
| `GET /api/events/replay?start={ts}&end={ts}` | Streaming replay |
| `GET /api/entities/{id}/timeline` | Formatted timeline |

---

## Demo Environments

### 1. Local Development
```bash
make demo-setup  # Seeds data, starts services
make demo-run    # Opens browser with guided tour
```

### 2. Staging (demo.all-source.xyz)
- Pre-seeded with demo data
- Reset nightly
- Public read-only access
- Demo API key published

### 3. Conference/Live Demo
- Isolated environment
- Manual seed trigger
- Backup recorded video if network fails

---

## Success Metrics

| Metric | Target |
|--------|--------|
| Demo video views | 10K in first month |
| "Try Demo" button clicks | 30% of landing page visitors |
| Signup after demo | 15% conversion |
| Time-to-first-event | <5 minutes |
| Demo NPS score | >50 |

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Network issues during live demo | Pre-recorded backup, offline mode |
| Claude rate limits | Simulated responses for high-traffic |
| Demo data looks fake | Use realistic patterns, real-world scenario |
| Too complex for 5 minutes | Practice, trim ruthlessly, focus on wow moment |
| Competitor copies demo | Keep innovating, our MCP integration is hard to replicate |

---

## The "Wow" Moments

1. **Time Travel**: Dashboard literally changes to show past state
2. **Stuck Events**: Visual gap in timeline—something's wrong
3. **AI Analysis**: Claude identifies root cause in seconds
4. **The Fix**: Exact config line that broke it
5. **Impact Assessment**: 847 orders, auto-generated report

These moments should make viewers think: "I can't do this with my current database."

---

## Appendix: Demo Data Schema

### Events to Generate

```elixir
# Normal operations (before incident)
- UserSignedUp (500 events, spread over 24h)
- OrderCreated (2000 events)
- PaymentAuthorized (1950 events)
- PaymentCaptured (1900 events)
- InventoryCheckRequested (1900 events)
- InventoryCheckCompleted (1890 events)
- OrderShipped (1800 events)

# The incident
- ConfigDeployed (1 event, 02:59:58)

# Stuck orders (incident)
- OrderCreated (847 events, 03:00-05:30)
- PaymentAuthorized (847 events)
- PaymentCaptured (847 events)
- InventoryCheckRequested (847 events)
# NO InventoryCheckCompleted - they're stuck

# Resolution
- ConfigRolledBack (1 event, 05:30)
- InventoryCheckCompleted (847 events, 05:31-06:00)
- OrderShipped (847 events, 06:00-08:00)
```

### Key Entities

| Entity | ID | Story |
|--------|-----|-------|
| Affected Order | order-7834 | The example we zoom into |
| Customer | customer-alice | Sympathetic affected user |
| Config | config-inventory-2.4.1 | The bad deployment |
| Service | service-inventory | What broke |

---

**Document Owner**: AllSource Team
**Last Updated**: 2026-02-12
**Status**: PROPOSED
