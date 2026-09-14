# Runbook — Control Plane CORS

**Service:** `apps/control-plane` (Go/Gin, Fly app `allsource-control-plane`).
**Owner:** platform.
**TL;DR:** the Control Plane grants **credentialed** CORS only to an explicit allowlist of frontend origins. To let a new browser app (e.g. the admin panel) call the API with cookies, add its origin to `ALLOWED_FRONTEND_URLS` and redeploy. Never reflect arbitrary origins with credentials.

---

## 1. The policy (what the code does)

`corsMiddleware` (`apps/control-plane/main.go`) decides per request, by `Origin` header:

| Request `Origin` | `Access-Control-Allow-Origin` | `Access-Control-Allow-Credentials` | Effect |
|---|---|---|---|
| In the allowlist | echoes that exact origin | `true` + `Vary: Origin` | cookies / `Authorization` usable cross-site (web app, admin panel) |
| Not in the allowlist (or absent) | `*` | *(not set)* | public, **non-credentialed** reads only; browser refuses to send cookies or expose a credentialed response |

The allowlist is `allowedCORSOrigins()` (`apps/control-plane/oauth.go`), derived from the **same** frontend allowlist used for OAuth open-redirect protection (`getAllowedFrontendURLs()`): `FRONTEND_URL` plus comma-separated `ALLOWED_FRONTEND_URLS`, each reduced to `scheme://host`. One source of truth → adding an origin once enables both CORS and OAuth redirect for it.

### Why it's an allowlist, not a reflector (the security bug this fixed)

The previous middleware echoed **any** `Origin` with `Access-Control-Allow-Credentials: true`. That is a credential-theft / CSRF vector: any website a logged-in admin visited could make authenticated requests to the Control Plane **and read the responses** in the victim's browser, because the browser saw `Allow-Origin: <attacker>` + `Allow-Credentials: true`. The fix gates the credentialed grant on the allowlist; everything else gets only the non-credentialed `*`. Public unauthenticated endpoints still work from anywhere; nothing can make a **credentialed** request unless it is explicitly allowed.

---

## 2. Config

| Env | Meaning | Prod value (`fly.toml`) |
|---|---|---|
| `FRONTEND_URL` | the primary frontend; always allowed | `https://www.all-source.xyz` |
| `ALLOWED_FRONTEND_URLS` | additional allowed origins, comma-separated, **no trailing slash** | `https://all-source.xyz,https://admin.all-source.xyz` |

Local dev: with neither set, `FRONTEND_URL` defaults to `http://localhost:3000`. To run the admin app (`apps/admin`, default port 3001) against a local Control Plane, export `ALLOWED_FRONTEND_URLS=http://localhost:3001` before starting it.

---

## 3. How to allow a new browser origin

1. Add the exact origin (scheme + host, no path, no trailing slash) to `ALLOWED_FRONTEND_URLS` in `apps/control-plane/fly.toml` (version-controlled, preferred) **or** as a secret:
   ```
   fly secrets set ALLOWED_FRONTEND_URLS="https://all-source.xyz,https://admin.all-source.xyz,https://NEW.origin" --app allsource-control-plane
   ```
   (A `fly.toml` `[env]` edit ships on the next `fly deploy`; `fly secrets set` triggers its own restart.)
2. Redeploy the Control Plane so the new value is read at boot:
   ```
   fly deploy apps/control-plane --app allsource-control-plane
   ```
   (CORS allowlist is built once in `setupMiddleware` at startup — an env change needs a restart/redeploy, not just a config push.)
3. Verify (§5).

> Fly app hostnames are not production origins and are **not** allowlisted. Test previews against a non-prod Control Plane, or give the app a stable custom domain and allowlist that.

---

## 4. The admin panel (`admin.all-source.xyz`) — full cross-origin wiring

The admin app (`apps/admin`, Fly app `allsource-admin`) calls the Control Plane **client-side with `credentials: "include"`** and authenticates via an `admin_token` httpOnly cookie. For login to work cross-origin, all three must hold:

1. **Custom domain** `admin.all-source.xyz` on the `allsource-admin` Fly app — so it shares the `.all-source.xyz` parent with `api.all-source.xyz` and the auth cookie is sendable. (A `*.fly.dev` host cannot share cookies with the branded API.)
2. **CORS** — `admin.all-source.xyz` in `ALLOWED_FRONTEND_URLS` (this runbook, §3). ✅ shipped in `fly.toml`.
3. **OAuth** — the Google/GitHub OAuth apps must list the admin domain in their authorized redirect URIs / origins (external console; owner action).
4. **Admin `NEXT_PUBLIC_APP_URL`** — set `NEXT_PUBLIC_APP_URL=https://admin.all-source.xyz` in `apps/admin/fly.toml`, then redeploy. The admin OAuth proxy (`apps/admin/src/app/api/v1/auth/oauth/[...path]/route.ts` → `getPublicUrl()`) sends the Control Plane a `redirect_to` built from this var; if it is unset it falls through to the `*.fly.dev` URL, which is **not** in `ALLOWED_FRONTEND_URLS`, so the Control Plane callback falls back to `FRONTEND_URL` and the user lands on `https://www.all-source.xyz/dashboard` after login instead of the admin app. The Control Plane already honors a per-app `redirect_to` (`OAuthAuthorize`/`OAuthCallback` in `oauth.go`, validated by `isAllowedRedirectURL`) — it just needs the admin app to send the allowlisted value.

5. **Control Plane `OAUTH_COOKIE_DOMAIN`** — set `OAUTH_COOKIE_DOMAIN=.all-source.xyz` (shipped in `fly.toml`). The OAuth flow is **started** on `admin.all-source.xyz` (the admin app proxies it) but the provider **callback** lands on `FRONTEND_URL` (`www.all-source.xyz`, per `getOAuthCallbackBaseURL`). The short-lived `oauth_state` + `oauth_redirect_to` cookies were host-only, so they were set on `admin.all-source.xyz` and **not sent to the `www` callback** → CSRF-state check and `redirect_to` both lost → fallback to `www`. Setting the registrable parent domain shares them across every `*.all-source.xyz` subdomain (`oauthCookieDomain()` in `oauth.go`). Verify: `curl -sD - -o /dev/null "https://api.all-source.xyz/api/v1/auth/oauth/google?redirect_to=https://admin.all-source.xyz" | grep set-cookie` → cookies carry `Domain=all-source.xyz`.
6. **Admin BFF + `CONTROL_PLANE_INTERNAL_URL`** — the Control Plane authenticates with `Authorization: Bearer` **only** (it ignores cookies — `admin_middleware.go extractBearerToken`). The admin data clients (`src/lib/*-api.ts`) therefore must NOT call the CP cross-origin with just `credentials: "include"` (that 401s). They call **same-origin** `/api/v1/...` (their `getApiUrl()` client branch returns `""`), which lands on the server-side BFF proxy `apps/admin/src/app/api/v1/[...path]/route.ts`. That proxy reads the httpOnly `admin_token` cookie and forwards to the CP with the Bearer attached. On Fly, set `CONTROL_PLANE_INTERNAL_URL=http://allsource-control-plane.internal:3901` in `apps/admin/fly.toml`. `getControlPlaneUrl()` reads this var.

> **Tenants list empty / 502 after login?** A **502** on `/api/v1/admin/tenants` means the BFF can't reach the CP → check `CONTROL_PLANE_INTERNAL_URL` (symptom 6). A **200 with no rows** is a data/tenant-scope issue, not auth. A login bounce to `/login?error=not_admin` means the email isn't in `ADMIN_EMAILS` (`auth.go roleForEmail()`).

**Admin app full env checklist (Fly `allsource-admin`, production):** `NEXT_PUBLIC_API_URL=https://api.all-source.xyz` (server-side fallbacks), `NEXT_PUBLIC_APP_URL=https://admin.all-source.xyz` (OAuth `redirect_to`), `CONTROL_PLANE_INTERNAL_URL=http://allsource-control-plane.internal:3901` (private BFF target). **Control Plane (`allsource-control-plane` Fly):** `ALLOWED_FRONTEND_URLS` includes `https://admin.all-source.xyz`, `OAUTH_COOKIE_DOMAIN=.all-source.xyz`, and the operator's email in `ADMIN_EMAILS`.

> **Known not-wired:** the admin **`/monitoring`** pages (`metrics-api.ts`) call the **Query Service**, not the Control Plane, with different auth — they do NOT flow through the BFF and are out of scope for this wiring. Tenants / billing / security / fleet pages (CP-backed) work through the BFF.

Until (1)–(6) land, the admin app loads but **login or the dashboard data will not complete**. See [FLEET_HEALTH_RECOVERY.md](./FLEET_HEALTH_RECOVERY.md) for the surrounding feature.

---

## 5. Verify

```
# Allowlisted origin → echoes origin + credentials:
curl -sI -H "Origin: https://admin.all-source.xyz" https://api.all-source.xyz/api/v1/billing/catalog \
  | grep -i 'access-control-allow-'
# expect: Access-Control-Allow-Origin: https://admin.all-source.xyz
#         Access-Control-Allow-Credentials: true

# Unknown origin → "*", NO credentials header:
curl -sI -H "Origin: https://evil.example.com" https://api.all-source.xyz/api/v1/billing/catalog \
  | grep -i 'access-control-allow-'
# expect: Access-Control-Allow-Origin: *      (and NO Access-Control-Allow-Credentials)
```

Automated coverage: `apps/control-plane/cors_test.go` (`go test -run CORS .`) asserts allowlisted-gets-credentials, unknown-never-gets-credentials, preflight-204, and env-driven allowlist.

---

## 6. Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| Browser console: *"No 'Access-Control-Allow-Origin'… credentials mode 'include'"* | the app's origin is not in the allowlist | add it to `ALLOWED_FRONTEND_URLS` + redeploy (§3) |
| Login redirects but session never sticks | cookie can't be shared cross-site | the app must be on a `.all-source.xyz` subdomain (§4 step 1), not `*.vercel.app` |
| `curl` shows `Allow-Origin: *` for an origin you *did* add | env not reloaded | redeploy/restart — the allowlist is built at boot (§3 step 2) |
| OAuth returns "redirect not allowed" | origin missing from `getAllowedFrontendURLs()` | same `ALLOWED_FRONTEND_URLS` covers both CORS and OAuth redirect — add it once |

---

## 7. Invariants

- **Never** pair `Access-Control-Allow-Credentials: true` with a reflected/arbitrary or `*` origin. Credentialed = allowlisted exact origin only.
- CORS and OAuth-redirect share one allowlist (`getAllowedFrontendURLs`). Don't fork them.
- Adding an origin is config + redeploy, not a code change.
