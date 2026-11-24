# AllSource Video Content Scripts

This document contains scripts, storyboards, and recording guides for creating sales and demo videos.

---

## Video 1: Product Overview (2-3 minutes)

### Purpose
High-level introduction for landing page and social media.

### Target Audience
CTOs, Engineering Managers, AI/ML Engineers

### Script

```
[SCENE 1: Hook - 0:00-0:15]
-------------------------------------------
VISUAL: Dark screen, then code streaming effect
AUDIO: Subtle tech ambiance

NARRATOR:
"Every decision your application makes creates events.
But what if you could query any moment in time...
and let AI understand your entire event history?"

[SCENE 2: Problem Statement - 0:15-0:45]
-------------------------------------------
VISUAL: Split screen showing frustrated developer + complex architecture diagram
AUDIO: Slight tension music

NARRATOR:
"Traditional databases weren't built for event sourcing.
You're stuck with either:
- Slow queries that can't keep up
- Complex infrastructure to maintain
- Or building everything from scratch

And when AI agents need to understand your data?
Good luck with that."

[SCENE 3: Introducing AllSource - 0:45-1:30]
-------------------------------------------
VISUAL: AllSource logo animation, then architecture diagram animating in
AUDIO: Upbeat, confident music

NARRATOR:
"Meet AllSource - the AI-native event sourcing platform
built for the way modern applications work.

469,000 events per second.
11.9 microsecond p99 queries.
73 tools for AI agents, built right in.

It's not just fast. It's designed for the AI era."

[SCENE 4: Key Features - 1:30-2:15]
-------------------------------------------
VISUAL: Feature cards animating in sequence

FEATURE 1 - PERFORMANCE
"Built with Rust at its core, AllSource delivers
469K events per second with 12 microsecond latency."

FEATURE 2 - AI NATIVE
"The only event store with native Model Context Protocol support.
Your AI agents can query events in natural language."

FEATURE 3 - TIME TRAVEL
"Query any point in history instantly.
Reconstruct entity state from any timestamp."

FEATURE 4 - POLYGLOT
"Rust for speed. Go for orchestration. Elixir for real-time.
The best tool for each job."

[SCENE 5: Call to Action - 2:15-2:30]
-------------------------------------------
VISUAL: Dashboard demo, then CTA screen

NARRATOR:
"Start building with AllSource today.
Open source. Cloud ready.
Future proof."

TEXT ON SCREEN:
github.com/all-source-os/all-source
"Star on GitHub"
```

### Storyboard Frames

```
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│  Frame 1: Hook  │  │ Frame 2: Problem│  │Frame 3: Solution│
│                 │  │                 │  │                 │
│   [Code rain    │  │  [Frustrated    │  │  [AllSource logo  │
│    animation]   │  │   developer]    │  │   reveal]       │
│                 │  │                 │  │                 │
│  "What if..."   │  │ "Traditional    │  │ "Meet AllSource"  │
└─────────────────┘  │  DBs weren't..."│  └─────────────────┘
                     └─────────────────┘

┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│Frame 4: Metrics │  │Frame 5: Features│  │  Frame 6: CTA   │
│                 │  │                 │  │                 │
│    469K/sec     │  │  [Feature cards │  │  [GitHub link   │
│    11.9us       │  │   animate in]   │  │   + dashboard]  │
│    73 MCP tools │  │                 │  │                 │
│                 │  │                 │  │ "Start today"   │
└─────────────────┘  └─────────────────┘  └─────────────────┘
```

---

## Video 2: Technical Demo (5-7 minutes)

### Purpose
Detailed walkthrough for developers evaluating AllSource.

### Target Audience
Software Engineers, DevOps, Technical Architects

### Script with CLI Commands

```
[SCENE 1: Setup - 0:00-1:00]
-------------------------------------------
TERMINAL RECORDING (asciinema)

NARRATOR:
"Let's spin up AllSource locally. It's just one command."

COMMANDS:
$ git clone https://github.com/all-source-os/all-source.git
$ cd all-source
$ docker compose up -d

NARRATOR:
"The stack starts from one compose file."

COMMANDS:
$ docker compose ps

[SCENE 2: Event Ingestion - 1:00-2:30]
-------------------------------------------
TERMINAL + CODE EDITOR

NARRATOR:
"Let's ingest some events. I'll create an order tracking scenario."

COMMANDS:
$ curl -X POST http://localhost:3900/api/v1/events \
  -H "Content-Type: application/json" \
  -d '{
    "entity_id": "order-12345",
    "event_type": "OrderCreated",
    "payload": {
      "customer_id": "cust-001",
      "items": [{"sku": "ABC", "qty": 2}],
      "total": 99.99
    }
  }'

RESPONSE:
200 OK

NARRATOR:
"Event ingested. Now let's add more events to build a history."

COMMANDS (rapid fire):
# OrderPaymentReceived
# OrderShipped
# OrderDelivered

[SCENE 3: Querying Events - 2:30-3:30]
-------------------------------------------
TERMINAL

NARRATOR:
"Query by entity, type, or time range. 11.9us p99 latency."

COMMANDS:
$ curl "http://localhost:3900/api/v1/events/query?entity_id=order-12345"

# Show formatted JSON response with 4 events

$ curl "http://localhost:3900/api/v1/events/query?event_type=OrderShipped&since=2024-01-01T00:00:00Z"

NARRATOR:
"Time-travel queries let you see state at any point in history."

COMMANDS:
$ curl "http://localhost:3900/api/v1/entities/order-12345/state?as_of=2024-01-15T10:00:00Z"

[SCENE 4: AI Integration with MCP - 3:30-5:00]
-------------------------------------------
SPLIT SCREEN: Terminal + Claude Desktop

NARRATOR:
"Here's where it gets interesting. Let's ask Claude about our events."

CLAUDE DESKTOP DEMO:
User: "What happened to order 12345?"

Claude: "Let me check the event history for order-12345...

Based on the events I found:
1. Order created on Jan 10 for $99.99
2. Payment received on Jan 10
3. Shipped on Jan 12
4. Delivered on Jan 15

The order was successfully fulfilled in 5 days."

NARRATOR:
"Claude used the MCP tools to query AllSource naturally.
No custom code. No manual API calls."

User: "Were there any delays compared to similar orders?"

Claude: "Analyzing patterns across order events...
This order took 5 days from creation to delivery.
The average for similar orders is 4.2 days.
Slightly longer, but within normal range."

[SCENE 5: Real-time Subscriptions - 5:00-6:00]
-------------------------------------------
TERMINAL WITH WEBSOCKET CLIENT

NARRATOR:
"Real-time subscriptions via WebSocket."

COMMANDS:
$ wscat -c "ws://localhost:3902/ws/websocket?token=<jwt>"
> {"topic": "events:order-12345", "event": "phx_join", "payload": {}, "ref": 1}
< {"event": "phx_reply", "payload": {"status": "ok"}}

# Show events streaming in as they're ingested

[SCENE 6: Wrap Up - 6:00-6:30]
-------------------------------------------
DASHBOARD + TERMINAL SIDE BY SIDE

NARRATOR:
"That's AllSource.
High-performance event sourcing.
AI-native from day one.
Try it yourself - link in the description."
```

### CLI Recording Setup (asciinema)

```bash
# Install asciinema
brew install asciinema

# Record terminal session
asciinema rec demo.cast \
  --title "AllSource Technical Demo" \
  --idle-time-limit 2

# Convert to GIF (optional)
npm install -g svg-term-cli
svg-term --cast demo.cast --out demo.svg --window

# Or use agg for animated GIF
cargo install --git https://github.com/asciinema/agg
agg demo.cast demo.gif --theme monokai
```

---

## Video 3: MCP Integration Deep Dive (3-4 minutes)

### Purpose
Show AI/ML engineers how to integrate AllSource with LLM applications.

### Script

```
[SCENE 1: Intro - 0:00-0:30]
-------------------------------------------
VISUAL: Claude Desktop interface

NARRATOR:
"AllSource ships a native Model Context Protocol server.

Let me show you what that means for AI applications."

[SCENE 2: Setup - 0:30-1:30]
-------------------------------------------
CODE EDITOR: claude_desktop_config.json

NARRATOR:
"Configuration is simple. Add AllSource to your Claude config."

CODE:
{
  "mcpServers": {
    "allsource": {
      "command": "mix",
      "args": ["run", "--no-halt"],
      "cwd": "/path/to/allsource/mcp-server-elixir",
      "env": {
        "CORE_URL": "http://localhost:3900"
      }
    }
  }
}

NARRATOR:
"Restart Claude Desktop. The AllSource tools are available: 55 by default, 73 with system administration enabled."

[SCENE 3: Tool Showcase - 1:30-3:00]
-------------------------------------------
CLAUDE DESKTOP: Live interaction

NARRATOR (over shoulder view):

"Watch what happens when I ask about customer behavior..."

USER: "Show me the purchase patterns for customer cust-001 over the last month"

CLAUDE RESPONSE (animate typing):
"I'll analyze the events for this customer...

[Uses query_events tool]
[Uses find_patterns tool]

Customer cust-001 Purchase Analysis:
- 4 orders in the last 30 days
- Average order value: $127.50
- Preferred category: Electronics
- Typical order time: Weekday evenings
- Payment method: Always credit card

Pattern detected: Increasing order frequency
(1 order/week in Jan -> 2 orders/week in Feb)"

NARRATOR:
"Notice how Claude combined multiple tools automatically.
It queried events, found patterns, and presented insights -
all without writing a single line of code."

[SCENE 4: Management Tools - 3:00-3:30]
-------------------------------------------
TERMINAL: MCP tool list

NARRATOR:
"Beyond queries, you can manage events with natural language."

USER: "Export all orders from January as CSV"
CLAUDE: [Uses export_events tool]
"Exported 156 orders to orders_january.csv"

USER: "Archive events older than 90 days"
CLAUDE: [Uses archive_events tool]
"Archived 12,847 events to cold storage"

[SCENE 5: CTA - 3:30-4:00]
-------------------------------------------
VISUAL: Documentation page

NARRATOR:
"Full MCP documentation in the repo.
Build AI-native applications with temporal intelligence."
```

---

## Video 4: Architecture Walkthrough (4-5 minutes)

### Purpose
Technical deep-dive into the polyglot architecture for architects.

### Animated Diagram Script

```
[Animation Sequence - C4 Diagrams]

0:00 - Start with Context Diagram
       "AllSource in its ecosystem"
       Animate: Users -> System -> External Systems

0:30 - Zoom into Container Diagram
       "The polyglot services"
       Animate: Each container appearing with its language badge

1:00 - Highlight Rust Core
       "The Rust core handles the critical path"
       Animate: Show data flow through Core
       Stats overlay: 469K/sec, 11.9us

1:30 - Highlight Go Control Plane
       "Go manages auth, RBAC, and routing"
       Animate: Request flow through Control Plane
       Stats overlay: JWT, 4 roles, audit logging

2:00 - Highlight Elixir Services
       "Elixir powers real-time and AI integration"
       Animate: WebSocket streams, MCP tool calls
       Stats overlay: BEAM reliability, 73 MCP tools

2:30 - Zoom into Rust Core Components
       "Clean Architecture inside the Core"
       Animate: Domain -> Application -> Infrastructure layers

3:00 - Data Flow Animation
       "Event ingestion path"
       Animate: API -> Validation -> WAL -> Index -> Storage -> WebSocket

3:30 - Deployment Options
       "Deploy anywhere"
       Animate: Docker image -> Kubernetes manifests -> Fly.io icons (hosted)

4:00 - Summary Stats
       Animate all key metrics appearing:
       - 469K events/sec
       - 11.9us p99
       - 73 MCP tool definitions
```

### Animation Tools

```bash
# Option 1: Motion Canvas (React-based)
npx create-motion-canvas@latest allsource-architecture

# Option 2: Remotion (React video)
npx create-video@latest

# Option 3: Mermaid + CSS Animations
# Export diagrams to SVG, animate with CSS/JS
```

---

## CLI Recording Scripts (asciinema/terminalizer)

### Quick Start Recording

```bash
#!/bin/bash
# save as: record-quickstart.sh

# Start recording
asciinema rec quickstart.cast --title "AllSource Quick Start"

# Commands to execute during recording:
echo "# Clone the repository"
git clone https://github.com/all-source-os/all-source.git
cd all-source

echo "# Start the stack"
docker compose up -d

echo "# Wait for services"
sleep 5

echo "# Check status"
docker compose ps

echo "# Ingest an event"
curl -X POST http://localhost:3900/api/v1/events \
  -H "Content-Type: application/json" \
  -d '{"entity_id":"demo-1","event_type":"Created","payload":{"hello":"world"}}'

echo "# Query events"
curl http://localhost:3900/api/v1/events/query?entity_id=demo-1 | jq

echo "# Clean up"
docker compose down
```

### Performance Demo Recording

```bash
#!/bin/bash
# save as: record-performance.sh

asciinema rec performance.cast --title "AllSource Performance Demo"

# Show benchmark
echo "# Running throughput benchmark"
cargo bench --manifest-path apps/core/Cargo.toml --bench performance_benchmarks -- ingestion_throughput

echo "# Running latency benchmark"
cargo bench --manifest-path apps/core/Cargo.toml --bench performance_benchmarks -- query_performance

echo "# Results:"
echo "Throughput: 469,000 events/sec"
echo "Latency p99: 11.9 microseconds"
```

### Terminalizer Config (terminalizer.yml)

```yaml
# terminalizer config for animated GIF
command: bash
cwd: /path/to/allsource-monorepo
env:
  recording: true
cols: 120
rows: 30
repeat: 0
quality: 100
frameDelay: auto
maxIdleTime: 2000
frameBox:
  type: floating
  title: AllSource Demo
  style:
    backgroundColor: "#0a0e27"
watermark:
  imagePath: null
  style: {}
theme:
  background: "#0a0e27"
  foreground: "#f8fafc"
  cursor: "#4ecdc4"
  black: "#0a0e27"
  red: "#ce422b"
  green: "#4ecdc4"
  yellow: "#f5da81"
  blue: "#00ADD8"
  magenta: "#4E2A8E"
  cyan: "#4ecdc4"
  white: "#f8fafc"
```

---

## Video Production Checklist

### Pre-Production
- [ ] Review script accuracy against current codebase
- [ ] Test all CLI commands locally
- [ ] Prepare demo data/events
- [ ] Set up clean development environment
- [ ] Configure terminal theme (dark mode)

### Recording
- [ ] Use 1920x1080 resolution minimum
- [ ] Clear terminal history before recording
- [ ] Type at readable pace (~60 WPM)
- [ ] Pause 2 seconds after each command output
- [ ] Record multiple takes for editing

### Post-Production
- [ ] Add captions/subtitles
- [ ] Include chapter markers
- [ ] Add brand intro/outro
- [ ] Compress for web (H.264, <50MB for 5min)
- [ ] Create thumbnail (1280x720)

### Publishing
- [ ] Upload to YouTube (unlisted for review)
- [ ] Create embed code for website
- [ ] Export short clips for social media
- [ ] Add to documentation/README
