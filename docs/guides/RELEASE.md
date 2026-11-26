# Release Guide

This guide covers the release workflow for the AllSource monorepo.

## Quick Reference

```bash
make release           # Full interactive release workflow
make release-quick     # Quick patch release (skip quality gates)
make release-preflight # Run pre-flight checks only
make version           # Show current version and recent tags
make images-check      # Check Docker image versions in GHCR
```

## Prerequisites

Before creating a release, ensure you have:

1. **Clean working tree**: No uncommitted changes
2. **GitHub CLI authenticated**: Run `gh auth login` if needed
3. **On main branch**: Releases should be created from main
4. **Remote accessible**: Can push to origin

The `make release-preflight` command checks all of these.

## Full Release Workflow

### 1. Start the Release

```bash
make release
```

### 2. Pre-flight Checks

The workflow automatically verifies:
- No uncommitted changes
- On main branch (or prompts for confirmation)
- Git remote is accessible
- GitHub CLI is authenticated

### 3. Version Selection

You'll be prompted to select a version:

```
Current version: v0.8.1

Version suggestions:
  1) v0.8.2 (patch - bug fixes)
  2) v0.9.0 (minor - new features)
  3) v1.0.0 (major - breaking changes)
  4) Custom version

Select version type (1-4) [1]:
```

**Version Guidelines:**
- **Patch (x.y.Z)**: Bug fixes, security patches, documentation updates
- **Minor (x.Y.0)**: New features, non-breaking changes
- **Major (X.0.0)**: Breaking changes, major rewrites

### 4. Release Title

Enter a descriptive title for the release:

```
Release title (e.g., 'Quality & Stability Release'):
```

### 5. Quality Gates (Optional)

You'll be asked whether to run quality gates:

```
Run quality gates before release? (Y/n):
```

This runs `make ci` which includes:
- Rust: formatting, Clippy, tests, docs
- Go: formatting, linting, tests
- Elixir: formatting, Credo, Dialyzer, tests

**Recommendation**: Always run quality gates for minor/major releases.

### 6. Documentation Updates

The workflow automatically updates:
- `README.md`: Version badges for Docker images
- `RELEASE.md`: Version number and release date

### 7. Git Tag Creation

An annotated tag is created with:
- Version number
- Release title
- Recent commit history

### 8. Push to Remote

The workflow pushes:
- Updated documentation to main branch
- New version tag (triggers docker-publish workflow)

### 9. GitHub Release

A GitHub release is created with:
- Release title
- Changelog (recent commits)
- Docker pull commands

### 10. Docker Image Publishing

The tag push triggers the `docker-publish` workflow which:
- Builds multi-arch images (amd64 + arm64)
- Pushes to GitHub Container Registry (ghcr.io)
- Tags images with version (e.g., `v0.8.2`, `0.8`, `latest`)

You can optionally wait for the workflow to complete.

## Quick Patch Release

For simple bug fixes, use the quick release:

```bash
make release-quick
```

This skips quality gates and uses defaults:
- Automatically increments patch version
- Uses "Patch Release" as default title
- Auto-generates release notes

## Post-Release Checklist

After a release:

1. **Verify Docker Images**
   ```bash
   make images-check
   ```
   Or visit: https://github.com/all-source-os/allsource-monorepo/pkgs

2. **Verify GitHub Release**
   Visit: https://github.com/all-source-os/allsource-monorepo/releases

3. **Update Release Branch** (optional)
   ```bash
   git checkout release/0.8
   git merge main
   git push origin release/0.8
   git checkout main
   ```

4. **Announce the Release**
   - Update project documentation
   - Notify stakeholders
   - Update changelog in external docs

## Troubleshooting

### Pre-flight Check Failures

**Uncommitted changes:**
```bash
git status
git stash  # or commit changes
make release
```

**Not on main branch:**
```bash
git checkout main
git pull origin main
make release
```

**GitHub CLI not authenticated:**
```bash
gh auth login
```

### Docker Publish Workflow Issues

If images aren't being tagged correctly:

1. Check workflow status:
   ```bash
   gh run list --workflow=docker-publish.yml --limit 5
   ```

2. View workflow logs:
   ```bash
   gh run view <run-id> --log
   ```

3. Manually trigger with force build:
   ```bash
   gh workflow run docker-publish.yml -f force_build=true
   ```

### Tag Already Exists

If you need to recreate a tag:

```bash
# Delete local tag
git tag -d v0.8.2

# Delete remote tag
git push origin :refs/tags/v0.8.2

# Recreate
make release
```

**Warning**: Only do this if the release hasn't been announced/used.

## Version Locations

All files that contain version numbers and must be updated for each release:

### Service Versions (checked by `scripts/check-versions.sh`)

| File | Field | Description |
|------|-------|-------------|
| `apps/core/Cargo.toml` | `version = "X.Y.Z"` | Rust Core crate version |
| `apps/control-plane/main.go` | `Version = "X.Y.Z"` | Go control plane constant |
| `apps/control-plane/tracing.go` | `serviceVersion = "X.Y.Z"` | OpenTelemetry service version |
| `apps/query-service/mix.exs` | `version: "X.Y.Z"` | Query Service Elixir version |
| `apps/mcp-server-elixir/mix.exs` | `version: "X.Y.Z"` | MCP Server Elixir version |
| `deploy/k8s/core.yaml` | `image: allsource/core:X.Y.Z` | K8s Core image tag |
| `deploy/k8s/query-service.yaml` | `image: allsource/query-service:X.Y.Z` | K8s Query Service image tag |
| `apps/prime-mcp/Cargo.toml` | `version = "X.Y.Z"`, `allsource-core` dependency | Prime MCP server |
| `tooling/recall-bench/Cargo.toml` | `allsource-core` dependency | Recall benchmark |
| `apps/chronis/Cargo.toml` | `allsource-core` dependency | Chronis CLI |
| `tooling/allsource-mcp/Cargo.toml` | `allsource-core` dependency | allsource-mcp server |
| `apps/control-plane/docs/openapi.yaml` | `info.version` | Control Plane OpenAPI spec |

### Documentation and UI (manual update)

| File | Field | Description |
|------|-------|-------------|
| `README.md` | Frontmatter `version`, badges, docker pull commands, version table, roadmap | Main repository README |
| `apps/mcp-server-elixir/README.md` | Frontmatter `version` | MCP Server README |
| `apps/web/src/components/sections/hero.tsx` | Version badge text | Web dashboard hero section |
| `docs/launch/MARKETING_MATERIALS.md` | `**Version:**` header | Marketing docs |
| `docs/launch/LAUNCH_READINESS_ASSESSMENT.md` | `**Version:**` header | Launch readiness |
| `docs/sales/evaluation/SAAS_EVALUATION.md` | `**Version**:` header | Sales evaluation |
| `docs/sales/SUMMARY.md` | `**Version**:` header | Sales summary |

### Quick Update Command

```bash
# Verify current state
./scripts/check-versions.sh

# Search for remaining old version references
grep -rn "0\.9\.0\|0\.9\.1" --include="*.md" --include="*.tsx" --include="*.go" --include="*.toml" --include="*.exs" --include="*.yaml" --exclude-dir=node_modules --exclude-dir=target --exclude-dir=_build --exclude-dir=deps --exclude-dir=.git
```

## Version History

Current crate and service version: 0.30.1 (`apps/core/Cargo.toml:3`).

| Version | Date | Highlights |
|---------|------|------------|
| v0.10.3 | 2026-02-16 | Critical QS proxy fix, domain migration chronos→allsource |
| v0.10.0 | 2026-02-13 | Leader-follower replication, event-sourced metadata, control plane v2 |
| v0.9.1 | 2026-02-12 | Stream & event type discovery APIs |
| v0.9.0 | 2026-02-10 | Dashboard, auth & logger fixes |
| v0.8.1 | 2026-02-08 | SIMD filter refactoring, Elixir test fixes |
| v0.8.0 | 2026-02-03 | Clean Architecture release |
| v0.7.3 | 2026-02-02 | Quality improvements |

---

### v0.10.3 — Critical QS Proxy Fix (2026-02-16)

**Fixed**: All tenant-scoped Query Service routes now work. They were returning 500 in v0.10.1 due to a string-vs-atom key mismatch in the tenant context.

**Verified working endpoints:**

| Endpoint | Use Case |
|----------|----------|
| `GET /api/events/entity/:entity_id` | Load entity stream (index, trade, balance) |
| `GET /api/events?stream_id=...` | Filtered event listing |
| `POST /api/query` (simple format) | Cross-entity queries by type |
| `GET /api/streams` | List all streams (for user indices) |
| `GET /api/event-types` | List event types |
| `GET /api/events/type/:type` | Events by type |

**Key discovery**: `entity_id` IS the stream ID — there is no separate `stream_id` field in the event schema. Code using `stream_id` when writing to Core should use `entity_id` instead. The Query Service accepts both as query params.

**Fixed**: DSL `where` clause (`"from"`/`"where"` format in `POST /api/query`) previously returned 500. Root cause: `compile_query` only extracted `entity_id`/`event_type` from simple predicates and silently dropped compound predicates (`and`/`or`). Now properly flattens all predicate trees into Core query params.

**Also in this release**: Domain migration from chronos→allsource across all docs, configs, and source files.

Full API reference: [`docs/allsource-qs-api-reference.md`](../allsource-qs-api-reference.md)

---

## Related Documentation

- [QS API Reference](../allsource-qs-api-reference.md) - Verified endpoint reference for v0.10.3
- [Docker Images](../docker-images.md) - Container deployment guide
- [Quality Gates](./QUALITY_GATES_SETUP.md) - CI/CD quality checks
- [RELEASE.md](../../RELEASE.md) - Current release notes
