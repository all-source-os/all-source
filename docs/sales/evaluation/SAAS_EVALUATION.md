# AllSource SaaS Evaluation Report

**Date**: February 2026
**Version**: 0.30.0
**Status**: Pre-Launch Assessment

---

## Executive Summary

AllSource is an **AI-native event sourcing platform** positioned for the emerging market of temporal data intelligence. This evaluation assesses its readiness for SaaS release, market positioning, and go-to-market strategy.

### Verdict: **Ready for Beta Launch**

| Criterion | Score | Notes |
|-----------|-------|-------|
| Technical Maturity | 8/10 | Durable event store (WAL, Parquet); tests per service |
| Market Fit | 9/10 | Strong alignment with AI/ML infrastructure trends |
| Differentiation | 9/10 | Unique AI-native + polyglot architecture |
| Scalability | 8/10 | Cloud-native, K8s-ready |
| Security | 8/10 | Multi-tenancy, RBAC, audit logging |

---

## 1. Product-Market Fit Analysis

### Target Market Segments

#### Primary Markets

| Segment | Pain Point | AllSource Solution |
|---------|-----------|------------------|
| **AI/ML Teams** | Training data pipelines, feature stores | MCP server, time-travel queries |
| **Fintech** | Audit trails, transaction history | Immutable events, compliance |
| **IoT/Telemetry** | High-throughput sensor data | 469K events/sec, 11.9us p99 query latency |
| **E-commerce** | Order tracking, inventory events | Event sourcing, projections |

#### Secondary Markets

- **Gaming**: Player action history, anti-cheat systems
- **Healthcare**: Patient event timelines, audit compliance
- **Supply Chain**: Shipment tracking, provenance

### Competitive Landscape

| Competitor | Strengths | Weaknesses | AllSource Advantage |
|------------|-----------|------------|-------------------|
| **EventStoreDB** | Mature, community | No AI integration, single language | 73 MCP tool definitions, polyglot |
| **Apache Kafka** | Ubiquitous, ecosystem | Not event-sourced, complex ops | Purpose-built, simple deploy |
| **Axon Server** | Java ecosystem | Java-only, enterprise pricing | Polyglot, open-source first |
| **Marten (PostgreSQL)** | Familiar stack | Requires PostgreSQL | Core is the event store; no PostgreSQL dependency |
| **Custom Solutions** | Tailored fit | Maintenance burden | Turnkey SaaS |

### Unique Value Propositions

1. **AI-Native First**: Native MCP integration (73 tool definitions)
2. **Performance**: 469K events/sec, 11.9us p99 latency
3. **Polyglot Architecture**: Best-in-class languages (Rust + Go + Elixir)
4. **Self-host**: Apache-2.0 community edition, started with `docker compose up`
5. **Time-Travel**: Query any point in history instantly
6. **Open Source Core**: Apache-2.0 community edition; enterprise features under BSL 1.1

---

## 2. Technical Readiness Assessment

### Architecture Strengths

```
Grade: A-

+ Clean Architecture layering (domain, application, infrastructure)
+ Cloud-native design (K8s, Helm, Cloud Run ready)
+ Modern tech stack (Rust 1.92, Go 1.26.11, Elixir 1.17)
+ Security-first (RBAC, JWT, audit logging, multi-tenancy)

- Horizontal scaling needs validation at >1M events/sec
```

### Performance Benchmarks

| Metric | Current (source) |
|--------|------------------|
| Ingestion | 469K events/sec (docs/current/PERFORMANCE.md) |
| Query latency (p99) | 11.9us (docs/current/PERFORMANCE.md) |
| Storage | Parquet columnar, Snappy compression |

### Security Checklist

- [x] Multi-tenancy with repository isolation
- [x] JWT + API key authentication
- [x] Role-based access control (4 roles, 7 permissions)
- [x] Comprehensive audit logging
- [x] Rate limiting per tenant
- [x] IP allowlist/blocklist
- [ ] SOC 2 compliance (planned)
- [ ] GDPR data deletion workflows (partial)
- [ ] Encryption at rest (planned)

---

## 3. Pricing Model Recommendations

### Current tiers (apps/web/src/lib/config.ts)

Live prices come from LemonSqueezy and override the config prices.

```
                    SELF-HOST       INDIE           STUDIO          SCALE           ENTERPRISE
                    ---------       -----           ------          -----           ----------
Price (GBP/month)   Free            £18.99          £78.99          £298.99         Custom
Events/month        unlimited       500K            5M              50M             negotiated
Streams             unlimited       3               unlimited       unlimited       unlimited
MCP access          full (self)     hosted read     read + write    read + write    dedicated cluster
Support             GitHub          email (48h)     email + Discord email + Slack   24/7 + SLA
```

No free hosted tier. Hosted trial: 14 days, 1,000 events, 1 GB outbound data.

### Alternative Models Considered

| Model | Pros | Cons | Recommendation |
|-------|------|------|----------------|
| **Per-seat** | Predictable | Discourages adoption | Not recommended |
| **Per-event** | Pure usage-based | Unpredictable bills | Hybrid approach |
| **Flat tier** | Simple | Leaves money on table | Use for Enterprise |
| **Usage + tier** | Balanced | Slightly complex | **Recommended** |

### Key Pricing Insights

1. **Free tier**: the self-hosted Community edition is free; hosted plans have a 14-day trial and no free tier
2. **Event volume** is natural scaling metric
3. **MCP tools** are premium differentiator
4. **Enterprise** needs custom pricing flexibility

---

## 4. Go-to-Market Strategy

### Phase 1: Developer Adoption (Months 1-3)

**Goal**: 1,000 active free-tier users

| Channel | Action | Target |
|---------|--------|--------|
| GitHub | Star campaign, README optimization | 500 stars |
| Hacker News | Launch post, Show HN | 200+ points |
| Reddit | r/rust, r/eventdriven, r/devops | 50+ saves |
| Dev.to/Medium | Technical deep-dives | 10K+ reads |
| X/Twitter | #BuildInPublic journey | 2K followers |

### Phase 2: Conversion (Months 4-6)

**Goal**: 100 paying customers

| Channel | Action | Target |
|---------|--------|--------|
| Product Hunt | Featured launch | Top 5 of day |
| LinkedIn | B2B content, case studies | 5K impressions/post |
| Partnerships | AI tool integrations | 3 partnerships |
| Webinars | "Event Sourcing for AI" series | 500 registrations |

### Phase 3: Scale (Months 7-12)

**Goal**: $100K ARR

| Channel | Action | Target |
|---------|--------|--------|
| Sales team | Outbound to enterprises | 10 enterprise deals |
| Content | SEO-driven blog | 50K organic visits/mo |
| Conferences | RustConf, AI Summit | 2 speaking slots |
| Analyst | Gartner/Forrester briefings | 1 mention |

---

## 5. Risk Assessment

### Technical Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Scaling beyond 1M events/sec | Medium | High | Horizontal sharding design ready |
| Data loss | Low | Critical | WAL + Parquet persistence, crash recovery |
| Security breach | Low | Critical | Audit logging, RBAC, penetration testing |

### Business Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Low adoption | Medium | High | Self-host Community edition, docs, community |
| Competitor response | Medium | Medium | Speed, AI-native differentiation |
| Pricing wrong | Medium | Medium | A/B testing, customer feedback |

### Operational Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Support overwhelm | High | Medium | Self-service docs, community forums |
| Infrastructure costs | Medium | Medium | Efficient architecture, reserved capacity |

---

## 6. Required Improvements for Launch

### Must Have (P0)

- [ ] Self-service signup flow
- [ ] Usage metering and billing integration
- [ ] SLA monitoring and alerting
- [ ] Customer dashboard for metrics
- [ ] Privacy policy and ToS

### Should Have (P1)

- [ ] SOC 2 Type II audit
- [ ] GDPR compliance tools
- [ ] Managed backup/restore UI
- [ ] Team management features

### Nice to Have (P2)

- [ ] White-label options
- [ ] Custom MCP tool builder
- [ ] GraphQL API
- [ ] Mobile SDK

---

## 7. Financial Projections

### Year 1 Revenue Model

| Month | Free Users | Paid Users | MRR |
|-------|------------|------------|-----|
| 1-3 | 500 | 10 | $990 |
| 4-6 | 2,000 | 50 | $4,950 |
| 7-9 | 5,000 | 150 | $14,850 |
| 10-12 | 10,000 | 350 | $34,650 |

**Year 1 Total**: ~$165K ARR

### Cost Structure

| Category | Monthly | Notes |
|----------|---------|-------|
| Infrastructure | $2,000-10,000 | Scales with usage |
| Support | $5,000 | 1 FTE initially |
| Marketing | $3,000 | Content + ads |
| Tools/SaaS | $500 | Monitoring, billing |
| **Total** | $10,500-18,500 | |

### Break-even Analysis

- **Break-even MRR**: ~$15,000
- **Expected timeline**: Month 7-8
- **Path to profitability**: Month 10+

---

## 8. Recommendations

### Immediate Actions (Next 30 Days)

1. **Create landing page** with clear value prop and signup
2. **Confirm LemonSqueezy** plans and prices (billing is implemented in the control plane)
3. **Launch beta program** with 50 hand-picked users
4. **Prepare launch content** (blog, social, video)

### Short-term (60-90 Days)

1. **Public launch** on Product Hunt and Hacker News
2. **First case study** from beta users
3. **API documentation** polish
4. **Support system** (Intercom/Discord)

### Medium-term (6 Months)

1. **Enterprise sales** outreach
2. **SOC 2 certification**
3. **Partnership program**
4. **Series A preparation** (if venture path)

---

## Appendix

### A. Key Metrics to Track

```
Acquisition:
- Website visitors
- GitHub stars
- Trial signups
- Documentation page views

Activation:
- First event ingested
- First query executed
- MCP tool usage
- Time to first value

Retention:
- DAU/MAU ratio
- Event volume trends
- Feature adoption
- Support ticket volume

Revenue:
- MRR/ARR
- ARPU
- Churn rate
- LTV:CAC ratio
```

### B. Competitive Positioning Matrix

```
                    Performance
                         ^
                         |
    [AllSource] *          |           * [Custom]
                         |
    -------------------- + --------------------> AI Integration
                         |
           * [Kafka]     |    * [EventStoreDB]
                         |
           * [Marten]    |
```

### C. Technology Differentiation

| Feature | AllSource | Others |
|---------|---------|--------|
| MCP Protocol | Native (73 tool definitions) | None |
| Time-travel | Built-in | Limited |
| Polyglot | Rust+Go+Elixir | Usually single |
| Query latency (p99) | 11.9us | not measured in this repo |

---

*This evaluation was prepared for internal planning purposes. Metrics and projections are estimates based on current market conditions.*
