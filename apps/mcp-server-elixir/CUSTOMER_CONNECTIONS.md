# Local customer review connection

This is the restricted profile of the existing Elixir MCP server. It currently
checks current workspace access and validates proposal syntax. It does not read
events, persist a review, approve an action or run a replay. Production issuance
remains disabled pending the complete customer delivery gates.

## Build and install

Building requires Elixir and Rust/Cargo. `mix compile` builds the app-owned Rust
connection utility from its locked dependencies. `MIX_ENV=prod mix release`
includes it both under the app's `priv/bin` and at
`bin/allsource-customer-connection` in the release. No other app's source is used.

In the product's connection settings, consent to the listed fields, create a
connection and enter the release directory and a new private connection filename.
Both must be absolute paths without symlinks. Choose a location outside shared
or synced project directories. The final parent directory may be missing; the
installer creates it with mode 0700. Earlier ancestors must already exist.

1. Copy the generated install command into a terminal, but do not run it yet.
2. Copy the connection data from the product. Return to the terminal and press
   Enter. The command reads the clipboard into the installer's stdin. The secret
   is not a shell argument, shell-history entry or environment variable.
3. Successful installation exits silently. It creates a new mode-0600 file and
   refuses to overwrite any existing filename. A failure never prints the
   credential or path. Check permissions and paths; choose a new filename for
   reconnects rather than overwriting a previous connection.
4. Copy and run the generated `claude mcp add-json --scope local` command. It
   contains only executable/configuration paths and the profile flag. Copying it
   also replaces the private clipboard data. Open Claude Code and check `/mcp`.

The clipboard command uses `pbpaste` on macOS, `wl-paste` on Linux Wayland or
`xclip` on Linux X11. The selected clipboard utility must already be installed.
This recipe follows [Claude Code's local MCP configuration interface](https://code.claude.com/docs/en/mcp).
It is not proof that the native host has completed a model-assisted workflow.

For automated local tests, the installer accepts one compact JSON line on stdin:

```text
bin/allsource-customer-connection --install /absolute/private-parent/connection.json
```

It refuses terminal input, malformed JSON, an empty/oversized input, and existing
files. Input must provide version 1, a URL string, token string and binding
object. Detailed schema and authority validation still happen on each tool call;
successful file installation alone is not successful authentication.

## Runtime configuration

The host sets only:

```json
{
  "ALLSOURCE_CUSTOMER_REVIEW": "true",
  "CUSTOMER_REVIEW_CONNECTION_FILE": "/absolute/private-parent/connection.json"
}
```

The private file contains exactly `version`, `url`, `token` and `binding`.
`version` is 1; `token` is the consent-bound `asreview_v2_` credential. `binding`
contains exactly `tenant_id`, `subject_id`, `client_id` and `resource`, as returned
by issuance. Only `claude-code` is accepted by this local profile.

The request destination must be the HTTPS resource's origin, without a path,
query, fragment or URL credentials. HTTP is accepted only for literal loopback
addresses in local test/development setups; the resource still remains HTTPS.
Redirects are not followed. Old `CUSTOMER_REVIEW_GRANT`, URL and binding
environment variables no longer authorize calls. There is no fallback.

## Owner boundary and limits

Every tool call launches the same packaged reader and reloads the file. Its
actual real and effective OS UIDs must match; environment usernames/UIDs are
irrelevant. Descriptor-relative traversal refuses symlinks and unsafe ancestor
ownership/write permissions. The immediate parent must belong to that UID, be
0700 and have no extended access ACL. The file must belong to that UID, be a
regular single-link file, be 0400 or 0600 with no special/execute bits or extended
access ACL, and contain at most 8 KiB. Metadata/ACL checks bracket the file read.

The Rust process has a three-second deadline; the Elixir caller waits at most
four seconds and bounds output to 8 KiB. FIFO/device reads do not block while
waiting for a writer. Invalid local configuration produces a fixed connection
error before any network request. Server-side membership, billing, expiry and
revocation checks remain independent and current on every call.

This protects the OS-account boundary. It does not attest a host application's
identity, isolate programs sharing a UID, stop the owner copying a credential,
or prevent privileged root access. Shared container UIDs are not separate users.
Windows is unsupported. macOS was exercised locally; Linux-specific ACL tests
and Docker packaging require the Linux CI/build result before release.
