# Workspace ownership verification

Host: macOS arm64, pinned Rust `nightly-2026-07-16`, Go module toolchain 1.26.6.
Commands used five-minute outer deadlines (`gtimeout -k 15 300`), except the
final optimized build's bounded 15-minute window. Tests used
private temporary WAL directories, synthetic identities and owned loopback Core
processes. Google/GitHub and the production email identity service were not used.

## Recorded checks

| Check | Result | Local log |
|---|---|---|
| Core full-feature library suite | 1,990 passed, 5 ignored, 0 failures | `/private/tmp/allsource-owner-core-allfeatures-test-final-20260926.log` |
| Final Core conditional config and tenant initialization tests | 3 + 2 passed; 0 failures | `/private/tmp/allsource-owner-core-strict-final-20260927.log` |
| Final Core all-targets/all-features Clippy | passed with `-D warnings` | `/private/tmp/allsource-owner-core-clippy-final-20260927.log` |
| Final Rust format check | passed | `/private/tmp/allsource-owner-rustfmt-pass-20260927.log` |
| Final Control Plane full suite | `go test ./...` passed | `/private/tmp/allsource-owner-cp-suite-verified-20260927.log` |
| Final real Core HTTP suite with Go race detector | 8 real-Core tests plus 8 existing email-auth subcases passed; 0 failures | `/private/tmp/allsource-owner-authority-final-20260927.log` |
| Final Control Plane lint | 0 issues | `/private/tmp/allsource-owner-cp-lint-verified-20260927.log` |
| Final existing Query Service/HTTP/compiled MCP stack | 14 tests, 0 failures, 1 optional Claude skip; 13 executed | `/private/tmp/allsource-owner-mcp-verified-20260927.log` |
| Enterprise Core binary build | passed | `/private/tmp/allsource-owner-core-build-strict-20260927.log` |
| Optimized Core library build | passed; final cached-dependency attempt completed in 1m 33s | `/private/tmp/allsource-owner-core-release-final-20260927.log` |
| Core documentation with warnings denied | passed | `/private/tmp/allsource-owner-core-docs-20260927.log` |
| Core manifest sorting | `cargo sort --check` passed | command output: `Checking core...` |

The full Core suite ran before tightening the `absent` enum variant's rejection
of unknown JSON fields; final all-targets Clippy, focused durability tests and
actual HTTP tests ran after that correction. Final real-Core tests also deny a
returning owner's session when its configured Core is a follower. Final Go full
suite and lint ran after that change. Formatting-only edits do not
change tested behavior. No lockfiles or lint suppressions were changed.

The first full-feature Rust attempt timed out during dependency compilation;
its bounded retry completed. An enterprise-only Clippy run hit an existing
feature-disabled async-trait warning in unchanged search code. The repository's
all-features gate passed; that warning was not suppressed or changed here.
The first optimized build also reached its five-minute deadline; the retry used
a 15-minute outer bound and completed normally. Documentation ran afterward.

Actual HTTP testing found and corrected permissive unknown fields in Serde's
unit enum variant. Another test initially expected 200 from config deletion;
the existing API correctly returns 204, and the fixture now handles that empty
response. A parallel MCP regression run timed out waiting for one owned Core
process to start. The isolated rerun passed without changing its deadline or
assertions. Startup contention is a possibility, not an established root cause.

## Reproduce

From the repository root:

```text
cargo fmt --all -- --check
cargo clippy --offline -p allsource-core --all-targets --all-features -- -D warnings
cargo test --offline -p allsource-core --all-features --lib
cargo test --offline -p allsource-core --features enterprise --test config_conditions --test tenant_initialization
cargo build --offline -p allsource-core --features enterprise --bin allsource-core
cargo build --offline -p allsource-core --lib --release
RUSTDOCFLAGS='-D warnings' cargo doc --offline -p allsource-core --no-deps --document-private-items
```

From `apps/control-plane`:

```text
go test ./...
golangci-lint run
ALLSOURCE_CORE_BINARY=/absolute/path/to/target/debug/allsource-core go test -race -run 'Test(OAuthWorkspace|EmailWorkspace|WorkspaceCore|EmailAuthService)' -count=1 -v .
```

From `apps/query-service`, using the existing compiled MCP release:

```text
env -u ALLSOURCE_CLAUDE_BINARY -u ALLSOURCE_CLAUDE_TRACE \
  MIX_ENV=test \
  ALLSOURCE_CORE_BINARY=/absolute/path/to/target/debug/allsource-core \
  ALLSOURCE_CUSTOMER_MCP_BINARY=/absolute/path/to/apps/mcp-server-elixir/_build/prod/rel/mcp_server_elixir/bin/mcp_server_elixir \
  mix test --include integration \
  test/query_service_ex/integration/customer_agent_http_test.exs \
  test/query_service_ex/integration/customer_agent_grant_core_test.exs \
  test/query_service_ex/domain/customer_agent/eligibility_test.exs
```

The Core binary used by final HTTP/MCP proof has SHA-256
`530adda2b4d65744e8a2f31e0060a22908f0410394c4361e39b25327993b9065`.
The MCP release is unchanged from the prior live-access evidence. Source hashes
are recorded alongside this document, relative to the repository root. Build
outputs and raw full-suite logs are not committed.
