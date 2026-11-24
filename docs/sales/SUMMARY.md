# AllSource SaaS Evaluation Summary

**Date**: February 11, 2026
**Version**: 0.30.0
**Status**: Ready for Beta Launch

---

## Executive Summary

AllSource is an **AI-native event sourcing platform** positioned for the emerging market of temporal data intelligence. This evaluation assessed its readiness for SaaS release.

### Verdict: Ready for Beta Launch (Score: 8.4/10)

| Criterion | Score | Notes |
|-----------|-------|-------|
| Technical Maturity | 8/10 | Durable event store (WAL, Parquet); tests per service |
| Market Fit | 9/10 | Strong alignment with AI/ML trends |
| Differentiation | 9/10 | Only event store with native MCP |
| Scalability | 8/10 | Cloud-native, K8s-ready |
| Security | 8/10 | Multi-tenancy, RBAC, audit logging |

---

## Key Value Propositions

| Message | Data |
|---------|------|
| AI-Native First | 73 MCP tool definitions (55 by default), native MCP server |
| Performance | 469K events/sec, 11.9μs p99 latency |
| Polyglot Architecture | Rust + Go + Elixir (best tool for each job) |
| Self-Host | Apache-2.0 community edition, `docker compose up` |
| Enterprise Security | Multi-tenancy, RBAC, JWT, audit logging |
| Time-Travel | Query any point in history instantly |

---

## Target Markets

### Primary
- **AI/ML Teams** - Training pipelines, feature stores
- **Fintech** - Audit trails, compliance
- **IoT/Telemetry** - High-throughput sensor data
- **E-commerce** - Order tracking, inventory events

### Secondary
- Gaming (player history, anti-cheat)
- Healthcare (patient timelines, compliance)
- Supply Chain (tracking, provenance)

---

## Competitive Advantage

| Competitor | AllSource Advantage |
|------------|-------------------|
| EventStoreDB | 73 MCP tool definitions, polyglot architecture |
| Kafka | Purpose-built for events, simpler ops |
| Marten | Core is the event store; no PostgreSQL dependency |
| Custom Solutions | Turnkey SaaS, no maintenance |

---

## Pricing (current)

Source: `apps/web/src/lib/config.ts` (`siteConfig.pricing`). Live prices come from LemonSqueezy and override the config prices.

```
Tier        Price (GBP/month)   Events/month   Streams    MCP access
Self-Host   Free (own infra)    unlimited      unlimited  full (self-host)
Indie       £18.99              500K           3          hosted read
Studio      £78.99              5M             unlimited  hosted read + write
Scale       £298.99             50M            unlimited  hosted read + write + dedicated
Enterprise  Custom              negotiated     unlimited  dedicated MCP cluster
```

- No free hosted tier. Hosted trial: 14 days, 1,000 events, 1 GB outbound data.
- Yearly prices: Indie £15.17/mo, Studio £63.17/mo, Scale £239.17/mo.
- Support: Indie email (48h); Studio email (24h) and Discord; Scale priority and Slack; Enterprise 24/7 with dedicated SE and SLA.

---

## Go-to-Market Strategy

### Phase 1: Developer Adoption (Months 1-3)
- Goal: 1,000 free-tier users
- Channels: GitHub, Hacker News, Reddit, Dev.to

### Phase 2: Conversion (Months 4-6)
- Goal: 100 paying customers
- Channels: Product Hunt, LinkedIn, Webinars

### Phase 3: Scale (Months 7-12)
- Goal: $100K ARR
- Channels: Enterprise sales, Conferences, Analysts

---

## Financial Projections

| Month | Free Users | Paid Users | MRR |
|-------|------------|------------|-----|
| 1-3 | 500 | 10 | $990 |
| 4-6 | 2,000 | 50 | $4,950 |
| 7-9 | 5,000 | 150 | $14,850 |
| 10-12 | 10,000 | 350 | $34,650 |

**Year 1 ARR**: ~$165K
**Break-even**: Month 7-8

---

## Required for Launch

### P0 (Must Have)
- [ ] Self-service signup flow
- [ ] Usage metering and billing (LemonSqueezy)
- [ ] SLA monitoring and alerting
- [ ] Customer dashboard for metrics
- [ ] Privacy policy and ToS

### P1 (Should Have)
- [ ] SOC 2 Type II audit
- [ ] GDPR compliance tools
- [ ] Managed backup/restore UI
- [ ] Team management features

### P2 (Nice to Have)
- [ ] White-label options
- [ ] Custom MCP tool builder
- [ ] GraphQL API
- [ ] Mobile SDK

---

## Artifacts Created

| Artifact | Location | Purpose |
|----------|----------|---------|
| SaaS Evaluation | `evaluation/SAAS_EVALUATION.md` | Strategic planning |
| C4 Diagrams (Mermaid) | `diagrams/C4_ARCHITECTURE_MERMAID.md` | Technical presentations |
| C4 Diagrams (Structurizr) | `diagrams/allsource.dsl` | Architecture tooling |
| Visual Assets | `pitch-deck/VISUAL_ASSETS.md` | SVG graphics, charts |
| Video Scripts | `videos/VIDEO_SCRIPTS.md` | Demo recordings |
| Sales Pitch Deck | `pitch-deck/SALES_PITCH_DECK.md` | Customer presentations |

---

## Next Steps

1. Review P0 requirements and prioritize implementation
2. Confirm the LemonSqueezy catalog and plan prices
3. Create landing page with signup flow
4. Record demo videos using provided scripts
5. Launch beta program with 50 hand-picked users
6. Prepare for Product Hunt / Hacker News launch

---

*Generated: February 11, 2026*
