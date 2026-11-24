# Demo Implementation Checklist

**Goal**: Ship the demo in 2 weeks

---

## Phase 1: Core Infrastructure (Days 1-5)

### 1.1 Time Travel Context Provider
**Priority**: P0 | **Effort**: 1 day

- [ ] Create `TimeTravelProvider` React context
- [ ] Add `useTimeTravel()` hook returning `{ asOf, setAsOf, isHistorical, returnToPresent }`
- [ ] Wrap dashboard layout with provider
- [ ] Pass `as_of` param to all API calls when set

**Files to create**:
```
apps/web/src/lib/context/time-travel-context.tsx
apps/web/src/hooks/use-time-travel.ts
```

**Files to modify**:
```
apps/web/src/app/(dashboard)/layout.tsx - Add provider
apps/web/src/lib/api/client.ts - Add as_of to requests
```

---

### 1.2 Time Travel Picker Component
**Priority**: P0 | **Effort**: 1 day

- [ ] Date picker with time precision
- [ ] Quick presets: "1 hour ago", "Yesterday 3am", "Last week"
- [ ] "Return to Present" button
- [ ] Visual indicator when viewing historical data (banner/badge)
- [ ] Keyboard shortcut: Cmd/Ctrl + T

**Files to create**:
```
apps/web/src/components/dashboard/time-travel-picker.tsx
apps/web/src/components/dashboard/historical-mode-banner.tsx
```

**UI Design**:
```
┌─────────────────────────────────────────────┐
│ 🕐 Viewing data from: Feb 11, 3:00 AM      │
│    [Return to Present]                      │
└─────────────────────────────────────────────┘
```

---

### 1.3 Demo Data Seed Script
**Priority**: P0 | **Effort**: 1 day

- [ ] Create Elixir seed script for incident scenario
- [ ] Generate 10,000+ realistic events
- [ ] Include the "config deployment breaks inventory" story
- [ ] Deterministic (same output every run)
- [ ] Idempotent (can re-run safely)

**Files to create**:
```
apps/query-service/priv/repo/seeds/demo_incident.exs
apps/query-service/lib/query_service_ex/demo/event_generator.ex
```

**Data requirements**:
```elixir
# Timeline:
# Feb 10, 00:00 - Feb 11, 02:59 → Normal operations
# Feb 11, 02:59:58 → Config deployment
# Feb 11, 03:00 - 05:30 → Incident (847 stuck orders)
# Feb 11, 05:30 → Config rollback
# Feb 11, 05:31+ → Recovery
```

---

### 1.4 Historical Dashboard Metrics
**Priority**: P0 | **Effort**: 2 days

- [ ] Modify stats endpoint to accept `as_of` parameter
- [ ] Calculate metrics as-of historical timestamp
- [ ] Update dashboard widgets to use time-travel context
- [ ] Show visual diff from current state

**Backend changes**:
```elixir
# apps/query-service/lib/query_service_ex_web/controllers/analytics_controller.ex
def stats(conn, %{"as_of" => as_of}) do
  # Calculate metrics up to as_of timestamp
end
```

**Frontend changes**:
```
apps/web/src/hooks/use-dashboard-stats.ts - Add as_of param
apps/web/src/components/dashboard/stats-cards.tsx - Show historical indicator
```

---

### 1.5 Event Replay Component
**Priority**: P0 | **Effort**: 2 days

- [ ] Fetch events in time range
- [ ] Play/pause controls
- [ ] Speed control (1x, 2x, 5x, 10x)
- [ ] Event-by-event stepping
- [ ] Visual timeline scrubber
- [ ] Highlight anomalies (gaps, errors)

**Files to create**:
```
apps/web/src/components/events/event-replay.tsx
apps/web/src/components/events/replay-controls.tsx
apps/web/src/components/events/replay-timeline.tsx
```

**UI Design**:
```
┌──────────────────────────────────────────────────┐
│  [◀◀] [▶ Play] [▶▶]  Speed: [2x ▼]              │
│                                                  │
│  ════════════●══════════════════════════════    │
│  02:58       03:00      03:30      04:00        │
│                 ↑                                │
│            Current: 03:00:15                     │
│                                                  │
│  ┌─────────────────────────────────────────┐    │
│  │ 03:00:01 InventoryCheckRequested ⏳     │    │
│  │ 03:00:01 InventoryCheckRequested ⏳     │    │
│  │ 03:00:02 InventoryCheckRequested ⏳     │    │
│  │ --- No InventoryCheckCompleted ---      │    │
│  └─────────────────────────────────────────┘    │
└──────────────────────────────────────────────────┘
```

---

## Phase 2: Entity & AI Features (Days 6-10)

### 2.1 Entity Timeline View
**Priority**: P1 | **Effort**: 2 days

- [ ] Vertical timeline visualization
- [ ] Event details on hover/click
- [ ] Gap detection (highlight missing events)
- [ ] Filter by event type
- [ ] Link to related entities

**Files to create**:
```
apps/web/src/components/events/entity-timeline.tsx
apps/web/src/components/events/timeline-event-card.tsx
apps/web/src/components/events/timeline-gap-indicator.tsx
```

**UI Design**:
```
Order #7834 Timeline
━━━━━━━━━━━━━━━━━━━━
│
├── 02:58:12  OrderCreated
│   └── amount: $149.00, customer: alice@...
│
├── 02:58:13  PaymentAuthorized
│   └── provider: stripe, auth_code: abc123
│
├── 02:58:14  PaymentCaptured
│   └── amount: $149.00
│
├── 02:58:15  InventoryCheckRequested
│   └── items: [{sku: "WIDGET-1", qty: 2}]
│
╳── ⚠️ GAP DETECTED: 2h 15m with no events
│
├── 05:31:22  InventoryCheckCompleted
│   └── status: available
│
└── 06:15:00  OrderShipped
    └── tracking: 1Z999AA10123456784
```

---

### 2.2 Claude Integration Panel
**Priority**: P1 | **Effort**: 2 days

- [ ] Collapsible side panel
- [ ] Pre-loaded demo prompts
- [ ] Display MCP tool calls being made
- [ ] Show response with formatting
- [ ] "Open in Claude Desktop" button

**Files to create**:
```
apps/web/src/components/demo/claude-panel.tsx
apps/web/src/components/demo/mcp-tool-indicator.tsx
apps/web/src/components/demo/demo-prompts.tsx
```

**For recorded demos**: Create mock responses that match the script

**For live demos**:
```json
// Claude Desktop MCP config
{
  "mcpServers": {
    "allsource-demo": {
      "command": "npx",
      "args": ["@allsource/mcp-server"],
      "env": {
        "ALLSOURCE_URL": "https://demo.all-source.xyz",
        "ALLSOURCE_API_KEY": "demo-key-xxx"
      }
    }
  }
}
```

---

### 2.3 Projection Creation from AI
**Priority**: P1 | **Effort**: 1 day

- [ ] "Create Projection" dialog
- [ ] Accept projection definition from Claude
- [ ] Show preview before creating
- [ ] Real-time state display after creation

**Files to create**:
```
apps/web/src/components/projections/create-projection-dialog.tsx
apps/web/src/components/projections/projection-preview.tsx
```

---

## Phase 3: Polish & Distribution (Days 11-14)

### 3.1 Guided Demo Tour
**Priority**: P2 | **Effort**: 1 day

- [ ] Step-by-step overlay using react-joyride or similar
- [ ] Highlights each demo step
- [ ] Can be triggered from "Take a Tour" button
- [ ] Tracks progress, allows skip

**Files to create**:
```
apps/web/src/components/demo/guided-tour.tsx
apps/web/src/lib/demo/tour-steps.ts
```

---

### 3.2 Demo Setup Automation
**Priority**: P2 | **Effort**: 0.5 day

- [ ] `make demo` command
- [ ] Seeds data
- [ ] Starts services
- [ ] Opens browser
- [ ] Prints Claude Desktop config

**Files to create**:
```
scripts/demo-setup.sh
Makefile (add demo target)
```

---

### 3.3 Demo Video Recording
**Priority**: P2 | **Effort**: 1 day

- [ ] Record 5-minute demo following script
- [ ] Create 30-second highlight reel
- [ ] Export GIFs for key moments
- [ ] Add captions/subtitles

**Output**:
```
assets/demo-full.mp4 (5 min)
assets/demo-highlight.mp4 (30 sec)
assets/demo-timetravel.gif
assets/demo-claude-analysis.gif
assets/demo-replay.gif
```

---

### 3.4 Landing Page Integration
**Priority**: P2 | **Effort**: 0.5 day

- [ ] Embed demo video in hero section
- [ ] Add "Try Demo" button linking to demo.all-source.xyz
- [ ] Add GIFs to feature sections

**Files to modify**:
```
apps/web/src/components/sections/hero.tsx
apps/web/src/app/page.tsx
```

---

## Backend Requirements

### API Endpoints Needed

| Endpoint | Status | Notes |
|----------|--------|-------|
| `GET /api/events?as_of=` | ✅ Exists | Verify works |
| `GET /api/entities/:id/events` | ✅ Exists | Verify works |
| `GET /api/analytics/stats?as_of=` | ⚠️ Modify | Add as_of support |
| `POST /api/demo/seed` | ❌ Create | Trigger seeding |
| `GET /api/demo/status` | ❌ Create | Check if demo data exists |

### MCP Tools to Verify

| Tool | Demo Usage |
|------|------------|
| `query_events` | Claude queries incident events |
| `reconstruct_state` | Claude shows order state at 3am |
| `analyze_changes` | Claude finds config diff |
| `find_patterns` | Claude identifies stuck pattern |
| `explain_entity` | Claude explains order journey |

---

## Testing Checklist

### Manual Testing
- [ ] Time travel works with all dashboard widgets
- [ ] Event replay plays smoothly
- [ ] Entity timeline shows gaps correctly
- [ ] Demo data tells coherent story
- [ ] Claude can query and analyze events
- [ ] Works on Chrome, Firefox, Safari
- [ ] Works on mobile (responsive)

### Automated Testing
- [ ] Time travel context unit tests
- [ ] Seed script generates expected data
- [ ] API as_of parameter tests
- [ ] E2E test of full demo flow

---

## Launch Checklist

### Pre-Launch
- [ ] Demo environment deployed (demo.all-source.xyz)
- [ ] Demo data seeded
- [ ] Demo API key created and published
- [ ] Claude Desktop config documented
- [ ] Demo video uploaded
- [ ] Landing page updated

### Launch Day
- [ ] Tweet demo video
- [ ] Post to Hacker News
- [ ] Share in relevant Discord/Slack communities
- [ ] Email to waitlist
- [ ] Monitor demo.all-source.xyz performance

### Post-Launch
- [ ] Monitor signup conversion
- [ ] Collect feedback
- [ ] Iterate on demo based on questions
- [ ] Create additional demo scenarios

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Demo environment goes down | Recorded video backup, can run locally |
| Claude rate limited | Pre-recorded Claude responses for high traffic |
| Demo data looks fake | Use realistic patterns, consistent story |
| Too long for attention span | Ruthlessly cut to 5 min, have 30 sec version |
| Competitors copy | Our MCP integration depth is hard to replicate |

---

## Success Criteria

| Metric | Target | Measurement |
|--------|--------|-------------|
| Demo video completion rate | >60% | YouTube analytics |
| "Try Demo" click rate | >20% | Plausible analytics |
| Demo → Signup conversion | >10% | Funnel analysis |
| Time to first event | <5 min | Backend logging |
| Support tickets about demo | <5/week | Zendesk |

---

**Owner**: Engineering + Marketing
**Timeline**: 2 weeks
**Status**: READY TO START
