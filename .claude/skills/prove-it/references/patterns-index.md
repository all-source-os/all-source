# Patterns index — L2

## Canonical seams

Found by import frequency, confirmed by each module's own header. Commands:

- Rust (Core): `git grep -hoE '^use crate::[a-z_]+(::[a-z_]+)?' -- 'apps/core/src/*.rs' | sort | uniq -c | sort -rn`
- Elixir (QS): `git grep -hoE 'alias QueryServiceEx\.[A-Za-z.]+' -- 'apps/query-service/lib/*.ex' | sort | uniq -c | sort -rn`
- Go (control plane): `git grep -hoE '^[[:space:]]+"github.com/allsource[^"]*"' -- 'apps/control-plane/*.go' | sort | uniq -c | sort -rn`

### S1 · `apps/core/src/error.rs` — `AllSourceError`, `Result<T>`

The crate's error enum and result alias. Reinvention shape: a new error enum or a boxed error
return in Core.

```
git diff <base>..HEAD -U0 -- apps/core/src | grep -E '^\+(pub )?enum [A-Za-z]*Error|^\+.*-> .*Box<dyn (std::error::)?Error'
```

### S2 · `QueryServiceEx.Infrastructure.Adapters.RustCoreClient`

`apps/query-service/lib/query_service_ex/infrastructure/adapters/rust_core_client.ex`;
`@moduledoc`: "HTTP client adapter for communicating with the Rust Event Store Core."
Reinvention shape: a QS module calling Core over HTTP directly.

```
git diff <base>..HEAD -U0 -- apps/query-service/lib | grep -E '^\+.*(Req\.(get|post|put|delete|request)|Finch\.|HTTPoison\.|Tesla\.)'
```

Kill: the hunk is under `infrastructure/adapters/`.

### S3 · `apps/control-plane/internal/infrastructure/clients/core_client.go` — `CoreClient`

"Package clients provides typed HTTP clients for communicating with external services." /
"CoreClient defines typed methods for communicating with the Core service". Reinvention shape: a
control-plane file building a Core request by hand.

```
git diff <base>..HEAD -U0 -- apps/control-plane | grep -E '^\+.*http\.(NewRequest|NewRequestWithContext|Get|Post)'
```

A hit is a finding when the same hunk names an `/api/v1/` path and the file is outside
`internal/infrastructure/clients/`. The gateway's reverse-proxy files (`read_model.go`,
`egress_guard.go`) forward requests by design; read their header before reporting.

### S4 · `sdks/contract/fixtures/*.json`

`sdks/contract/README.md`: shared wire fixtures. The Go, Python, Rust and TypeScript suites each
load every file there (`git grep -n 'contract/fixtures' -- sdks`). Reinvention shape: a new SDK
test asserting a wire shape inline instead of adding a fixture — judged, no grep.

Re-grep each header before quoting it; a header that changed since this census is doc drift, not a
finding.

## The bounded sibling algorithm

For each **new file or new exported symbol**:

1. **Nearest siblings.** List the directory the new file landed in; if the diff added the
   directory, list its parent. Cap at the **8** siblings whose names share the most tokens with
   the new file's.
2. **Read their headers, not their bodies.**

   | Language here | The header | The export-list trap |
   |---|---|---|
   | Rust | the `//!` module doc | `^pub ` under-counts: a crate-internal seam is `pub(crate)` |
   | Elixir | `@moduledoc` | — |
   | Go | the package comment, then the doc comment on the first exported type | `^func ` misses methods with receivers |
   | TypeScript | leading block comment + exports | `^export ` misses `export * from` |
   | Python | module docstring + `__all__` | `^def ` misses decorated functions |

3. **Decide, and say which.** A sibling's header describes this responsibility → **finding**,
   name it and quote the header line. No sibling owns it and the new file's header says why it
   exists → **not a finding**. No sibling owns it and the new file has **no header** → `sand`,
   routed as "say why this is new".
4. **Stop.** One pass, no widening. No siblings at all → print `no sibling set — new subsystem,
   reviewed by L1 for placement instead`.

Print the bound: `L2: <n> new symbols, <n> siblings read, <n> findings`.

## This repo's sibling pattern

Core and QS split large modules into **sibling files**, not nested modules: `store.rs` with
`store_feed.rs`, `store_view.rs`, `store_strict_read.rs` …; tests in `*_tests.rs` siblings
(`git ls-files 'apps/core/src/store_*'`). A new `store_*.rs` sibling is the sanctioned shape
(discipline rule 10). A new top-level directory beside them is the shape to question.
