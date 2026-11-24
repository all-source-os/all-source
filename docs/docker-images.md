---
title: "AllSource Docker Images"
status: CURRENT
last_updated: 2026-11-24
category: operations
---

# AllSource Docker Images

This document describes the official Docker images for the AllSource event store platform.

## Authentication

GitHub Container Registry requires authentication to pull images:

```bash
# Create a Personal Access Token (PAT) with read:packages scope at:
# https://github.com/settings/tokens/new?scopes=read:packages

# Login to GHCR
echo $GITHUB_TOKEN | docker login ghcr.io -u YOUR_USERNAME --password-stdin

# Or use GitHub CLI (recommended)
gh auth token | docker login ghcr.io -u $(gh api user -q .login) --password-stdin
```

> **Note**: If the packages are made public, authentication is not required for pulling.

## Available Images

All images are published to GitHub Container Registry (GHCR). Images **without** a
`-community` suffix are Enterprise (BSL 1.1) and require `docker login ghcr.io` (see
[Authentication](#authentication) above); `*-community` images are Apache 2.0 and public:

| Image | Description | Port |
|-------|-------------|------|
| `ghcr.io/all-source-os/allsource-core` | High-performance Rust event store | 3900 |
| `ghcr.io/all-source-os/allsource-query-service` | Elixir API gateway (stateless, no database) | 3902 |
| `ghcr.io/all-source-os/allsource-control-plane` | Go auth, billing, operations | 3901 |
| `ghcr.io/all-source-os/allsource-web` | Next.js web dashboard | 3000 |
| `ghcr.io/all-source-os/allsource-mcp-server` | Elixir MCP server for AI integration (stdio) | n/a |
| `ghcr.io/all-source-os/allsource-mcp-server-embedded` | Same, with Core in-process via Rustler NIF (stdio) | n/a |

## Image Tags

Images are tagged with multiple identifiers:

- `latest` - Latest build from main branch (moving)
- `main` - Latest build from main branch (moving)
- `0.22.0` - Specific version, **unprefixed** semver (the git tag for the same release is `v0.22.0`)
- `0.22` - Major.minor version
- `0` - Major version
- `sha-abc1234` - Git commit SHA

Pin a specific version in production; `latest` moves under you.

## Quick Start

### Using Docker Compose (Recommended)

Create a `docker-compose.yml`:

```yaml
services:
  allsource-core:
    image: ghcr.io/all-source-os/allsource-core:latest
    ports:
      - "3900:3900"
    volumes:
      - allsource-data:/data
    environment:
      - RUST_LOG=info
    healthcheck:
      test: ["CMD", "wget", "-q", "--spider", "http://localhost:3900/health"]
      interval: 30s
      timeout: 10s
      retries: 3

  allsource-control-plane:
    image: ghcr.io/all-source-os/allsource-control-plane:latest
    ports:
      - "3901:3901"
    environment:
      - CORE_URL=http://allsource-core:3900
      - JWT_SECRET=your-jwt-secret
    depends_on:
      - allsource-core

  allsource-query:
    image: ghcr.io/all-source-os/allsource-query-service:latest
    ports:
      - "3902:3902"
    environment:
      - CORE_URL=http://allsource-core:3900
      - CONTROL_PLANE_URL=http://allsource-control-plane:3901
      - SECRET_KEY_BASE=generate-with-mix-phx-gen-secret
    depends_on:
      - allsource-core
      - allsource-control-plane

  allsource-web:
    image: ghcr.io/all-source-os/allsource-web:latest
    ports:
      - "3000:3000"
    environment:
      - NEXT_PUBLIC_API_URL=http://localhost:3902
    depends_on:
      - allsource-query

volumes:
  allsource-data:
```

Start the stack:

```bash
docker compose up -d
```

### Single Container

Run just the core event store:

```bash
docker run -d \
  --name allsource-core \
  -p 3900:3900 \
  -v allsource-data:/data \
  ghcr.io/all-source-os/allsource-core:latest
```

## Environment Variables

### chronos-core

| Variable | Description | Default |
|----------|-------------|---------|
| `RUST_LOG` | Log level (trace, debug, info, warn, error) | `info` |
| `DATA_DIR` | Data directory path | `/data` |
| `HTTP_PORT` | HTTP server port | `3900` |

### chronos-query-service

| Variable | Description | Default |
|----------|-------------|---------|
| `CORE_URL` | URL of allsource-core | Required |
| `CONTROL_PLANE_URL` | URL of control plane | `http://localhost:3901` |
| `PORT` | HTTP server port | `3902` |
| `SECRET_KEY_BASE` | Phoenix secret key | Required for prod |
| `GOOGLE_CLIENT_ID` | Google OAuth client ID | Optional |
| `GITHUB_CLIENT_ID` | GitHub OAuth client ID | Optional |

### chronos-control-plane

| Variable | Description | Default |
|----------|-------------|---------|
| `PORT` | HTTP server port | `3901` |
| `CORE_URL` | URL of allsource-core | Required |
| `JWT_SECRET` | JWT signing secret | Required |
| `JAEGER_ENDPOINT` | Jaeger tracing endpoint | Optional |

### chronos-web

| Variable | Description | Default |
|----------|-------------|---------|
| `PORT` | HTTP server port | `3000` |
| `NEXT_PUBLIC_API_URL` | Public API URL | Required |

### allsource-mcp-server

| Variable | Description | Default |
|----------|-------------|---------|
| `CORE_URL` | Gateway URL (or Core, if internal) | Required |
| `CORE_API_KEY` | Bearer token for an authenticated Core/gateway | Required for hosted |
| `MIX_ENV` | Elixir environment | `prod` |
| `ALLSOURCE_READ_ONLY` | Hide the 10 mutation tools | `false` |
| `ALLSOURCE_CONTROL_URL` | Enables the 9 tenant/fleet tools | Unset |
| `ALLSOURCE_SYSTEM_ADMIN` | Enables the 8 recovery tools + tenant_notice | `false` |

> The MCP server speaks **stdio**, not HTTP — it is launched as a subprocess by the MCP
> client (`docker run -i`). There is no port to publish and no SSE endpoint.

## Multi-Architecture Support

All images are built for `linux/amd64` (x86_64). CI does not build `linux/arm64` images.

## Health Checks

All images include health checks:

```bash
# Check if container is healthy
docker inspect --format='{{.State.Health.Status}}' allsource-core
```

## Security

- All images run as non-root users
- Base images are Alpine Linux (minimal attack surface)
- Images are scanned for vulnerabilities during CI/CD
- No secrets are baked into images

## License

AllSource ships under a **dual licence**, and which one applies depends on the image:

| Edition | Images | Licence | Registry access |
|---------|--------|---------|-----------------|
| Community | `*-community` (e.g. `allsource-core-community`) | [Apache 2.0](../LICENSE) | Public — no login |
| Enterprise | all others (e.g. `allsource-mcp-server`) | [BSL 1.1](../LICENSE-BSL) | Login required |

The BSL 1.1 licence converts to Apache 2.0 on **2029-03-01**. Its Additional Use Grant
permits production use but not offering the Licensed Work to third parties as a
commercially hosted or managed service — see `LICENSE-BSL` for the binding text.

> **Note**: `allsource-core/LICENSE` is MIT, a deliberate per-component exception.
> `LICENSE` and `LICENSE-BSL` at the repository root are authoritative for everything else.

### Attribution requirements

Under Apache 2.0 (community images and source):

1. **Source redistribution**: include the `LICENSE` file and retain copyright, patent,
   trademark, and attribution notices
2. **Binary distribution**: include the licence notice in your documentation
3. **Modified files**: state that you changed them
4. **NOTICE**: if a `NOTICE` file is present, carry its attributions forward
5. **SaaS/hosted**: no attribution required in your UI, but welcome

### Commercial use

Apache 2.0 (community) permits commercial use without royalties, modification, private
use, distribution, sublicensing, and carries an express patent grant. It places no
restriction on proprietary derivatives, closed-source modifications, or charging for
services built on AllSource.

Enterprise (BSL 1.1) images carry the hosted-service restriction above until the
2029-03-01 change date. If you intend to offer AllSource itself as a managed service
before then, use the community edition or obtain a commercial licence.

## Support

- GitHub Issues: https://github.com/all-source-os/all-source/issues
- Documentation: https://github.com/all-source-os/all-source/tree/main/docs

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for version history.
