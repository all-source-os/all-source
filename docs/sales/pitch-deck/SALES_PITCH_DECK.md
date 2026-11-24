# AllSource Sales Pitch Deck

## Deck Overview

**Duration**: 15-20 minutes (with Q&A)
**Audience**: CTOs, VP Engineering, Technical Decision Makers
**Goal**: Demonstrate value, build credibility, drive trial/purchase

---

## Slide 1: Title

```
┌────────────────────────────────────────────────────────────────────┐
│                                                                    │
│                           ALLSOURCE                                  │
│                                                                    │
│           AI-Native Event Sourcing Platform                        │
│                                                                    │
│       Temporal Data Intelligence at 469K events/sec                │
│                                                                    │
│                        [Company Logo]                              │
│                                                                    │
│                     all-source.xyz                                 │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

**Speaker Notes:**
- Introduce yourself and company
- Set expectation: "I'll show you how AllSource solves the event sourcing + AI challenge"

---

## Slide 2: The Problem

```
┌────────────────────────────────────────────────────────────────────┐
│                                                                    │
│             The Event Sourcing Dilemma                             │
│                                                                    │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐ │
│  │   Traditional    │  │   Build Your     │  │   Specialized    │ │
│  │   Databases      │  │      Own         │  │   Event Stores   │ │
│  ├──────────────────┤  ├──────────────────┤  ├──────────────────┤ │
│  │                  │  │                  │  │                  │ │
│  │  - Slow queries  │  │  - Custom build  │  │  - No AI support │ │
│  │  - No time-travel│  │  - Maintenance   │  │  - Single lang   │ │
│  │  - Schema fights │  │  - Security debt │  │  - Complex ops   │ │
│  │                  │  │                  │  │                  │ │
│  └──────────────────┘  └──────────────────┘  └──────────────────┘ │
│                                                                    │
│          And when AI agents need your event data?                  │
│                        Good luck.                                  │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

**Speaker Notes:**
- "Raise your hand if you've struggled with event sourcing infrastructure"
- "The AI challenge: agents can't natively query your event history"
- Build pain awareness before presenting solution

---

## Slide 3: The Solution

```
┌────────────────────────────────────────────────────────────────────┐
│                                                                    │
│                  Introducing AllSource                               │
│                                                                    │
│     The only event sourcing platform built for the AI era          │
│                                                                    │
│                                                                    │
│           ╔═══════════════════════════════════╗                    │
│           ║                                   ║                    │
│           ║   High-Performance Event Store    ║                    │
│           ║              +                    ║                    │
│           ║   Native AI Integration           ║                    │
│           ║              +                    ║                    │
│           ║   Production-Ready Security       ║                    │
│           ║                                   ║                    │
│           ╚═══════════════════════════════════╝                    │
│                                                                    │
│         469K events/sec  |  11.9μs latency  |  73 MCP tools       │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

**Speaker Notes:**
- "AllSource combines three things no one else has in one platform"
- Quick hit on headline metrics
- Transition: "Let me show you why these numbers matter"

---

## Slide 4: Performance That Scales

```
┌────────────────────────────────────────────────────────────────────┐
│                                                                    │
│                   Performance Benchmarks                           │
│                                                                    │
│     ┌────────────────────────────────────────────────────────┐    │
│     │                                                        │    │
│     │  Throughput                                            │    │
│     │  ═══════════════════════════════════════════ 469K     │    │
│     │                                                        │    │
│     └────────────────────────────────────────────────────────┘    │
│                                                                    │
│     Query Latency (p99):     11.9 μs                              │
│     Ingestion:               469K events/sec (baseline)           │
│                                                                    │
│                     Built with Rust for speed                      │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

**Speaker Notes:**
- "These aren't marketing numbers - they're from our benchmark suite"
- "Rust gives us zero-cost abstractions and memory safety"
- "Rust keeps the per-event cost of the hot path low"

---

## Slide 5: AI-Native Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│                                                                    │
│          Built for the AI Era: Model Context Protocol              │
│                                                                    │
│  ┌───────────────┐          ┌───────────────────────────────────┐ │
│  │               │   MCP    │                                   │ │
│  │    Claude     │ ◄──────► │         AllSource MCP Server        │ │
│  │    GPT-4      │  JSON    │                                   │ │
│  │    Custom     │  RPC     │  ┌─────────────────────────────┐  │ │
│  │    Agents     │          │  │     73 Tool Definitions     │  │ │
│  │               │          │  │                             │  │ │
│  └───────────────┘          │  │  • query_events             │  │ │
│                             │  │  • reconstruct_state        │  │ │
│                             │  │  • find_patterns            │  │ │
│                             │  │  • analyze_changes          │  │ │
│                             │  │  • export_events            │  │ │
│                             │  │  • semantic_search_events   │  │ │
│                             │  │  • + more...                │  │ │
│                             │  │                             │  │ │
│                             │  └─────────────────────────────┘  │ │
│                             └───────────────────────────────────┘ │
│                                                                    │
│      "What happened to order 12345?" → Instant, accurate answer   │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

**Speaker Notes:**
- "MCP is the standard for connecting AI to tools - we're the first event store to support it"
- "Your AI agents can query event history in natural language"
- "TOON output: about 50% fewer tokens than JSON for tabular data"

---

## Slide 6: Live Demo Placeholder

```
┌────────────────────────────────────────────────────────────────────┐
│                                                                    │
│                         LIVE DEMO                                  │
│                                                                    │
│                                                                    │
│                     ┌──────────────────┐                          │
│                     │                  │                          │
│                     │   [Demo Video    │                          │
│                     │    or Live       │                          │
│                     │    Terminal]     │                          │
│                     │                  │                          │
│                     └──────────────────┘                          │
│                                                                    │
│                                                                    │
│                    1. Spin up AllSource (docker compose up -d)      │
│                    2. Ingest 10K events                           │
│                    3. Query with REST API                         │
│                    4. Ask Claude about the events                  │
│                    5. Real-time WebSocket stream                   │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

**Speaker Notes:**
- Have backup video in case of technical issues
- Keep demo under 5 minutes
- Focus on "wow" moments: AI query, speed

---

## Slide 7: Architecture Overview

```
┌────────────────────────────────────────────────────────────────────┐
│                                                                    │
│         Polyglot Architecture: Best Tool for Each Job              │
│                                                                    │
│  ┌──────────────────────────────────────────────────────────────┐ │
│  │                      Web Dashboard                            │ │
│  │                   (Next.js / React 19)                        │ │
│  └──────────────────────────────────────────────────────────────┘ │
│                              │                                     │
│  ┌──────────────────────────────────────────────────────────────┐ │
│  │                      Control Plane                            │ │
│  │           (Go - Auth, RBAC, Audit, Routing)                   │ │
│  └──────────────────────────────────────────────────────────────┘ │
│                              │                                     │
│  ┌────────────┐  ┌────────────┐  ┌────────────────────┐           │
│  │   Rust     │  │  Elixir    │  │      Elixir        │           │
│  │   Core     │  │  Query     │  │    MCP Server      │           │
│  │            │  │  Service   │  │                    │           │
│  │  469K/sec  │  │ Real-time  │  │  73 tool defs      │           │
│  │  11.9μs    │  │ WebSocket  │  │    TOON format     │           │
│  └────────────┘  └────────────┘  └────────────────────┘           │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

**Speaker Notes:**
- "We chose each language for a specific reason"
- "Rust for the hot path where every microsecond counts"
- "Elixir for real-time via BEAM's fault tolerance"
- "This isn't complexity for complexity's sake - it's precision engineering"

---

## Slide 8: Enterprise Security

```
┌────────────────────────────────────────────────────────────────────┐
│                                                                    │
│                    Enterprise-Grade Security                       │
│                                                                    │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐   │
│  │  Multi-Tenancy  │  │      RBAC       │  │  Audit Logging  │   │
│  │                 │  │                 │  │                 │   │
│  │  Repository-    │  │  4 Roles        │  │  JSON-format    │   │
│  │  level          │  │  7 Permissions  │  │  Every action   │   │
│  │  isolation      │  │  Policy engine  │  │  Auth & billing │   │
│  │                 │  │                 │  │  events in Core │   │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘   │
│                                                                    │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐   │
│  │  Authentication │  │  Rate Limiting  │  │  Observability  │   │
│  │                 │  │                 │  │                 │   │
│  │  JWT tokens     │  │  Per-tenant     │  │  OpenTelemetry  │   │
│  │  API keys       │  │  Token bucket   │  │  Prometheus     │   │
│  │  OAuth/SSO      │  │  IP filtering   │  │  Grafana ready  │   │
│  │                 │  │                 │  │                 │   │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘   │
│                                                                    │
│               SOC 2 Type II certification planned                  │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

**Speaker Notes:**
- "Security isn't an afterthought - it's in the architecture"
- "Complete audit trail for compliance"
- Address common security concerns proactively

---

## Slide 9: Use Cases

```
┌────────────────────────────────────────────────────────────────────┐
│                                                                    │
│                  Who's Using AllSource?                              │
│                                                                    │
│  ┌───────────────────────────────────────────────────────────────┐│
│  │                                                               ││
│  │  FINTECH                        AI/ML TEAMS                   ││
│  │  • Transaction audit trails     • Training data pipelines     ││
│  │  • Regulatory compliance        • Feature stores              ││
│  │  • Fraud pattern detection      • Agent memory systems        ││
│  │                                                               ││
│  ├───────────────────────────────────────────────────────────────┤│
│  │                                                               ││
│  │  E-COMMERCE                     IOT / TELEMETRY               ││
│  │  • Order lifecycle tracking     • Sensor data streams         ││
│  │  • Inventory events             • Device state history        ││
│  │  • Customer journey analysis    • Anomaly detection           ││
│  │                                                               ││
│  └───────────────────────────────────────────────────────────────┘│
│                                                                    │
│        "If your application generates events, AllSource fits."       │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

**Speaker Notes:**
- Tailor this slide to your prospect's industry
- Have specific examples ready for each vertical
- Focus on the use case closest to their needs

---

## Slide 10: Customer Success / Social Proof

```
┌────────────────────────────────────────────────────────────────────┐
│                                                                    │
│                   What Developers Are Saying                       │
│                                                                    │
│                                                                    │
│   [Customer quotes: add only quotes approved for use,              │
│    with name and company.]                                         │
│   ────────────────────────────────────────────────────────────    │
│                                                                    │
│                        [GitHub Stars Badge]                        │
│                        [Community Size]                            │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

**Speaker Notes:**
- Update with real testimonials as you get them
- GitHub stars and community size build credibility
- "Join [X] companies already using AllSource"

---

## Slide 11: Pricing

```
┌────────────────────────────────────────────────────────────────────┐
│                                                                    │
│                      Simple, Scalable Pricing                      │
│                                                                    │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐   │
│  │   SELF-HOST     │  │     HOSTED      │  │   ENTERPRISE    │   │
│  │                 │  │                 │  │                 │   │
│  │     FREE        │  │  from £18.99    │  │    Custom       │   │
│  │                 │  │                 │  │                 │   │
│  │  unlimited      │  │  500K-50M/mo    │  │  negotiated     │   │
│  │  unlimited      │  │  3 to unlimited │  │  unlimited      │   │
│  │  unlimited      │  │  unlimited      │  │  unlimited      │   │
│  │  full (self)    │  │  read to r+w+ded│  │  dedicated MCP  │   │
│  │  GitHub         │  │  email to Slack │  │  24/7 + SLA     │   │
│  │                 │  │                 │  │                 │   │
│  │  [Self-host]    │  │  [Start trial]  │  │  [Contact Us]   │   │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘   │
│                                                                    │
│           Hosted trial: 14 days, 1,000 events, 1 GB outbound data        │
│                                                                    │
│                     Community edition: Apache-2.0, self-hosted. Enterprise: BSL 1.1            │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

**Speaker Notes:**
- "Start with the 14-day trial, then pick a tier"
- "Open source means no lock-in"
- Be ready to discuss enterprise pricing flexibility

---

## Slide 12: Getting Started

```
┌────────────────────────────────────────────────────────────────────┐
│                                                                    │
│                   Get Started                                      │
│                                                                    │
│                                                                    │
│     1. Clone & Run                                                 │
│        ┌─────────────────────────────────────────────────────┐    │
│        │  $ git clone https://github.com/all-source-os/all-source.git       │    │
│        │  $ docker compose up -d                              │    │
│        └─────────────────────────────────────────────────────┘    │
│                                                                    │
│     2. Ingest Your First Event                                     │
│        ┌─────────────────────────────────────────────────────┐    │
│        │  $ curl -X POST http://localhost:3900/api/v1/events │    │
│        └─────────────────────────────────────────────────────┘    │
│                                                                    │
│     3. Connect Your AI Agent                                       │
│        ┌─────────────────────────────────────────────────────┐    │
│        │  Add AllSource MCP to claude_desktop_config.json      │    │
│        └─────────────────────────────────────────────────────┘    │
│                                                                    │
│                                                                    │
│         [GitHub]     [Documentation]     [Discord Community]       │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

**Speaker Notes:**
- "It really is this simple - I can show you after"
- Lower barrier to trial as much as possible
- Offer to help them set up a POC

---

## Slide 13: Why Now?

```
┌────────────────────────────────────────────────────────────────────┐
│                                                                    │
│                     The Time is Now                                │
│                                                                    │
│                                                                    │
│   ┌─────────────────────────────────────────────────────────────┐ │
│   │                                                             │ │
│   │   AI agents are becoming central to every application       │ │
│   │                                                             │ │
│   │   Event sourcing is proven for audit, compliance, ML        │ │
│   │                                                             │ │
│   │   MCP is emerging as the standard for AI tool integration   │ │
│   │                                                             │ │
│   │   Your competitors are exploring AI-native infrastructure   │ │
│   │                                                             │ │
│   └─────────────────────────────────────────────────────────────┘ │
│                                                                    │
│                                                                    │
│       AllSource is the only platform at the intersection of          │
│       high-performance event sourcing and AI-native design.        │
│                                                                    │
│                                                                    │
│                     Don't get left behind.                         │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

**Speaker Notes:**
- Create urgency without being pushy
- Connect to broader industry trends
- "The companies building on this foundation now will have the advantage"

---

## Slide 14: Call to Action

```
┌────────────────────────────────────────────────────────────────────┐
│                                                                    │
│                      Next Steps                                    │
│                                                                    │
│                                                                    │
│                                                                    │
│          ┌─────────────────────────────────────────────┐          │
│          │                                             │          │
│          │   1. Start Free Trial Today                 │          │
│          │      all-source.xyz/trial                       │          │
│          │                                             │          │
│          │   2. Book a Technical Deep-Dive             │          │
│          │      calendly.com/allsource-demo              │          │
│          │                                             │          │
│          │   3. Join Our Community                     │          │
│          │      discord.gg/allsource                     │          │
│          │                                             │          │
│          └─────────────────────────────────────────────┘          │
│                                                                    │
│                                                                    │
│                        Questions?                                  │
│                                                                    │
│              [Your Name] | [email] | [LinkedIn]                    │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

**Speaker Notes:**
- Clear, specific next steps
- Make it easy to continue the conversation
- Offer to answer questions now

---

## Appendix Slides (for Q&A)

### A1: Detailed Benchmarks

```
Performance Details:

Source: apps/core/benches/performance_benchmarks.rs; baseline in docs/current/PERFORMANCE.md

Ingestion throughput (groups ingestion_throughput): 469,000 events/sec
Query latency (p99): 11.9 μs

Indexing: DashMap (sharded, lock-free reads)
```

### A2: Migration Path

```
Import: import_events accepts JSON, JSONL and CSV, with validation, deduplication and entity ID mapping.

No EventStoreDB or Kafka migration tooling is part of this repository.
```

### A3: Roadmap

```
See docs/roadmaps/2026-02-15_CONSOLIDATED_ROADMAP.md.
```

---

## Presentation Tips

### Before the Meeting
- Research the prospect's tech stack
- Prepare industry-specific examples
- Test demo environment
- Have backup slides/videos ready

### During the Presentation
- Ask discovery questions early
- Adapt the demo to their use case
- Watch for engagement signals
- Leave time for Q&A (20%+)

### Follow-up
- Send deck within 24 hours
- Include relevant case studies
- Offer POC assistance
- Schedule technical deep-dive

### Common Objections & Responses

| Objection | Response |
|-----------|----------|
| "We already use X" | "AllSource integrates with X, and adds AI-native capabilities" |
| "Seems complex" | "One Docker command to start. We handle the complexity." |
| "Security concerns" | Walk through enterprise security slide in detail |
| "Pricing" | No free hosted tier. The Apache-2.0 community edition is self-hosted with no license fee |
| "Support?" | Community + paid support tiers. Enterprise SLA available |
