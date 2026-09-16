---
title: "AllSource Documentation"
status: CURRENT
last_updated: 2026-02-08
---

# AllSource Documentation Hub

Welcome to the AllSource documentation. This hub provides comprehensive guides, architecture documentation, and operational resources for the AllSource event sourcing platform.

---

## Quick Navigation

| | Link | Description |
|---|------|-------------|
| 🚀 | [Quick Start](./guides/QUICK_START.md) | Get up and running in minutes |
| 📖 | [Architecture](./current/CLEAN_ARCHITECTURE.md) | Clean architecture principles |
| ⚡ | [Performance](./current/PERFORMANCE.md) | Optimization strategies |
| 🔒 | [Security](./current/TENANT_ARCHITECTURE.md) | Multi-tenant security model |
| 🛠️ | [Quality Gates](./current/QUALITY_GATES.md) | CI/CD quality standards |
| 🗺️ | [Roadmap](./roadmaps/) | Future plans and progress |

---

## What's New

**February 2026**
- Updated documentation hub with modern navigation
- Consolidated quality gates documentation

**November 2025**
- Added multi-tenant architecture security documentation
- Quality gates implementation and enforcement
- Critical bug fixes documented

**October 2025**
- Clean Architecture refactoring complete (Phase 1.5)
- Performance optimization guide published
- Query Service roadmap established

---

## Status Legend

| Marker | Status | Meaning |
|--------|--------|---------|
| ✅ | CURRENT | Active, up-to-date documentation |
| ⚠️ | NEEDS_REVIEW | May be outdated, review before use |
| 📦 | ARCHIVED | Historical reference, superseded by newer docs |

---

## Documentation by Audience

### Getting Started

For new users looking to understand and run AllSource.

| Document | Description | Status |
|----------|-------------|--------|
| [Quick Start](./guides/QUICK_START.md) | Installation and first run | ✅ |
| [Demo Walkthrough](./guides/DEMO.md) | Hands-on demonstration | ✅ |
| [MCP Server Setup](./guides/mcp-server/CLAUDE_DESKTOP_SETUP.md) | Claude Desktop integration | ✅ |

---

### Core Concepts

For understanding the system architecture and design decisions.

| Document | Description | Status |
|----------|-------------|--------|
| [Clean Architecture](./current/CLEAN_ARCHITECTURE.md) | Layered architecture guide | ✅ |
| [SOLID Principles](./current/SOLID_PRINCIPLES.md) | Design principles applied | ✅ |
| [Tenant Architecture](./current/TENANT_ARCHITECTURE.md) | Multi-tenancy and isolation | ✅ |
| [Event Store Features](./current/EVENT_STORE_FEATURES.md) | Core event sourcing capabilities | ✅ |
| [Architecture Optimization](./current/ARCHITECTURE_OPTIMIZATION.md) | System-level optimizations | ✅ |

---

### Guides

How-to documentation for common tasks.

| Document | Description | Status |
|----------|-------------|--------|
| [WebSocket Configuration](./guides/WEBSOCKET_CONFIGURATION.md) | Real-time event streaming setup | ✅ |
| [Release Guide](./guides/RELEASE.md) | Create releases with `make release` | ✅ |
| [Quality Gates Setup](./guides/QUALITY_GATES_SETUP.md) | Configure CI/CD quality checks | ✅ |
| [Branch Protection](./guides/BRANCH_PROTECTION.md) | Git workflow protection | ✅ |
| [MCP Demo Script](./guides/mcp-server/DEMO_SCRIPT.md) | MCP demonstration walkthrough | ✅ |
| [MCP Quick Reference](./guides/mcp-server/QUICK_REFERENCE.md) | MCP tool reference card | ✅ |
| [Claude Code Skills](./guides/CLAUDE_CODE_SKILLS.md) | Automated testing, release, and diagnostics skills | ✅ |
| [Embedded Prime Integration](./guides/EMBEDDED_PRIME_INTEGRATION.md) | Use Prime as an embedded library in Tauri / Rust apps — graph, HNSW vectors, tiered recall | ✅ |

---

### API Reference

Developer documentation for service integration.

| Service | README | Details |
|---------|--------|---------|
| Rust Core | [apps/core](../apps/core/README.md) | Event sourcing engine |
| Control Plane | [apps/control-plane](../apps/control-plane/README.md) | Cluster management (Go) |
| Query Service | [apps/query-service](../apps/query-service/README.md) | Read-side queries (Elixir) |
| MCP Server | [apps/mcp-server](../apps/mcp-server/README.md) | Model Context Protocol |
| Web App | [apps/web](../apps/web/README.md) | Web interface |

---

### Operations

For DevOps and platform engineers.

| Document | Description | Status |
|----------|-------------|--------|
| [Alerts](./operations/ALERTS.md) | Repo-defined alarms, their thresholds and escalation | ✅ |
| [Docker Deployment](./deployment/DOCKER.md) | Container images and configuration | ✅ |
| [Quality Gates](./current/QUALITY_GATES.md) | CI/CD enforcement standards | ✅ |
| [Performance Guide](./current/PERFORMANCE.md) | Optimization and tuning | ✅ |
| [Docker Images](./docker-images.md) | Container configuration | ✅ |
| [Troubleshooting](./guides/TROUBLESHOOTING.md) | Common issues and solutions | ✅ |
| [Test Coverage Plan](./testing/control-plane-coverage-plan.md) | Control plane testing | ✅ |

---

## Roadmaps & Planning

| Document | Description | Status |
|----------|-------------|--------|
| [Comprehensive Roadmap](./roadmaps/2025-10-22_COMPREHENSIVE_ROADMAP.md) | v1.0 to v2.0 vision | ✅ |
| [Query Service Roadmap](./roadmaps/query-service-roadmap.md) | Query service evolution | ✅ |
| [MCP v2 Enhancements](./roadmaps/mcp-v2-enhancements.md) | MCP protocol improvements | ✅ |
| [Vector Embedding Design](./roadmaps/FUTURE_VECTOR_EMBEDDING_DESIGN.md) | Future AI/ML integration | ✅ |
| [LanceDB Comparison](./roadmaps/ALLSOURCE_VS_LANCEDB_COMPARISON.md) | Technology comparison | ✅ |

---

## Archive Index

Historical documentation preserved for reference. Each archived document notes what superseded it.

### Supersession Reference

| Archived Document | Superseded By | Date |
|-------------------|---------------|------|
| `archive/2025-10-22_CLEAN_ARCHITECTURE_FULL.md` | [current/CLEAN_ARCHITECTURE.md](./current/CLEAN_ARCHITECTURE.md) | 2025-10-22 |
| `archive/2025-10-22_SOLID_PRINCIPLES_FULL.md` | [current/SOLID_PRINCIPLES.md](./current/SOLID_PRINCIPLES.md) | 2025-10-22 |
| `archive/2025-10-22_PERFORMANCE_FULL.md` | [current/PERFORMANCE.md](./current/PERFORMANCE.md) | 2025-10-22 |
| `archive/2025-11-04/ARCHITECTURE_OPTIMIZATION_v1.md` | [current/ARCHITECTURE_OPTIMIZATION.md](./current/ARCHITECTURE_OPTIMIZATION.md) | 2025-11-04 |
| `archive/2025-11-04/QUERY_SERVICE_ROADMAP_v1.md` | [roadmaps/query-service-roadmap.md](./roadmaps/query-service-roadmap.md) | 2025-11-04 |
| `archive/2025-10-21_ROADMAP.md` | [roadmaps/2025-10-22_COMPREHENSIVE_ROADMAP.md](./roadmaps/2025-10-22_COMPREHENSIVE_ROADMAP.md) | 2025-10-22 |

### Archive Directories

| Directory | Contents |
|-----------|----------|
| [archive/apps-core-phases/](./archive/apps-core-phases/) | Core app refactoring phases (Phase 1-5) |
| [archive/2025-11-04/](./archive/2025-11-04/) | November 2025 refactoring documentation |
| [archive/2025-11-03-marketing/](./archive/2025-11-03-marketing/) | Marketing materials drafts |
| [archive/2024-11-03/](./archive/2024-11-03/) | Migration documentation |

---

## Directory Structure

```
docs/
├── current/           # ✅ Active documentation
├── archive/           # 📦 Historical documents
├── guides/            # How-to guides
│   └── mcp-server/   # MCP-specific guides
├── roadmaps/          # Planning and vision
├── operations/        # Operational guides
├── testing/           # Test plans and coverage
├── marketing/         # Marketing materials
└── x402/              # X402 protocol documentation
```

---

## Documentation Conventions

### File Naming
- Timestamped: `YYYY-MM-DD_FILENAME.md`
- Current: Descriptive name without timestamp

### Status Markers
All documents should include a status in their frontmatter or header:
- `status: CURRENT` - Actively maintained
- `status: NEEDS_REVIEW` - May need updates
- `status: ARCHIVED` - Historical only

### Linking
Use relative paths for internal links:
```markdown
[Architecture Guide](./current/CLEAN_ARCHITECTURE.md)
[Core README](../apps/core/README.md)
```

---

## Contributing

1. **Creating**: Place new docs in the appropriate directory and update this index
2. **Updating**: Update the `last_updated` date in frontmatter
3. **Archiving**: Move to `archive/` with timestamp, update supersession reference above
4. **Questions**: Create an issue with `[docs]` prefix

---

**Navigation**: [Repository Home](../README.md) | [Current Docs](./current/) | [Guides](./guides/) | [Roadmaps](./roadmaps/) | [Archive](./archive/)
