# Monorepo Structure — Best Practices

> **This is authoritative.** All contributors (human and AI) must follow this layout. Do not create new top-level directories or place code in the wrong location.

---

## Directory Layout

```
all-source/
├── apps/                  # Deployable services (backend, frontend, CLI)
│   ├── core/              # Rust event store (AllSource Core)
│   ├── query-service/     # Elixir/Phoenix API gateway
│   ├── control-plane/     # Go control plane (auth, onboarding, billing, admin)
│   ├── auth/              # Rust auth service
│   ├── web/               # Next.js frontend dashboard
│   ├── admin/             # Admin frontend
│   ├── mcp-server-elixir/ # Elixir MCP server
│   ├── prime-mcp/         # Rust MCP server for Prime
│   ├── chronis/           # Event-sourced task CLI (standalone workspace)
│   └── marketing-assets/  # Marketing assets
│
├── sdks/                  # Client SDKs, one directory per language
│   ├── rust/
│   ├── go/
│   ├── python-client/
│   ├── typescript/
│   └── contract/          # Shared SDK test fixtures for the latest-state read contract
│
├── crates/                # Shared Rust libraries (not binaries)
│   ├── allsource-prime-models/   # Vendored all-MiniLM-L6-v2 ONNX weights for Prime
│   ├── better-auth-allsource/   # better-auth DatabaseAdapter (event-sourced auth)
│   └── hound-extract/           # Tree-sitter codebase extractor for Prime Hound
│
├── packages/              # Shared non-Rust packages (NOT SDKs, NOT crates)
│   └── ui/                # Shared UI component library
│
├── deploy/                # Deployment manifests (k8s, helm, grafana, prometheus, cloudrun)
├── docs/                  # Documentation
├── integrations/          # Integration packages (langgraph-checkpoint-allsource)
├── plugin/                # Plugin packages (mammoth)
├── skills/                # Agent skills shipped with the repo
├── scripts/               # Build and automation scripts
├── tooling/               # Developer tools and quality gates, one directory per tool
├── test-results/          # Test output (tracked: .last-run.json only)
│
├── .agents/               # Agent skills
├── .chronis/              # Chronis task sync state
├── .claude/skills/        # Claude Code skills (auto-discovered)
├── .claude-plugin/        # Claude plugin marketplace manifest
├── .github/               # CI workflows
└── .rewind/               # Rewind configuration
```

`tmp/` and `.planning/` exist in a working checkout but hold no git-tracked files.
They are not part of the layout. Do not commit files under them.

---

## Rules

### 1. SDKs go in `sdks/`, nowhere else

Every client library — regardless of language — lives under `sdks/`. Not in `packages/`, not in `apps/`, not at the root.

```
sdks/
├── rust/           # name by language, not "rust-client"
├── go/
├── python-client/  # (legacy naming, would be python/ if starting fresh)
├── typescript/     # TypeScript SDK
└── swift/          # future
```

Naming convention: prefer the bare language name (`rust/`, `go/`, `python/`). Avoid `-client` suffix for new SDKs.

### 2. `apps/` is for deployable services only

Each subdirectory in `apps/` produces a deployable artifact (Docker image, binary, or web bundle). If it doesn't get deployed independently, it doesn't belong here.

### 3. Apps are fully isolated

**This is a hard rule.** Each app is self-contained — no app may depend on, import, or copy another app's source code.

#### What isolation means in practice

| Principle | Correct | Wrong |
|-----------|---------|-------|
| **Build** | Each app's Dockerfile only references its own source and shared crates (`sdks/`, `packages/`) | `COPY apps/chronis/ apps/chronis/` inside Core's Dockerfile |
| **Cargo workspace** | Apps that need independence use `workspace.exclude` and manage their own `[dependencies]` | Every Rust app in one flat workspace, forcing all Dockerfiles to stub every other app |
| **Runtime** | Apps communicate over the network (HTTP, gRPC, WebSocket) | Direct function calls or shared-memory coupling between apps |
| **Shared code** | Extract to `sdks/` or `packages/` | One app importing from another app's `src/` |

#### Rust workspace isolation

Apps that would create cross-contamination in Dockerfiles (e.g., chronis depends on core as a library) should be **excluded** from the root Cargo workspace:

```toml
# Root Cargo.toml
[workspace]
exclude = ["apps/chronis"]
```

The excluded app manages its own dependency versions and can still use `path = "../core"` for local development. This means:
- Core's Dockerfile does not need to know chronis exists
- Registry's Dockerfile does not need to stub chronis
- Adding a new app never requires modifying another app's build

#### Why this matters

Without isolation, adding app N forces changes to apps 1 through N-1. Dockerfiles accumulate stubs, build times grow, and a change to one app's Cargo.toml can break another app's Docker build. Isolation keeps each app's blast radius to itself.

### 4. `crates/` is for shared Rust libraries

Shared Rust crates consumed by multiple `apps/`. If a Rust crate is a library (not a deployable binary) and is used by more than one app, it belongs in `crates/`.

```
crates/
├── allsource-prime-models/
├── better-auth-allsource/
└── hound-extract/
```

**Note**: `allsource-core` currently lives in `apps/core/` because it is both a library and a deployable binary. If/when the library is split from the binary, the library portion moves to `crates/` and the binary stays in `apps/`.

**DO NOT** put crates in `packages/`, `sdks/`, or `apps/` unless they are deployable binaries.

### 5. `packages/` is for shared non-Rust packages (NOT SDKs, NOT crates)

Shared non-Rust libraries consumed by `apps/` — UI component libraries, shared types, internal utilities. Not for external-facing client SDKs, not for Rust crates.

### 6. SDKs and crates are fully isolated (same as apps)

The same isolation principles from Rule 3 apply to `sdks/` and `crates/`:

- **Each SDK is self-contained.** No SDK may import from another SDK, from `apps/`, or from `crates/` source code directly.
- **Each shared crate is self-contained.** Crates may depend on other crates via `path = "../other-crate"` within `crates/`, but never on `apps/` source.
- **Dockerfiles never cross boundaries.** An app's Dockerfile references its own source, plus `crates/` and `sdks/` Cargo.toml stubs if needed — never another app.
- **No circular dependencies.** The dependency graph is strictly: `apps/` → `crates/` → (external). SDKs are leaf nodes — they depend on nothing internal.

```
Allowed dependency direction:

  apps/       →  crates/     →  (external crates.io deps)
  apps/       →  sdks/       (only if embedding an SDK)
  sdks/       →  (external)  (SDKs are standalone HTTP clients)
  crates/     →  crates/     (shared crate → shared crate OK)
  packages/   →  (external)  (UI components are standalone)
```

**DO NOT** create dependencies from `sdks/` → `apps/`, `sdks/` → `crates/`, `crates/` → `apps/`, or `packages/` → `apps/`.

### 7. `docs/` subdirectories have purpose

| Directory | Contents |
|-----------|----------|
| `proposals/` | Design proposals (RFC-style, named `UPPER_SNAKE_CASE.md`) |
| `use-cases/` | Use case documents tied to proposals |
| `current/` | Living architecture documentation |
| `sales/` | Customer-facing documents |
| `checklists/` | Operational runbooks and checklists |

Other `docs/` subdirectories (for example `adr/`, `guides/`, `runbooks/`, `reports/`) follow the same rule: one purpose per directory.

### 8. No stray top-level directories

Do not create new top-level directories without updating this document. The current set is intentional:

- `apps/` — deployable services and binaries
- `crates/` — shared Rust library crates
- `sdks/` — client SDKs
- `packages/` — shared non-Rust packages
- `deploy/` — deployment configs
- `docs/` — documentation
- `scripts/` — automation
- `tooling/` — dev tools and gates
- `test-results/` — test output
- `integrations/`, `plugin/`, `skills/` — integration and plugin code
- `.agents/`, `.chronis/`, `.claude/`, `.claude-plugin/`, `.github/`, `.rewind/` — tool and CI configuration

### 9. Version consistency

Apps and services share one version number. Use `make set-version VERSION=X.Y.Z` to bump them and `make check-versions` to verify. `set-version` does not touch the SDKs.

SDKs release independently. Bump one with `make set-sdk-version SDK=<rust|go|typescript|python> VERSION=X.Y.Z`. Its release tag is `sdk-<lang>-v<VERSION>`, so SDK versions do not follow the Core version.

---

## Cleanup History

Legacy SDK copies in `packages/` (`go-client/`, `python-client/`, `client/`) were removed in v0.10.5. All SDK code now lives exclusively in `sdks/`.
