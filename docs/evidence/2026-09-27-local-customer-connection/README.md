# Local customer connection ownership and installation

27 September 2026. Base: `e79b5d71844e20ca1dabdc0973e4a1340c166fdc`.
Local macOS verification with synthetic accounts and isolated Core storage.
Task `t-7ce02d` remains open; this does not complete the customer delivery goal.

## Result

The existing Elixir MCP customer profile no longer accepts a bearer credential
or binding through environment variables. Its only connection setting is an
absolute file path. Each context/validation call uses the packaged Rust reader
to check real/effective OS UID, directory ownership, permissions, link count,
extended ACLs and bounded file content before making any HTTP request. File
reads traverse directory descriptors without following symlinks. Unsupported or
failed checks deny access without credential/path diagnostics.

The same utility installs a bounded JSON line from non-interactive stdin into a
new mode-0600 file inside a mode-0700 owner directory. It never overwrites an
existing file. Interrupted installation may leave a partial private file; the
runtime's JSON/schema checks deny it. Choose a new filename for a retry.

Connection settings now provide complete one-time configuration and ordered
clipboard/terminal instructions. Credentials stay outside command arguments,
shell history and Claude's registration configuration. HTTPS destinations must
match the bound resource origin; literal HTTP loopback remains a local-test
exception. The original live membership, entitlement and durable revocation
checks still apply. See the [configuration/runbook](../../../apps/mcp-server-elixir/CUSTOMER_CONNECTIONS.md).

## Verification

| Check | Observed result |
|---|---|
| Rust reader/installer tests on macOS | 8 passed; includes 1 unit owner predicate and 7 subprocess cases |
| Strict Rust Clippy and rustfmt | Passed |
| Full MCP suite | 643 tests, 0 failures, 5 integration/embedded exclusions |
| Final focused MCP suite | 7 passed against final packaged reader |
| MCP production release | Built, includes installer under `bin` and reader under app `priv/bin` |
| Final actual installer → compiled MCP → HTTP → Core | 10 tests, 0 failures, 2 intentional skips; 8 executed |
| Opt-in browser fixture | 4 tests, 0 failures, 155.4 seconds |
| Full web suite | 43 files, 203 tests passed |
| Web TypeScript/production build/targeted Biome | Passed |
| MCP warnings-as-errors compile, format, configured strict Credo, Dialyzer | Passed; no filters/suppressions added |
| QS format and strict Credo | Passed |
| Workflow actionlint and whitespace | Passed |

Rust tests exercise private-file reload, misleading UID/USER environment values,
wrong expected owner, shared/executable/special modes, ACL grants despite private
mode, parent permissions, ancestor links, hard links, relative/parent paths,
oversized/empty content and nonblocking FIFO rejection. Installer tests verify
exact persisted bytes, permissions, no output, no overwrite and link/input denial.
The wrong-owner predicate test uses real file metadata with a different expected
UID; it does not impersonate another login or require root.

Final actual-Core test invokes the release's installer and MCP executables,
changes the installed file to 0644 between calls and observes denial, restores
0600 and observes success, then revokes in Core and proves current/reconnected
MCP denial. It retains exclusive tool discovery and no-approval assertions.
Two skips are the separately opted-in native Claude model test and manual
browser fixture. The latter ran independently as described below.

Full MCP regression preceded the final installer/JSON-line additions; final
Rust and focused MCP/actual-Core tests include those changes. QS production code
did not change in this increment, so its earlier full-suite evidence remains
applicable; only relevant integration/fixture and lint checks were repeated.

## Browser observation

Actual Next production build on `127.0.0.1:4344`, synthetic identity exchange on
4345 and real connection handlers/Core. After refreshing the expired synthetic
session, consent created a masked one-time credential. Absolute paths containing
spaces produced quoted commands. All three copy buttons reported success.
Browser-session clipboard inspection returned empty, so verification used actual
paste into the test form instead: the pasted configuration had the expected
version, origin, workspace/client binding and credential shape. No credential
was printed or persisted in evidence. A subsequent host-command copy/paste
contained only paths/profile configuration and replaced the clipboard secret.
The setup screenshot was inspected for readable text and horizontal command
overflow; no screenshot artifact is retained.

The connection was revoked through the UI and the form disappeared with a
persisted revoked receipt. The fixture, Core process and Next preview stopped,
and the temporary browser tab closed. No Claude configuration was installed in
the user's actual account and no model call or external charge occurred.

## Remaining limits

Not deployed or enabled in production. Actual Claude Code/claude.ai model proof
still requires the pending external-processing approval; remote OAuth/PKCE,
source authority, proposal storage/display and the separate human action gate
remain unfinished. No bead acceptance criterion is closed by this increment.

This is an OS-account boundary, not host application attestation. Other programs
with the same UID, the account owner and root are outside the isolation claim.
It is not atomic with a later remote data disclosure. Windows is unsupported.
Local Docker engine did not answer within 15 seconds, so Linux ACL behavior and
both Docker variants were not exercised locally. Linux CI now runs the Rust
tests (including a POSIX ACL case), Clippy and formatting before MCP checks.
Do not call that coverage passed until its actual result is available.

The macOS absent-ACL handling was corrected after failing positive-file tests.
[Apple's ACL implementation](https://raw.githubusercontent.com/apple-oss-distributions/Libc/main/posix1e/acl_file.c)
reads the descriptor's FILESEC_ACL property; the observed no-property case is
`ENOENT`. Explicit ACL entries still deny. Tests proved both cases.
