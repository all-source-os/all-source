# Release Procedure

How to cut a new release of the AllSource Chronos monorepo.

## Quick Reference

```bash
# 1. Bump versions
make set-version VERSION=X.Y.Z
cd apps/core && cargo update --workspace && cd ../..

# 2. Run CI (fix until green)
make ci

# 3. Commit + tag
git add -A
git commit -m "release: vX.Y.Z — description"
git tag -a vX.Y.Z -m "vX.Y.Z — description"

# 4. Push
git push && git push --tags
```

Or just tell Claude: `/chronos-release 0.10.4`

---

## Detailed Steps

### 1. Choose a Version

We use [SemVer](https://semver.org/):
- **Patch** (0.10.3 -> 0.10.4): bug fixes, CI fixes, doc updates
- **Minor** (0.10.x -> 0.11.0): new features, non-breaking API changes
- **Major** (0.x -> 1.0): breaking changes, production launch

Check the current version:
```bash
grep 'version = ' apps/core/Cargo.toml | head -1
```

### 2. Verify Preconditions

```bash
git status          # Must be clean (or only intended changes staged)
git branch --show-current  # Should be main
git tag -l "vX.Y.Z" # Must return empty (tag must not exist)
```

### 3. Bump Versions

```bash
make set-version VERSION=X.Y.Z
```

This updates 8 files automatically:
| File | What |
|------|------|
| `apps/core/Cargo.toml` | Rust crate version |
| `apps/control-plane/main.go` | Go binary version const |
| `apps/control-plane/tracing.go` | OpenTelemetry service version |
| `apps/query-service/mix.exs` | Elixir app version |
| `apps/mcp-server-elixir/mix.exs` | MCP server version |
| `deploy/k8s/core.yaml` | Docker image tag |
| `deploy/k8s/query-service.yaml` | Docker image tag |
| `README.md` | Displayed version |

**Also update manually** (not covered by `set-version`):
```bash
# Lock file
cd apps/core && cargo update --workspace && cd ../..

# OpenAPI spec
# Edit apps/control-plane/docs/openapi.yaml → info.version
```

### 4. Run CI

```bash
make ci
```

This runs all quality gates in parallel:
- **check-versions**: Verifies all version strings match
- **quality-rust**: fmt, sort, clippy, tests, docs, release build
- **quality-go**: fmt, golangci-lint, tests, build
- **quality-elixir-full**: format, unused deps, compile (warnings-as-errors), credo --strict, dialyzer, tests (x2 apps)

Fix any failures, then re-run. Do **not** commit between CI fix rounds.

### 5. Commit

Create exactly **one** commit with all version bumps + CI fixes:

```bash
git add -A
git commit -m "$(cat <<'EOF'
release: vX.Y.Z — brief description of what changed

- Key change 1
- Key change 2
- Key change 3

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

### 6. Tag

Create an **annotated** tag:

```bash
git tag -a vX.Y.Z -m "vX.Y.Z — brief description"
```

### 7. Push

```bash
git push && git push --tags
```

---

## Immutable Tags Policy

**Tags are permanent.** Once a tag is created and pushed, it must never be moved or deleted.

- If you discover a bug after tagging: bump the version and cut a new release
- v0.10.3 has a bug -> fix it -> release v0.10.4
- Never `git tag -d` + `git tag -a` on a pushed tag
- Never `git push --force origin --tags`

This ensures that every tag is a reliable, permanent reference point. Docker images, changelogs, and deployment records can always trace back to the exact commit.

---

## What `make ci` Checks

| Gate | Tool | Fails On |
|------|------|----------|
| Rust format | `cargo fmt --check` | Any unformatted Rust code |
| Rust sort | `cargo sort --check` | Unsorted Cargo.toml deps |
| Rust lint | `cargo clippy -D warnings` | Any clippy warning |
| Rust tests | `cargo test --lib` | Test failures |

The toolchain comes from `rust-toolchain.toml`. Do not write
`cargo +nightly`: it overrides the pin, so the gate lints against whichever
nightly the machine last installed rather than the one CI uses.
| Rust docs | `RUSTDOCFLAGS="-D warnings" cargo doc` | Broken doc links |
| Rust build | `cargo build --release` | Compilation errors |
| Go format | `gofmt -l` | Any unformatted Go code |
| Go lint | `golangci-lint run` | Lint warnings (goconst, gocritic, etc.) |
| Go tests | `go test -v -race` | Test failures, races |
| Go build | `go build` | Compilation errors |
| Elixir format | `mix format --check-formatted` | Unformatted Elixir code |
| Elixir deps | `mix deps.unlock --check-unused` | Unused deps in mix.lock |
| Elixir compile | `mix compile --warnings-as-errors` | Compiler warnings |
| Elixir lint | `mix credo --strict` | Credo issues (alias ordering, complexity, etc.) |
| Elixir types | `mix dialyzer` | Type errors (filtered by .dialyzer_ignore.exs) |
| Elixir tests | `mix test` | Test failures |

---

## Troubleshooting

**`make set-version` fails**: Check that all target files exist. The Makefile uses `sed -i ''` (macOS). If a file path changed, update the Makefile.

**Clippy fails with `collapsible_if`**: Use Rust let-chain syntax: `if let Some(x) = foo && x > 0 { }` (requires nightly).

**Credo fails with cyclomatic complexity**: Either refactor to reduce branching, or add `# credo:disable-for-next-line Credo.Check.Refactor.CyclomaticComplexity`.

**Dialyzer fails**: Add pattern to `apps/query-service/.dialyzer_ignore.exs` for known false positives.

**Tests fail on Core connection refused**: Expected in CI — Core is not running. Tests should handle `{:error, :econnrefused}` gracefully.
