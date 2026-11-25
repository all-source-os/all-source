# Placement map — L1

## Units

From `git grep -n -A40 '^\[workspace\]' -- Cargo.toml`, `git grep -n -A12 '"workspaces"' -- package.json`,
and `git ls-files 'go.mod' '**/go.mod' 'mix.exs' '**/mix.exs' '**/pyproject.toml'`.

- Root Cargo workspace members: `apps/core`, `sdks/rust`, `tooling/performance`. Every other Rust
  unit is a standalone workspace listed in `[workspace] exclude`, each with its own `Cargo.lock`.
- Bun workspaces: `apps/*`, `packages/*`, `sdks/*`, `tooling/*`.
- Go modules: `apps/control-plane`, `sdks/go`.
- Mix projects: `apps/query-service`, `apps/mcp-server-elixir`.
- Python: `sdks/python-client`, `integrations/langgraph-checkpoint-allsource`.

## Planes

| Plane | Units | Source |
|---|---|---|
| Runs on our servers (Fly, org `allsource`) | `allsource-core`, `allsource-query`, `allsource-control-plane`, `allsource-auth`, `allsource-prime`, `allsource-web`, `allsource-admin` | repo `CLAUDE.md` § Production Deployment Topology |
| Shipped to users | `sdks/*` (published), `apps/prime-mcp` binary, `apps/chronis` CLI, `crates/better-auth-allsource` (published crate), `apps/core` as the embedded library | manifests |
| Build-time / dev-only | `tooling/*` | manifests |
| Published as data | **dropped.** Code is the default repair site `[author, forge-prove-it interview]` | — |

## The placement rules this repo declares

Quoted from repo `CLAUDE.md` or `docs/MONOREPO_STRUCTURE.md`. Every probe is a delta: run it on
`<base>..HEAD`, and re-measure at base when a rule has standing violations.

### P-L1a · Home directories

Rule: SDKs live in `sdks/`, shared Rust libraries in `crates/`, deployables in `apps/`.
Asserted by: nothing.

```
git diff --name-status <base>..HEAD -- '*Cargo.toml' '*package.json' '*go.mod' '*pyproject.toml' '*mix.exs'
```

A manifest with status `A` outside its home is `rock`.

### P-L1b · Dependency direction

Rule: `apps/ → crates/ → (external)`; `sdks/ → (external only)`; no app depends on another app's
source. Asserted by: nothing.

```
git diff <base>..HEAD -U0 -- '*Cargo.toml' '*go.mod' '*mix.exs' '*package.json' | grep -E '^\+.*(path *= *"[./]*apps/|"\.\./\.\./apps|=> .*apps/)'
git diff <base>..HEAD -U0 -- 'sdks/*Cargo.toml' 'sdks/*go.mod' 'sdks/*package.json' 'sdks/*pyproject.toml' | grep -E '^\+.*(path *=|replace |file:|workspace:)'
```

Any hit is `rock`.

### P-L1c · Cross-app Dockerfile COPY

Rule: never `COPY apps/<other-app>/` in a Dockerfile.
Asserted by: nothing.

```
git diff <base>..HEAD -U0 -- '*Dockerfile*' | grep -E '^\+COPY.*apps/'
```

Compare the copied app to the directory of the Dockerfile. A **new** cross-app line is `rock`.
Cross-app copies already stand at base (re-run the grep without `^\+` at base); they are not this
diff's problem.

### P-L1d · Root workspace membership

Rule: do not add an app to root `workspace.members` if it forces other Dockerfiles to stub it.

```
git diff <base>..HEAD -U0 -- Cargo.toml
```

A `+` line inside `members = [` is `sand`: ask which Dockerfiles now need a stub.

### P-L1e · Per-tenant projection compute stays out of Core

Rule: per-tenant read-model compute lives in the Query Service, never in Core.
Asserted by: `Tenant Isolation Gate`, scan root `apps/core/src`. **Delegated** for `apps/core/src`.
**Reviewed** for Core code outside it (`apps/core/examples`, `apps/core/benches`, `apps/core/tests`)
and for any rename that moves Core source out of `src`.

### P-L1f · Core is the database

Rule: no PostgreSQL as a required or primary store. Asserted by: nothing.

```
git diff <base>..HEAD -U0 -- '*Cargo.toml' '*go.mod' '*mix.exs' '*package.json' | grep -iE '^\+.*(postgres|sqlx|tokio-postgres|ecto_sql|jackc/pgx)'
```

`rock`. The pattern already matches lines in root `Cargo.toml` and `apps/core/Cargo.toml` at
`main@b6614086` (with the `postgres_*_repository.rs` files under
`apps/core/src/infrastructure/repositories/`); only additions count. Kill: a `dev-dependencies`
test container whose purpose is a migration or comparison test.

### P-L1g · Durability prose

Rule: never describe Core as in-memory only, non-durable, or losing data on restart.

```
git diff <base>..HEAD -U0 -- '*.md' '*.mdx' '*.tsx' | grep -iE '^\+.*(in-memory only|not production.ready|lost on restart)'
```

`rock`. Kill: the line is a quoted prohibition (like the one in `CLAUDE.md`).

### P-L1h · Clean env var names

Rule: QS uses `CORE_URL` / `CORE_WS_URL` and config key `:core_url`.

```
git diff <base>..HEAD -U0 | grep -E '^\+.*(RUST_CORE_URL|:rust_core_url)'
```

No current sites; pattern validated against a hand-written line. The module name
`RustCoreClient` is not a hit.

### P-L1i · API prefix

Rule: Core endpoints live under `/api/v1/`; `GET /health` and `GET /metrics` are the root paths.

```
git diff <base>..HEAD -U0 -- apps/core/src/infrastructure/web | grep -E '^\+.*\.route\('
```

A new route whose path is not `/api/v1/…`, `/health` or `/metrics` is `rock`.

## Layers inside a unit

Core (`apps/core/src/{domain,application,infrastructure}`) and control plane
(`apps/control-plane/internal/{domain,application,infrastructure}`) are laid out in layers.
No architecture or import test asserts a direction:
`git grep -lE 'fn [a-z_]*(layer|architecture|boundary)[a-z_]*\(' -- apps/core` returns three files,
none of them a layer test. Direction is **convention** `[author, forge-prove-it interview]`:
domain imports nothing from application or infrastructure.

```
git diff <base>..HEAD -U0 -- apps/core/src/domain | grep -E '^\+use crate::(infrastructure|application)'
git diff <base>..HEAD -U0 -- apps/control-plane/internal/domain | grep -E '^\+.*"github.com/allsource/control-plane/internal/(infrastructure|application)'
```

A crossing is `sand`, routed as "say why domain needs this", with the convention quoted. Never
`rock`.

## The repair-site question

Code is the default repair site `[author, forge-prove-it interview]`, so L1 runs no
data-versus-binary triage. It still asks: **is this the hop where the defect entered?** A value
wrong at a gateway is fixed where it is produced, in Core, not re-mapped in `apps/control-plane`
or `rust_core_client.ex`. A fix at the last hop leaves the other consumers (four SDKs, QS, the two
MCP servers) broken: `WRONG-PLANE`.
