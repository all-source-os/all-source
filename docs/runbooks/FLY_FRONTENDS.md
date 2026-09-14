# Fly.io Frontend Deployment

AllSource web and admin run as standalone Next.js containers in the `allsource`
Fly.io organization.

| Surface | Fly app | Public domain | Internal port | Health check |
|---|---|---|---:|---|
| Marketing + dashboard | `allsource-web` | `www.all-source.xyz` | 3000 | `/api/healthz` |
| Admin console | `allsource-admin` | `admin.all-source.xyz` | 3001 | `/api/healthz` |

The apex `all-source.xyz` is served by `allsource-web` and returns a permanent
redirect to `www.all-source.xyz`. `api.all-source.xyz` remains on
`allsource-control-plane`.

## Deploy

Run from the repository root:

```bash
flyctl deploy . --config apps/web/fly.toml --remote-only --ha=false
flyctl deploy . --config apps/admin/fly.toml --remote-only --ha=false
```

Both Dockerfiles produce Next.js standalone images. Public environment values
are build arguments in each `fly.toml`; server-only service URLs are runtime
environment variables. Web and admin use Fly private DNS for server-to-server
Control Plane and Query Service calls.

Optional web integrations require Fly secrets when enabled:

```bash
flyctl secrets set STATUS_MONITOR_TOKEN=... --app allsource-web
flyctl secrets set GITHUB_FEEDBACK_TOKEN=... --app allsource-web
flyctl secrets set ALLSOURCE_API_KEY=... --app allsource-web
```

Turnstile remains disabled until both public and secret production keys are
configured.

## DNS and TLS

Unstoppable Domains DNS is authoritative. Public records point directly to Fly:

| Name | Type | Value |
|---|---|---|
| `@` | A | `66.241.125.155` |
| `@` | AAAA | `2a09:8280:1::180:38b1:0` |
| `www` | A | `66.241.125.155` |
| `www` | AAAA | `2a09:8280:1::180:38b1:0` |
| `admin` | A | `66.241.124.175` |
| `admin` | AAAA | `2a09:8280:1::180:38b2:0` |
| `api` | A | `66.241.125.106` |
| `api` | AAAA | `2a09:8280:1::d4:42b8:0` |

Certificates are attached with:

```bash
flyctl certs add all-source.xyz --app allsource-web
flyctl certs add www.all-source.xyz --app allsource-web
flyctl certs add admin.all-source.xyz --app allsource-admin
flyctl certs add api.all-source.xyz --app allsource-control-plane
```

Preserve any mail records during DNS cutovers. User-facing status remains the
`/status` path on `www.all-source.xyz`; no separate `status` host is required.

## Verify

```bash
curl -fsS https://www.all-source.xyz/api/healthz
curl -fsSI https://all-source.xyz/
curl -fsS https://admin.all-source.xyz/api/healthz
curl -fsS https://www.all-source.xyz/sitemap.xml >/dev/null
curl -fsS https://www.all-source.xyz/robots.txt >/dev/null

flyctl checks list --app allsource-web
flyctl checks list --app allsource-admin
flyctl certs check www.all-source.xyz --app allsource-web
flyctl certs check admin.all-source.xyz --app allsource-admin
```

Expected health payloads identify `allsource-web` and `allsource-admin`.
Expected apex response is HTTP 308 with `Location: https://www.all-source.xyz/`.

## Rollback

Application rollback:

```bash
flyctl releases --app allsource-web
flyctl releases rollback <version> --app allsource-web

flyctl releases --app allsource-admin
flyctl releases rollback <version> --app allsource-admin
```

DNS rollback requires restoring prior Vercel project-domain attachments, then
changing the authoritative Unstoppable Domains records back to Vercel targets.
Keep Vercel projects undeleted until Fly has served production traffic
successfully through one full release cycle.
