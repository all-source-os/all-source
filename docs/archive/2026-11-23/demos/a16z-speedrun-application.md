# a16z Speedrun Application — AllSource

Form URL: https://speedrun.a16z.com/apply/form
Drafted: 2026-05-13

Copy each field into the form. Placeholders in `[brackets]` need your input. Word counts are pre-checked against Speedrun's limits.

---

## 1. Team

**Are you full-time or part-time on the startup?**
Full-time

**Number of Full-Time Founders**
1

**Total FTE Employees**
1

### Founder Details — CEO

| Field | Value |
|---|---|
| First name | Decebal |
| Last name | Dobrica |
| Email | decebal@technical-leaders.com |
| Phone number | `[+44 phone number]` |
| Country | United Kingdom |
| City | London |
| Citizenship | British |
| College / University | `[your university name]` |
| Highest education | Bachelor's |
| Years of professional experience | 15 *(pick exact number in your 15–19 range)* |
| Technical enough to build end-to-end? | **Yes** |
| LinkedIn URL | `[https://www.linkedin.com/in/...]` |
| GitHub URL | `[https://github.com/...]` |
| X URL | `[https://x.com/...]` |
| Portfolio URL | https://technical-leaders.com *(or your preferred portfolio — confirm)* |

### Relevant experience *(max 100 words — current: 88)*

15+ years shipping production systems across fintech, developer infrastructure, and AI tooling. Polyglot engineer (Rust, Elixir, Go, TypeScript) with deep expertise in distributed systems, event-driven architecture, and low-latency data platforms. Led engineering teams and have personally architected and operated high-throughput services in production. Active open-source contributor in the agent and event-sourcing space — author of AllSource Core (Rust event store) and chronis (event-sourced task CLI that dogfoods AllSource). Write regularly on agent memory architectures and Rust-based AI infrastructure. Single-handedly built AllSource from event store to multi-tier agent memory engine in seven months.

### Tell us more about the team *(max 100 words — current: 99)*

Sole founder by design for this stage. AllSource Core (Rust), the Elixir Query Service, the Next.js dashboard, and Prime — the agent memory layer with knowledge graph + vector recall — are all written, shipped, and operated by one person. That's possible because the architecture is deliberately minimal: Rust event store as source of truth, thin Elixir API gateway, no Kafka, no Postgres in the event path. Informally advised by senior engineers from the AI infra and Rust ecosystems. The wedge — agent memory built on event sourcing — sits at the intersection of three things I've worked on for 15+ years.

---

## 2. Startup Details

**Startup Name**
AllSource

**Pitch your startup in one sentence** *(max 10 words — current: 9)*
The AI-native event store giving agents perfect, time-travel memory.

### Startup Description *(max 100 words — current: 99)*

AI agents have no memory between sessions, and traditional databases only store current state. Building durable, queryable agent memory today means stitching together a vector DB, a graph DB, and an event log — expensive, slow, brittle.

AllSource is the AI-native event store. Every event is durable (WAL + Parquet with CRC32 checksums) and time-travelable. The Rust core delivers 469K events/sec ingest and 12μs p99 query latency. A built-in agent memory layer (Prime) adds knowledge graph and vector recall on the same stream. 43 MCP tools let Claude or GPT manage event streams autonomously. MIT-licensed, self-hostable, with x402 micropayments for agent commerce.

**Primary Category**
Infrastructure / Dev Tools

**Secondary Category**
B2B / Enterprise Applications  *(alternative: Deep Tech — pick whichever fits your pitch better)*

**Where do you intend to build your startup?**
- Country: United Kingdom
- City: London

**When was your startup founded?**
- Year: 2025
- Month: October

**Company Website**
https://www.all-source.xyz

### Anything else we should know? *(max 100 words — current: 92)*

AllSource is already live and shipping: MIT-licensed on GitHub, free tier on www.all-source.xyz, currently on v0.19 with x402 agent payments. Built solo in seven months, ~129MB binary, 469K events/sec verified with WAL + Parquet durability. 20+ technical blog posts, three SDKs (Rust, Go, TypeScript) in production, plus chronis — an event-sourced task CLI that dogfoods the platform. Self-host or cloud paths both supported. Capital would accelerate (1) developer adoption, (2) hire 2 engineers — Rust core + DX, (3) ship the managed multi-tenant cloud. Strong fit with Speedrun's builder-heavy, infra-friendly thesis.

---

## 3. Additional Information

**Pitch Deck (PDF)** — *Optional. Upload one if you have it. If not, leave blank — the website + GitHub do a lot of the lifting here.*

**Traction** — *Optional. Suggested points to list:*
- v0.19 shipped publicly; MIT-licensed open source on GitHub (`all-source-os/all-source`)
- 469K events/sec, 11.9μs p99 query latency benchmarked
- Free tier live with self-serve OAuth signup
- 3 production SDKs (Rust, Go, TypeScript) + Python client
- 43 MCP tools shipped for Claude / GPT autonomous workflows
- *Add: GitHub stars, signups, paying customers, design partners if you have numbers*

**Funding History** — *Optional. If bootstrapped to date, state that explicitly.* Suggested: "Bootstrapped to date. No external funding."

**Active Fundraising Round** — *Optional. Suggested if applicable:* "Raising pre-seed / Speedrun cheque to fund first two engineering hires and the managed cloud GA."

**Referral** — *Optional. Leave blank unless someone specific referred you.*

### Where did you learn about Speedrun?

**Source:** a16z newsletter / blog
**Additional info:** *(leave blank, or write "Speedrun Substack" if that's where you saw it)*

---

## Checklist before you hit submit

- [ ] Fill the 5 placeholder fields (phone, university, LinkedIn, GitHub, X)
- [ ] Confirm portfolio URL (technical-leaders.com vs another)
- [ ] Decide secondary category — B2B/Enterprise vs Deep Tech
- [ ] Decide whether to upload a deck or skip
- [ ] Add concrete traction numbers if you have them (signups, stars, paying users)
- [ ] Re-read the 100-word answers in your own voice — tweak any phrasing that doesn't sound like you
