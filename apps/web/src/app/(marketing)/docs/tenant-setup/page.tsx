"use client";

import { buttonVariants, cn, Section } from "@allsource/ui";
import { Key, Shield, Terminal } from "lucide-react";
import Link from "next/link";
import { staticMotion as motion } from "@/components/ui/static-motion";

const codeOnboard = `# Self-service onboard — no bootstrap key needed, no admin access
# Creates a tenant + API key in one call

curl -X POST https://api.all-source.xyz/api/v1/onboard/start \\
  -H "Content-Type: application/json" \\
  -d '{
    "email": "agent@your-company.com",
    "name": "My Production App"
  }'

# Response:
# {
#   "api_key": "eyJhbGciOiJIUzI1NiIs...",
#   "tenant_id": "onboard-agent-at-your-company-com",
#   "tier": "free",
#   "events_quota": 100000,
#   "getting_started": { ... }
# }`;

const codeIngest = `# Use the API key from onboard to ingest events immediately

curl -X POST https://api.all-source.xyz/api/v1/events \\
  -H "Authorization: Bearer $API_KEY" \\
  -H "Content-Type: application/json" \\
  -d '{
    "event_type": "user.action",
    "entity_id": "user-123",
    "payload": { "action": "login", "source": "github" }
  }'

# Response: 200 OK with event ID`;

const codeQuery = `# Query your events back

curl "https://api.all-source.xyz/api/v1/events/query?\\
event_type=user.action&limit=10&sort=desc" \\
  -H "Authorization: Bearer $API_KEY"

# Response:
# { "events": [...], "count": 10 }`;

const codeAdminTenant = `# ADMIN ONLY: Create a named tenant (requires bootstrap key)
# Most agents should use /onboard/start instead

curl -X POST https://api.all-source.xyz/api/v1/tenants \\
  -H "Authorization: Bearer $BOOTSTRAP_API_KEY" \\
  -H "Content-Type: application/json" \\
  -d '{
    "id": "my-company-prod",
    "name": "My Company Production"
  }'`;

const codeScopedKey = `# ADMIN ONLY: Mint a scoped API key for a specific tenant
# The resulting key can only access this tenant's events

curl -X POST https://api.all-source.xyz/api/v1/teams/agent-keys \\
  -H "Authorization: Bearer $ADMIN_JWT" \\
  -H "Content-Type: application/json" \\
  -d '{
    "name": "prod-agent-readonly",
    "role": "readonly"
  }'

# Response includes the scoped API key
# Use this key in your agent — never the bootstrap key`;

const codeClientKey = `# On the service's machine: generate a P-256 key (keep the PEM secret)
openssl ecparam -name prime256v1 -genkey -noout \\
  | openssl pkcs8 -topk8 -nocrypt -out client-key.pem

# PLATFORM OPERATOR ONLY: register the public JWK on the tenant
curl -X POST https://api.all-source.xyz/api/v1/admin/tenants/$TENANT/client-keys \\
  -H "Authorization: Bearer $OPERATOR_SESSION" \\
  -H "Content-Type: application/json" \\
  -d '{
    "client_id": "my-service",
    "jwk": {"kty": "EC", "crv": "P-256", "x": "...", "y": "..."},
    "scopes": ["events:read", "events:write"]
  }'

# 201 response includes the kid (RFC 7638 thumbprint).
# The service signs a 60 s ES256 assertion per request with that kid.`;

const codeChronisConfig = `# Configure chronis CLI to sync with your tenant

# In your project's .chronis/config.toml:
[sync]
mode = "remote"
remote_url = "https://api.all-source.xyz"
api_key = "eyJhbGciOiJIUzI1NiIs..."  # Your scoped API key

# Then sync:
cn sync`;

function CodeBlock({ title, code }: { title: string; code: string }) {
  return (
    <motion.div
      className="overflow-hidden rounded-xl border bg-[#0c0c14]"
      initial={{ opacity: 0, y: 10 }}
      whileInView={{ opacity: 1, y: 0 }}
      viewport={{ once: true }}
    >
      <div className="flex items-center gap-2 border-b px-4 py-3">
        <div className="h-3 w-3 rounded-full bg-red-500" />
        <div className="h-3 w-3 rounded-full bg-yellow-500" />
        <div className="h-3 w-3 rounded-full bg-green-500" />
        <span className="ml-2 text-xs text-muted-foreground">{title}</span>
      </div>
      <pre className="overflow-x-auto p-5 text-sm leading-relaxed text-gray-300">
        <code>{code}</code>
      </pre>
    </motion.div>
  );
}

export default function TenantSetupPage() {
  return (
    <div className="pt-24">
      {/* Hero */}
      <Section className="pb-8">
        <motion.div
          className="mx-auto max-w-3xl text-center"
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.6 }}
        >
          <div className="mb-4 inline-flex items-center gap-2 rounded-full border border-primary/30 bg-primary/10 px-4 py-1.5 text-sm text-primary">
            <Key className="h-4 w-4" />
            Docs
          </div>
          <h1 className="text-4xl font-bold tracking-tight sm:text-5xl">Tenant setup guide</h1>
          <p className="mx-auto mt-6 max-w-2xl text-lg text-muted-foreground">
            How to create tenants, get API keys, and start ingesting events. Two paths: self-service
            (one API call) or admin-managed (named tenants with scoped keys).
          </p>
        </motion.div>
      </Section>

      {/* Path 1: Self-service */}
      <Section className="py-12">
        <div className="mx-auto max-w-3xl space-y-8">
          <motion.div
            initial={{ opacity: 0, y: 10 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: true }}
          >
            <div className="flex items-center gap-3 mb-4">
              <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-green-500/10">
                <Terminal className="h-5 w-5 text-green-500" />
              </div>
              <div>
                <h2 className="text-2xl font-bold">Path 1: Self-service onboard</h2>
                <p className="text-sm text-muted-foreground">
                  For agents, scripts, and automated setups — no admin access needed
                </p>
              </div>
            </div>
            <div className="rounded-xl border border-green-500/20 bg-green-500/5 p-4 mb-6">
              <p className="text-sm text-foreground">
                <strong>This is the recommended path for LLMs and agents.</strong> One POST creates
                a tenant and returns an API key. No bootstrap key, no OAuth, no dashboard. The agent
                is ready to ingest events immediately.
              </p>
            </div>
          </motion.div>

          <div className="space-y-6">
            <div>
              <h3 className="text-lg font-semibold mb-3">Step 1: Create tenant + get API key</h3>
              <CodeBlock title="onboard.sh" code={codeOnboard} />
              <p className="mt-3 text-sm text-muted-foreground">
                The tenant ID is derived from the email. The API key is a long-lived JWT with
                developer role — it can read and write events, manage schemas, and create
                projections. New tenants start on a 14-day trial.
              </p>
            </div>

            <div>
              <h3 className="text-lg font-semibold mb-3">Step 2: Ingest your first event</h3>
              <CodeBlock title="ingest.sh" code={codeIngest} />
            </div>

            <div>
              <h3 className="text-lg font-semibold mb-3">Step 3: Query events back</h3>
              <CodeBlock title="query.sh" code={codeQuery} />
            </div>

            <div className="rounded-xl border p-4">
              <p className="text-sm text-muted-foreground">
                <strong>That&apos;s it.</strong> Three curl commands: onboard → ingest → query. No
                configuration files, no dashboard clicks, no waiting for approval. The API key works
                immediately.
              </p>
            </div>
          </div>
        </div>
      </Section>

      {/* Path 2: Admin-managed */}
      <Section className="py-12">
        <div className="mx-auto max-w-3xl space-y-8">
          <motion.div
            initial={{ opacity: 0, y: 10 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: true }}
          >
            <div className="flex items-center gap-3 mb-4">
              <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-orange-500/10">
                <Shield className="h-5 w-5 text-orange-500" />
              </div>
              <div>
                <h2 className="text-2xl font-bold">Path 2: Admin-managed tenants</h2>
                <p className="text-sm text-muted-foreground">
                  For production environments where you need named tenants and scoped keys
                </p>
              </div>
            </div>
            <div className="rounded-xl border border-orange-500/20 bg-orange-500/5 p-4 mb-6">
              <p className="text-sm text-foreground">
                <strong>
                  Use this when you need control over tenant naming, key scoping, and rotation.
                </strong>{" "}
                Requires the bootstrap API key (Fly secret on allsource-core) or OAuth admin access
                via the Control Plane dashboard.
              </p>
            </div>
          </motion.div>

          <div className="space-y-6">
            <div>
              <h3 className="text-lg font-semibold mb-3">Step 1: Create a named tenant</h3>
              <CodeBlock title="create-tenant.sh" code={codeAdminTenant} />
              <p className="mt-3 text-sm text-muted-foreground">
                The bootstrap key is a Fly secret on allsource-core. It has god-mode access to all
                tenants. <strong>Never embed it in application code or agent configs.</strong>
              </p>
            </div>

            <div>
              <h3 className="text-lg font-semibold mb-3">Step 2: Mint a scoped API key</h3>
              <CodeBlock title="mint-key.sh" code={codeScopedKey} />
              <p className="mt-3 text-sm text-muted-foreground">
                Scoped keys can only access their own tenant&apos;s events. Roles:{" "}
                <code>admin</code>, <code>developer</code>, <code>readonly</code>,{" "}
                <code>serviceaccount</code>. Use the narrowest role that fits —{" "}
                <code>readonly</code> for dashboards, <code>developer</code> for agents that write
                events.
              </p>
            </div>

            <div>
              <h3 className="text-lg font-semibold mb-3">Step 3: Configure your agent or CLI</h3>
              <CodeBlock title=".chronis/config.toml" code={codeChronisConfig} />
            </div>

            <div>
              <h3 className="text-lg font-semibold mb-3">
                Optional: Register a client key (ES256) for a long-running service
              </h3>
              <CodeBlock title="register-client-key.sh" code={codeClientKey} />
              <p className="mt-3 text-sm text-muted-foreground">
                A registered client key replaces a stored bearer: the service keeps the private key
                and signs a fresh assertion for every request, so nothing long-lived travels on the
                wire. Registration is done by a platform operator, never by a tenant credential. The
                role is always <code>serviceaccount</code> and the scopes come from the
                registration. The Rust, TypeScript, Python and Go SDKs and{" "}
                <code>allsource-mcp --remote</code> sign the assertion for you. Full steps,
                including computing the kid and rotating:{" "}
                <a
                  href="https://github.com/all-source-os/all-source/blob/main/docs/guides/REGISTERED_CLIENT_KEYS.md"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-foreground underline underline-offset-4 hover:opacity-80"
                >
                  registered client keys guide
                </a>
                .
              </p>
            </div>
          </div>
        </div>
      </Section>

      {/* Best practices */}
      <Section className="py-12">
        <div className="mx-auto max-w-3xl">
          <h2 className="text-2xl font-bold mb-6">Best practices</h2>
          <div className="space-y-4">
            {[
              {
                title: "Never use the bootstrap key in application code",
                desc: "The bootstrap key is god-mode. Use it once to create the tenant and mint a scoped key, then store the scoped key in your app. If the bootstrap key leaks, rotate it via `fly secrets set` — your scoped keys keep working.",
              },
              {
                title: "Use the onboard endpoint for automated setups",
                desc: "If your agent just needs to store and query events, /api/v1/onboard/start is the simplest path. One call, no bootstrap key needed, works immediately. New tenants start on a 14-day trial — pick a plan before it ends to keep ingesting.",
              },
              {
                title: "Scope keys to the narrowest role",
                desc: "Agents that only read events should use a `readonly` key. Agents that write events should use `developer`. Only human operators need `admin`. The RBAC system has 4 roles and 7 permissions — use them.",
              },
              {
                title: "Rotate keys, don't share them",
                desc: "Each agent or service should have its own API key. If one is compromised, revoke it via the Control Plane dashboard or API without affecting other agents. Keys are cheap — create as many as you need.",
              },
              {
                title: "Move old onboarding keys before the sunset",
                desc: "Keys minted by the old onboarding flow that were never registered stop working at the gateway's legacy-key sunset date and get 401 with reason legacy_key_sunset. Dashboard keys are not affected. Replace an old onboarding key with a dashboard key or a registered client key before that date.",
              },
              {
                title: "Use api.all-source.xyz, not Core directly",
                desc: "The gateway (api.all-source.xyz) handles auth, rate limiting, quotas, and x402 payments. Core is internal-only and trusts any caller on the network. Always route external traffic through the gateway.",
              },
            ].map((practice) => (
              <div key={practice.title} className="rounded-xl border p-5">
                <h3 className="font-semibold mb-1">{practice.title}</h3>
                <p className="text-sm text-muted-foreground">{practice.desc}</p>
              </div>
            ))}
          </div>
        </div>
      </Section>

      {/* Quick reference */}
      <Section className="py-12">
        <div className="mx-auto max-w-3xl">
          <h2 className="text-2xl font-bold mb-6">Quick reference</h2>
          <div className="overflow-hidden rounded-xl border">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b bg-muted/30">
                  <th className="px-4 py-3 text-left font-medium">Endpoint</th>
                  <th className="px-4 py-3 text-left font-medium">Auth</th>
                  <th className="px-4 py-3 text-left font-medium">What it does</th>
                </tr>
              </thead>
              <tbody>
                {[
                  {
                    endpoint: "POST /api/v1/onboard/start",
                    auth: "None",
                    desc: "Self-service: create tenant + API key",
                  },
                  {
                    endpoint: "POST /api/v1/tenants",
                    auth: "Bootstrap key",
                    desc: "Admin: create named tenant",
                  },
                  {
                    endpoint: "POST /api/v1/teams/agent-keys",
                    auth: "Admin JWT",
                    desc: "Admin: mint scoped API key",
                  },
                  {
                    endpoint: "POST /api/v1/admin/tenants/:id/client-keys",
                    auth: "Operator session",
                    desc: "Operator: register a client public key (ES256)",
                  },
                  {
                    endpoint: "GET /api/v1/admin/tenants/:id/client-keys",
                    auth: "Operator session",
                    desc: "Operator: list a tenant's client keys",
                  },
                  {
                    endpoint: "DELETE /api/v1/admin/tenants/:id/client-keys/:kid",
                    auth: "Operator session",
                    desc: "Operator: revoke a client key (permanent)",
                  },
                  {
                    endpoint: "POST /api/v1/events",
                    auth: "API key or client assertion",
                    desc: "Ingest event",
                  },
                  {
                    endpoint: "GET /api/v1/events/query",
                    auth: "API key or client assertion",
                    desc: "Query events",
                  },
                  { endpoint: "GET /health", auth: "None", desc: "Health check" },
                  {
                    endpoint: "GET /x402/routes",
                    auth: "None",
                    desc: "x402 priced route discovery",
                  },
                ].map((row) => (
                  <tr key={row.endpoint} className="border-b last:border-0">
                    <td className="px-4 py-3 font-mono text-xs text-primary">{row.endpoint}</td>
                    <td className="px-4 py-3 text-muted-foreground">{row.auth}</td>
                    <td className="px-4 py-3">{row.desc}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </Section>

      {/* CTA */}
      <Section className="py-16 text-center">
        <h2 className="text-3xl font-bold">Ready to connect?</h2>
        <p className="mx-auto mt-4 max-w-lg text-muted-foreground">
          One API call to onboard. Three commands to go from zero to events.
        </p>
        <div className="mt-8 flex items-center justify-center gap-4">
          <Link href="/signup" className={cn(buttonVariants({ variant: "default", size: "lg" }))}>
            Start 14-day trial
          </Link>
          <Link href="/docs/api" className={cn(buttonVariants({ variant: "outline", size: "lg" }))}>
            Full API reference
          </Link>
        </div>
      </Section>
    </div>
  );
}
