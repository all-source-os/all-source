# Production email signup repair

## Root causes and repair

- Auth listened on IPv4 while Fly private DNS uses IPv6; bind now uses `[::]`.
- Browser register/login paths did not match better-auth's routes or its opaque-session contract. Control Plane now validates durable credentials and issues the tenant-scoped product JWT. The web proxy sets an HttpOnly, Secure, SameSite cookie and never exposes that JWT in JavaScript or redirects.
- Auth's storage credential was invalid. A dedicated `allsource-auth` storage tenant and scoped service-account key were provisioned; startup validates the storage identity. Existing other-product auth streams were not migrated or overwritten.
- Auth reads and writes now explicitly share the same tenant scope.
- The upstream email plugin stored password hashes in user metadata, which our adapter correctly rejects. The replacement uses Argon2 through better-auth's password utilities and stores hashes only in credential Accounts. Banned/2FA-enabled accounts cannot bypass verification.
- Workspace identity derives from the authenticated user ID, not unverified email. Email signups cannot inherit OAuth workspaces or ADMIN_EMAILS privileges.
- Trial metadata is persisted separately because Core's create DTO ignores metadata. Both quota wire keys are supplied for current consumers. Web tier normalization now recognizes `trial` rather than labeling it self-host.

## Production verification

Real browser signup reached onboarding and authenticated dashboard. A separate Rust smoke run verified two QA signups, duplicate rejection, wrong-password rejection, returning login, HttpOnly sessions, distinct workspaces, event write/read, cross-tenant isolation, and logout. Synthetic credentials stay in process memory; test addresses use reserved `example.invalid`. Events carry explicit QA markers.

Final quota-enhanced smoke passed after deployment: both new sessions reported tier `trial`, 1,000 events and 100 queries. This second smoke added two QA signups and one QA workspace event after the count snapshot below; total controlled test artifacts are five accounts and two activated QA workspaces. These are not customer conversions.

Auth Rust tests: 9 library + 1 IPv6 listener test passed. Full web build and Fly health checks passed. Control Plane stays running because `.internal` requests bypass Fly Proxy auto-start.

Deployed images: auth `deployment-01M327HR3N4ZA25595V8C7GAK3`; web `deployment-01M329VV28Y0041QQ1TEFT9896`; Control Plane quota fix `deployment-01M329VN8672HE46A1S5ZC282E` (subsequent configuration-only rollout keeps it running).

## Authoritative count snapshot

Window: **2026-09-20 00:00 UTC inclusive to 2026-09-21 15:03 UTC exclusive**.

| Metric | Count |
| --- | ---: |
| Non-QA email signups | 0 |
| Activated workspaces from that customer email cohort | 0 |
| QA email signups | 3 |
| QA workspaces with a test event | 1 |

Source: aggregate Core event reconciliation. Activation means at least one non-seeded, non-system event from a workspace belonging to the new email signup cohort. This is not an all-time user count and excludes OAuth, deleted tenants, other products, and historical cohorts. It does not establish that no customers exist.

PostHog reconciliation remains unverified: browser access was denied because the approval workspace was out of credits. Do not equate missing analytics events with missing backend signups. Project 244095 is shared; any later reconciliation must filter AllSource and exclude QA/test traffic. No alternate access path was attempted after that denial.

## Repeatable checks

- `cd apps/web && bun run type-check && bun run test` — 166 tests passed.
- `cd apps/control-plane && go test ./...` — passed (test servers require local socket permission).
- `RUSTC_WRAPPER= cargo test --manifest-path apps/auth/Cargo.toml --lib --bins`
- `RUSTC_WRAPPER= cargo run --manifest-path apps/auth/Cargo.toml --example verify_signup -- --apply` — creates two marked QA accounts and one marked QA event; do not run as a read-only count query.

The one-time `provision_storage` example changes credentials and must not be rerun blindly. No secrets belong in reports, command output, or git.
