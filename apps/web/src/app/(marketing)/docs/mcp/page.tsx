import { siteConfig } from "@/lib/config";
import { constructMetadata } from "@/lib/utils";

export const metadata = constructMetadata({
  title: "MCP Integration — Connect securely to production",
  description: `Understand ${siteConfig.name} MCP connector modes, exact tool counts, local setup, and current hosted-gateway release status.`,
  canonical: "/docs/mcp",
});

const GATEWAY = "https://api.all-source.xyz";
const IMAGE = "ghcr.io/all-source-os/allsource-mcp-server";
const IMAGE_EMBEDDED = "ghcr.io/all-source-os/allsource-mcp-server-embedded";
// Keep in step with apps/mcp-server-elixir/mix.exs on each release.
const VERSION = "0.22.0";

function CodeBlock({ children }: { children: string }) {
  return (
    <pre className="rounded-lg border border-border bg-muted/50 p-4 text-sm overflow-x-auto">
      <code>{children}</code>
    </pre>
  );
}

function Code({ children }: { children: React.ReactNode }) {
  return <code className="text-sm bg-muted px-1.5 py-0.5 rounded">{children}</code>;
}

const SERVERS = [
  {
    name: "allsource-mcp-server",
    what: "The 55-tool default event-store connector. Talks to a remote Core through the gateway over HTTPS.",
    when: "Local or self-hosted Core on stable 0.22.0. Hosted gateway after the 0.23.0 authorization fix is released.",
    transport: "stdio",
    install: "Docker image",
  },
  {
    name: "allsource-mcp-server-embedded",
    what: "Same toolset, with Core compiled in-process via a Rustler NIF. No network hop.",
    when: "Lowest latency, single machine, data on local disk. No gateway, so no tenant isolation.",
    transport: "stdio",
    install: "Docker image",
  },
  {
    name: "allsource-mcp",
    what: "Rust binary that reads Core's WAL and Parquet files directly off disk. No server needed.",
    when: "Local debugging against a data directory — inspecting a crashed or offline Core.",
    transport: "stdio",
    install: "cargo install",
  },
  {
    name: "prime-mcp",
    what: "Prime's 19 graph, vector, and recall tools — a different toolset, not the event-store one.",
    when: "Agent memory, code graphs, semantic recall. Runs alongside, not instead of, the above.",
    transport: "stdio + HTTP",
    install: "Docker image",
  },
];

const TOOL_COUNTS = [
  ["Default (remote, writes enabled)", "55", "—"],
  ["ALLSOURCE_READ_ONLY=true", "45", "Hides the 10 mutation tools"],
  ["+ ALLSOURCE_CONTROL_URL set", "64", "Adds 9 tenant / fleet-health tools"],
  ["+ ALLSOURCE_SYSTEM_ADMIN=true", "73", "Adds the 8 recovery tools + tenant_notice"],
];

export default function McpPage() {
  return (
    <div className="mx-auto w-full max-w-screen-md px-4 lg:px-8 py-24">
      <h1 className="text-3xl font-bold text-foreground sm:text-4xl mb-2">
        Connect AI agents over MCP
      </h1>
      <p className="text-lg text-muted-foreground mb-10">
        AllSource ships a Model Context Protocol (MCP) connector so AI agents — Claude Desktop,
        Cursor, or anything that speaks MCP — can ingest, query, and reason over your event store in
        natural language. This page separates local setup from hosted gateway access, lists exact
        tool counts, and records the current stable-release limitation before you copy a command.
      </p>

      <div className="mb-10 rounded-lg border border-amber-500/40 bg-amber-500/10 p-4">
        <p className="font-medium text-foreground">Hosted connector release status</p>
        <p className="mt-1 text-sm leading-relaxed text-muted-foreground">
          Stable image <Code>{VERSION}</Code> does not send the Authorization header required by the
          hosted gateway, so hosted calls return 401. The fix is merged for 0.23.0 but is not
          released. Use the authenticated REST API for hosted production until that image ships; use{" "}
          <Code>{VERSION}</Code> only with local or self-hosted Core.
        </p>
      </div>

      <div className="prose prose-neutral dark:prose-invert max-w-none space-y-10">
        {/* ── Which server ─────────────────────────────────────────────── */}
        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">
            Which MCP server do you want?
          </h2>
          <p className="text-muted-foreground leading-relaxed mb-4">
            Four things in the AllSource ecosystem speak MCP. Pick one before you copy any command —
            they are not interchangeable. All four speak <strong>stdio</strong>, so your client
            launches them as a subprocess rather than connecting to a URL. Only{" "}
            <Code>prime-mcp</Code> additionally exposes HTTP; the event-store connectors do not, so
            there is no <Code>/sse</Code> endpoint to point a client at.
          </p>
          <div className="space-y-3">
            {SERVERS.map((s) => (
              <div key={s.name} className="rounded-lg border border-border p-4">
                <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1 mb-1">
                  <code className="text-sm font-mono text-primary">{s.name}</code>
                  <span className="text-xs text-muted-foreground">
                    {s.install} · {s.transport}
                  </span>
                </div>
                <p className="text-sm text-muted-foreground mb-1">{s.what}</p>
                <p className="text-sm">
                  <span className="font-medium text-foreground">Use when: </span>
                  <span className="text-muted-foreground">{s.when}</span>
                </p>
              </div>
            ))}
          </div>
          <p className="text-muted-foreground leading-relaxed mt-4 text-sm">
            The rest of this page covers <Code>allsource-mcp-server</Code> — the default. For the
            disk-reading Rust binary see the{" "}
            <a
              href="https://github.com/all-source-os/all-source/blob/main/docs/guides/ALLSOURCE_MCP.md"
              className="text-foreground underline underline-offset-4 hover:opacity-80"
            >
              allsource-mcp guide
            </a>
            ; for Prime see{" "}
            <a
              href="/docs/prime/mcp"
              className="text-foreground underline underline-offset-4 hover:opacity-80"
            >
              Prime MCP
            </a>
            .
          </p>
        </section>

        {/* ── Security model ───────────────────────────────────────────── */}
        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">
            How a secure connection is shaped
          </h2>
          <p className="text-muted-foreground leading-relaxed mb-3">
            The MCP connector never talks to the database (Core) directly. Every request is mediated
            by the authenticated gateway at <Code>{GATEWAY}</Code>, which validates your API key,
            scopes the call to your tenant, and enforces quotas and rate limits. Core is
            internal-only and trusts any caller on its network — so it is never exposed to the
            public, and neither your agent nor the connector should ever point at it.
          </p>
          <CodeBlock>{`Claude Desktop / Cursor
        │  (MCP — stdio, connector runs as a subprocess)
        ▼
AllSource MCP connector            ← runs on YOUR machine / your infra
        │  HTTPS + Authorization: Bearer <serviceaccount key>
        ▼
${GATEWAY}  (gateway)               ← validates key, derives tenant_id
        │  internal network only
        ▼
AllSource Core (your tenant's events only)`}</CodeBlock>
          <p className="text-muted-foreground leading-relaxed mt-3">
            Three properties make this safe: the key is a{" "}
            <strong>least-privilege, tenant-scoped</strong> credential (a scoped key can only ever
            read/write its own tenant&apos;s events); the transport to the gateway is{" "}
            <strong>TLS</strong>; and the connector runs where <em>you</em> control it, so your key
            never leaves your environment.
          </p>
        </section>

        {/* ── Step 1: mint a key ───────────────────────────────────────── */}
        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">
            1. Mint a least-privilege API key
          </h2>
          <p className="text-muted-foreground leading-relaxed mb-3">
            An MCP agent needs to read and write events — nothing more. Mint a key with the{" "}
            <Code>serviceaccount</Code> role: it is granted read + write and is denied admin, tenant
            management, metrics, and schema administration. Do <strong>not</strong> use an admin JWT
            or an admin-role key as your connector credential.
          </p>
          <p className="text-muted-foreground leading-relaxed mb-3">
            Self-service (creates a tenant and returns a scoped key in one call):
          </p>
          <CodeBlock>{`curl -X POST ${GATEWAY}/api/v1/onboard/start \\
  -H "Content-Type: application/json" \\
  -d '{"name": "my-agent"}'

# →
# {
#   "tenant_id": "my-agent-a1b2c3",
#   "api_key": "eyJhbGciOiJIUzI1NiIs...",   ← store this in a secret, shown once
#   ...
# }`}</CodeBlock>
          <p className="text-muted-foreground leading-relaxed my-3">
            Already onboarded? Mint a fresh, role-scoped key with an admin token:
          </p>
          <CodeBlock>{`curl -X POST ${GATEWAY}/api/v1/teams/agent-keys \\
  -H "Authorization: Bearer $ADMIN_JWT" \\
  -H "Content-Type: application/json" \\
  -d '{"name": "claude-desktop", "role": "serviceaccount"}'`}</CodeBlock>
          <p className="text-muted-foreground leading-relaxed mt-3 text-sm">
            The role string is exactly <Code>serviceaccount</Code> — no underscore. A drifted{" "}
            <Code>service_account</Code> is rejected and every request silently 403s.
          </p>
        </section>

        {/* ── Step 2: run the connector ────────────────────────────────── */}
        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">
            2. Check hosted connector release status
          </h2>
          <p className="text-muted-foreground leading-relaxed mb-3">
            Do not run stable image <Code>{VERSION}</Code> against the hosted gateway. That release
            does not attach <Code>CORE_API_KEY</Code> to upstream requests, so valid hosted keys
            still receive 401. The Authorization fix is merged for 0.23.0 but has not been
            published.
          </p>
          <p className="text-muted-foreground leading-relaxed mb-3 text-sm">
            The connector image is an <strong>Enterprise (BSL 1.1)</strong> build, so the registry
            requires a login first — a GitHub token with <Code>read:packages</Code>:
          </p>
          <CodeBlock>{`gh auth token | docker login ghcr.io -u $(gh api user -q .login) --password-stdin`}</CodeBlock>
          <div className="mb-3" />
          <CodeBlock>{`# LOCAL / SELF-HOSTED CORE ONLY for ${VERSION}.
# stdio server: -i keeps stdin open, and there is no port to publish.
printf '%s\\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \\
  | docker run -i --rm \\
      -e CORE_URL=http://host.docker.internal:3900 \\
      ${IMAGE}:${VERSION}

# → {"jsonrpc":"2.0","id":1,"result":{...,"serverInfo":{"name":"allsource-mcp-elixir","version":"${VERSION}"}}}`}</CodeBlock>
          <p className="text-muted-foreground leading-relaxed mt-3">
            For hosted production today, call the gateway REST API with{" "}
            <Code>Authorization: Bearer $ALLSOURCE_API_KEY</Code>. Pin connector 0.23.0 only after
            it appears in the release registry and changelog.
          </p>
        </section>

        {/* ── Step 3: wire the client ──────────────────────────────────── */}
        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">
            3. Point your MCP client at a local connector
          </h2>
          <p className="text-muted-foreground leading-relaxed mb-3">
            Stable <Code>{VERSION}</Code> can drive local or self-hosted Core over stdio. Add this
            local-only configuration to <Code>claude_desktop_config.json</Code>:
          </p>
          <CodeBlock>{`{
  "mcpServers": {
    "allsource": {
      "command": "docker",
      "args": [
        "run", "-i", "--rm",
        "-e", "CORE_URL",
        "${IMAGE}:${VERSION}"
      ],
      "env": {
        "CORE_URL": "http://host.docker.internal:3900"
      }
    }
  }
}`}</CodeBlock>
          <p className="text-muted-foreground leading-relaxed mt-3">
            Passing <Code>-e CORE_URL</Code> with no value forwards it from the <Code>env</Code>{" "}
            block rather than baking it into the argument list. Keep this Core bound to a trusted
            local network: direct Core has no public-tenant authentication boundary.
          </p>
        </section>

        {/* ── Verify ───────────────────────────────────────────────────── */}
        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">
            4. Verify the connection is tenant-scoped
          </h2>
          <p className="text-muted-foreground leading-relaxed mb-3">
            Before trusting the agent, confirm the key reaches your data and only your data. The
            same credential against the gateway REST API:
          </p>
          <CodeBlock>{`curl "${GATEWAY}/api/v1/events/query?limit=1" \\
  -H "Authorization: Bearer $ALLSOURCE_API_KEY"
# → {"events": [ ... ], "count": N}   ← your tenant's events only`}</CodeBlock>
          <p className="text-muted-foreground leading-relaxed mt-3">
            Connector verification against hosted data must wait for 0.23.0. A scoped REST key
            cannot read another tenant&apos;s events — the gateway derives the tenant from the key,
            not from any field the caller supplies.
          </p>
        </section>

        {/* ── Registered client keys ───────────────────────────────────── */}
        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">
            Hosted reads without a stored bearer
          </h2>
          <p className="text-muted-foreground leading-relaxed mb-3">
            The Rust <Code>allsource-mcp</Code> binary has a <Code>--remote</Code> mode that reads
            views (<Code>get_latest_state</Code>, <Code>list_latest</Code>,{" "}
            <Code>list_changes</Code>) from the hosted gateway. It can authenticate with a
            registered client key instead of an API key: a platform operator registers your P-256
            public key on the tenant, and the binary signs a fresh 60-second ES256 assertion for
            every request. When these variables are set, <Code>ALLSOURCE_API_KEY</Code> is ignored.
          </p>
          <CodeBlock>{`{
  "mcpServers": {
    "allsource-remote": {
      "command": "allsource-mcp",
      "args": ["--remote", "${GATEWAY}"],
      "env": {
        "ALLSOURCE_CLIENT_SIGNING_KEY": "/path/to/client-key.pem",
        "ALLSOURCE_CLIENT_ID": "my-mcp-reader",
        "ALLSOURCE_TENANT_ID": "my-tenant",
        "ALLSOURCE_CLIENT_KID": "<kid from registration>"
      }
    }
  }
}`}</CodeBlock>
          <p className="text-muted-foreground leading-relaxed mt-3 text-sm">
            <Code>ALLSOURCE_ASSERTION_AUDIENCE</Code> is optional and defaults to{" "}
            <Code>{GATEWAY}</Code>. Register a separate read-only key for the connector. Keys minted
            by the old onboarding flow that were never registered stop working at the gateway&apos;s
            legacy-key sunset with <Code>401 legacy_key_sunset</Code>; dashboard keys are not
            affected. Key generation, registration and rotation:{" "}
            <a
              href="https://github.com/all-source-os/all-source/blob/main/docs/guides/REGISTERED_CLIENT_KEYS.md"
              className="text-foreground underline underline-offset-4 hover:opacity-80"
            >
              registered client keys guide
            </a>
            .
          </p>
        </section>

        {/* ── Efficiency ───────────────────────────────────────────────── */}
        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">Running it efficiently</h2>
          <p className="text-muted-foreground leading-relaxed mb-3">
            Every tool the connector advertises is described in your client&apos;s context window on
            every turn, so the toolset is a real running cost. How many you expose depends on
            configuration:
          </p>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-border text-left">
                  <th className="py-2 pr-4 font-medium text-foreground">Configuration</th>
                  <th className="py-2 pr-4 font-medium text-foreground">Tools</th>
                  <th className="py-2 font-medium text-foreground">Effect</th>
                </tr>
              </thead>
              <tbody>
                {TOOL_COUNTS.map(([cfg, count, effect]) => (
                  <tr key={cfg} className="border-b border-border/50">
                    <td className="py-2 pr-4 text-muted-foreground">{cfg}</td>
                    <td className="py-2 pr-4 font-mono text-foreground">{count}</td>
                    <td className="py-2 text-muted-foreground">{effect}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <ul className="space-y-2 text-muted-foreground leading-relaxed list-none pl-0 mt-4">
            {[
              [
                "Set ALLSOURCE_READ_ONLY=true unless the agent must write",
                "Drops the 10 mutation tools (ingest_event, delete_events, archive_events, import_events, clone_entity, merge_entities, split_entity, compact_storage, backup_create, backup_restore). Smaller context and a hard stop on accidental writes — a gated call returns a refusal rather than mutating.",
              ],
              [
                "Leave ALLSOURCE_CONTROL_URL unset for a single-tenant agent",
                "The 9 tenant and fleet-health tools only make sense for fleet operators. Unset is the default, and the tools stay hidden.",
              ],
              [
                "Use the embedded image when data is local",
                `${IMAGE_EMBEDDED} compiles Core in-process via a Rustler NIF, removing the HTTP hop entirely. No gateway means no auth and no tenant isolation, so use it only against your own local data.`,
              ],
              [
                "Explore cheaply before querying broadly",
                "quick_stats and sample_events cost far less than an unbounded query_events. get_query_advice suggests a better shape for a query, and infer_schema derives structure from existing events instead of you describing it.",
              ],
              [
                "Use sessions for multi-turn work",
                "start_session, refine_query, and get_session_context keep query state on the server, so a narrowing conversation does not re-send the full filter set every turn.",
              ],
              [
                "Always bound query_events",
                "Pass limit, and order=desc when you want the newest events. Results are ordered by (timestamp, version) ascending by default, so an unbounded query walks history from the beginning.",
              ],
            ].map(([title, desc]) => (
              <li key={title} className="rounded-lg border border-border p-4">
                <span className="block font-medium text-foreground mb-1">{title}</span>
                <span className="text-sm">{desc}</span>
              </li>
            ))}
          </ul>
        </section>

        {/* ── Versions ─────────────────────────────────────────────────── */}
        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">
            Staying on a current version
          </h2>
          <p className="text-muted-foreground leading-relaxed mb-3">
            Pin a version rather than tracking <Code>latest</Code>, so an agent&apos;s toolset
            cannot change under you mid-conversation. Published tags are{" "}
            <strong>unprefixed semver</strong> — <Code>{VERSION}</Code>, not <Code>v{VERSION}</Code>{" "}
            — alongside the moving <Code>latest</Code> and <Code>main</Code>, a major.minor tag (
            <Code>0.22</Code>), and <Code>sha-&lt;commit&gt;</Code>.
          </p>
          <p className="text-muted-foreground leading-relaxed mb-3">
            Ask the connector what it is. The <Code>initialize</Code> handshake reports the build in{" "}
            <Code>serverInfo.version</Code>:
          </p>
          <CodeBlock>{`printf '%s\\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \\
  | docker run -i --rm ${IMAGE}:${VERSION} 2>/dev/null \\
  | grep -o '"serverInfo":{[^}]*}'
# → "serverInfo":{"name":"allsource-mcp-elixir","version":"${VERSION}"}

# What is actually published:
docker run --rm gcr.io/go-containerregistry/crane ls ${IMAGE}`}</CodeBlock>
          <p className="text-muted-foreground leading-relaxed mt-3">
            To upgrade, bump the pinned tag in your client config and restart the client — the
            connector is stateless, so nothing migrates. Check the{" "}
            <a
              href="/changelog"
              className="text-foreground underline underline-offset-4 hover:opacity-80"
            >
              changelog
            </a>{" "}
            for tool additions or removals first; a removed tool is a breaking change for any prompt
            that names it.
          </p>
          <p className="text-muted-foreground leading-relaxed mt-3 text-sm">
            The Docker images track platform releases. The <Code>cargo install allsource-mcp</Code>{" "}
            binary versions independently and currently lags the platform — check{" "}
            <Code>allsource-mcp --version</Code> against{" "}
            <a
              href="https://crates.io/crates/allsource-mcp"
              className="text-foreground underline underline-offset-4 hover:opacity-80"
            >
              crates.io
            </a>{" "}
            before relying on a recent Core feature through it.
          </p>
        </section>

        {/* ── Hardening checklist ──────────────────────────────────────── */}
        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">
            Production hardening checklist
          </h2>
          <ul className="space-y-2 text-muted-foreground leading-relaxed list-none pl-0">
            {[
              [
                "Use the gateway, never Core",
                `Always set CORE_URL=${GATEWAY}. Core is internal-only and trusts any caller on its network — pointing an external connector at it bypasses auth, quotas, and tenant isolation entirely.`,
              ],
              [
                "serviceaccount role, least privilege",
                "Mint the connector key as serviceaccount (read + write). Reserve admin keys for humans; never hand an admin credential to an agent.",
              ],
              [
                "Read-only unless writes are required",
                "ALLSOURCE_READ_ONLY=true is the safer default and the cheaper one. Turn it off deliberately, for a connector that genuinely ingests.",
              ],
              [
                "Key in a secret, not in config (0.23.0+)",
                "After the hosted connector fix is released, inject CORE_API_KEY from an environment secret or secrets manager. Keep it out of images, compose files, claude_desktop_config.json, and git.",
              ],
              [
                "TLS only",
                "CORE_URL must be https://. The connector → gateway hop carries your key — never run it over plain http in production.",
              ],
              [
                "Pin the image tag",
                "Run a specific version, not latest, so the advertised toolset only changes when you choose. Review the changelog before bumping.",
              ],
              [
                "Rotate and revoke",
                "Rotate the key on a schedule and immediately if a machine running the connector is lost. Revoke from the dashboard; a revoked key fails closed.",
              ],
              [
                "One key per connector",
                "Give each connector / machine its own key so you can revoke a single one without taking down the rest, and so audit trails stay attributable.",
              ],
            ].map(([title, desc]) => (
              <li key={title} className="rounded-lg border border-border p-4">
                <span className="block font-medium text-foreground mb-1">{title}</span>
                <span className="text-sm">{desc}</span>
              </li>
            ))}
          </ul>
        </section>

        {/* ── Example tools ────────────────────────────────────────────── */}
        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">What the agent can do</h2>
          <p className="text-muted-foreground leading-relaxed mb-4">
            A representative slice of the toolset — all scoped to your tenant by the key:
          </p>
          <div className="space-y-3">
            {[
              { name: "query_events", desc: "Query events by type, time range, or entity" },
              { name: "ingest_event", desc: "Store a new event (hidden in read-only mode)" },
              { name: "quick_stats", desc: "Cheap event counts and store summary" },
              { name: "sample_events", desc: "Small representative sample without a full query" },
              {
                name: "get_query_advice",
                desc: "Suggests a better shape for a query you describe",
              },
              { name: "reconstruct_state", desc: "Rebuild an entity's state at a point in time" },
              { name: "semantic_search_events", desc: "Natural-language search over events" },
              { name: "register_schema", desc: "Register a JSON schema for event validation" },
              { name: "health_deep", desc: "Core health, replication, and system streams" },
            ].map((tool) => (
              <div
                key={tool.name}
                className="flex items-start gap-3 rounded-lg border border-border p-3"
              >
                <code className="text-sm font-mono text-primary shrink-0">{tool.name}</code>
                <span className="text-sm text-muted-foreground">{tool.desc}</span>
              </div>
            ))}
          </div>
          <p className="text-muted-foreground leading-relaxed mt-4 text-sm">
            Run <Code>tools/list</Code> against your own connector for the authoritative set — it
            reflects your gating, so it is the only count that matches what your agent sees.
          </p>
        </section>

        {/* ── Troubleshooting ──────────────────────────────────────────── */}
        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">Troubleshooting</h2>
          <ul className="space-y-2 text-muted-foreground leading-relaxed list-none pl-0">
            {[
              [
                "manifest unknown / pull failure",
                "Stale image name, or not logged in. The images are allsource-mcp-server and allsource-mcp-server-embedded; the older chronos-* names no longer exist. Tags are unprefixed semver, so 0.22.0 works and v0.22.0 does not. Enterprise images also need docker login ghcr.io with a read:packages token — an unauthenticated pull reports the manifest as unknown rather than as a permission error.",
              ],
              [
                "Client reports the server exited immediately",
                "Missing -i on docker run. This is a stdio server: without stdin held open it reads EOF and shuts down cleanly. There is no port to publish and no /sse endpoint to point a url at.",
              ],
              [
                "401 from the gateway",
                "No / invalid key. Confirm CORE_API_KEY is set in the connector's environment and the key hasn't been revoked or expired. Note that images up to and including 0.22.0 never sent the Authorization header at all, so a hosted Core 401s no matter what you configure — that fix lands in the next release.",
              ],
              [
                "403 on every call",
                "Role drift. The key's role must be the exact string serviceaccount (no underscore). Re-mint with the correct role.",
              ],
              [
                "Fewer tools than expected",
                "Gating, not a bug. Mutation tools need ALLSOURCE_READ_ONLY unset; tenant tools need ALLSOURCE_CONTROL_URL; recovery tools additionally need ALLSOURCE_SYSTEM_ADMIN=true.",
              ],
              [
                "A tool the docs mention isn't there",
                "Check tools/list on your connector rather than trusting a name — that list is authoritative for your version and configuration.",
              ],
              [
                "Empty results but data exists",
                "Wrong tenant or wrong base URL. Verify CORE_URL is the gateway and the key belongs to the tenant that owns the data — the gateway scopes by key, so a key from another tenant returns nothing, not an error.",
              ],
              [
                "Connector can't reach the gateway",
                `Check egress/TLS from the machine running the connector to ${GATEWAY}. Do not work around it by pointing CORE_URL at Core.`,
              ],
            ].map(([title, desc]) => (
              <li key={title} className="rounded-lg border border-border p-4">
                <span className="block font-medium text-foreground mb-1">{title}</span>
                <span className="text-sm">{desc}</span>
              </li>
            ))}
          </ul>
        </section>

        {/* ── Local dev contrast ───────────────────────────────────────── */}
        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">
            Local development (no auth — never for production)
          </h2>
          <p className="text-muted-foreground leading-relaxed mb-3">
            When you run the full AllSource stack locally, the connector points at your local Core
            with no key. This is convenient for development but has{" "}
            <strong>no authentication and no tenant isolation</strong> — only ever use it against a
            local Core, never against production:
          </p>
          <CodeBlock>{`# LOCAL ONLY — local Core, no auth. Do not use these values for production.
docker run -i --rm \\
  -e CORE_URL=http://host.docker.internal:3900 \\
  ${IMAGE}:${VERSION}`}</CodeBlock>
          <p className="text-muted-foreground leading-relaxed mt-3 text-sm">
            The difference between this and a secure production connection is exactly the two things
            above: <Code>CORE_URL</Code> pointed at the gateway instead of a local Core, and a
            scoped <Code>CORE_API_KEY</Code>.
          </p>
        </section>

        {/* ── Honest status note ───────────────────────────────────────── */}
        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">A note on hosted MCP</h2>
          <p className="text-muted-foreground leading-relaxed">
            Today you run the MCP connector yourself (locally or on your own infrastructure) and it
            reaches production through the gateway — there is no public, multi-tenant hosted MCP URL
            to point a client at directly. If you only need to store and query events and don&apos;t
            want to run a connector at all, the gateway REST API and the{" "}
            <a
              href="/docs"
              className="text-foreground underline underline-offset-4 hover:opacity-80"
            >
              SDKs
            </a>{" "}
            give you the same tenant-scoped access with the same <Code>serviceaccount</Code> key.
          </p>
        </section>
      </div>
    </div>
  );
}
