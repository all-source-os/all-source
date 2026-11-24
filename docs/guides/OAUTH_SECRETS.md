# OAuth & Secrets Configuration

How to configure Google and GitHub OAuth secrets for the AllSource stack.

---

## Required Secrets

The OAuth variables are read from the environment at runtime. The Control Plane reads the provider client IDs and secrets (`apps/control-plane/oauth.go`). The Query Service reads `SECRET_KEY_BASE` (`apps/query-service/config/runtime.exs:61-64`):

| Secret | Env Var | Source |
|--------|---------|--------|
| Google OAuth | `GOOGLE_CLIENT_ID` | [Google Cloud Console](https://console.cloud.google.com/apis/credentials) |
| Google OAuth | `GOOGLE_CLIENT_SECRET` | Same |
| GitHub OAuth | `GITHUB_CLIENT_ID` | [GitHub Developer Settings](https://github.com/settings/developers) |
| GitHub OAuth | `GITHUB_CLIENT_SECRET` | Same |
| JWT signing | `JWT_SECRET` | Generate with `mix phx.gen.secret` |
| Phoenix secret | `SECRET_KEY_BASE` | Generate with `mix phx.gen.secret` |
| Frontend URL | `FRONTEND_URL` | e.g. `https://all-source.xyz` |

OAuth is optional — if `GOOGLE_CLIENT_ID` or `GITHUB_CLIENT_ID` are not set, the corresponding provider returns `:unconfigured` and the login redirect fails gracefully.

---

## Local Development

Add to a `.env` file (already gitignored) or export in your shell:

```bash
export GOOGLE_CLIENT_ID="your-id.apps.googleusercontent.com"
export GOOGLE_CLIENT_SECRET="GOCSPX-..."
export GITHUB_CLIENT_ID="Ov23li..."
export GITHUB_CLIENT_SECRET="..."
export JWT_SECRET="$(mix phx.gen.secret)"
export SECRET_KEY_BASE="$(mix phx.gen.secret)"
export FRONTEND_URL="http://localhost:3000"
```

---

## Fly.io Production

As noted in `apps/query-service/fly.toml:4`, use `flyctl secrets set`:

```bash
flyctl secrets set \
  GOOGLE_CLIENT_ID="your-id.apps.googleusercontent.com" \
  GOOGLE_CLIENT_SECRET="GOCSPX-..." \
  GITHUB_CLIENT_ID="Ov23li..." \
  GITHUB_CLIENT_SECRET="..." \
  JWT_SECRET="$(mix phx.gen.secret)" \
  SECRET_KEY_BASE="$(mix phx.gen.secret)" \
  FRONTEND_URL="https://all-source.xyz" \
  --app allsource-query
```

Fly.io secrets are encrypted at rest and injected as env vars at runtime. They never appear in `fly.toml` or git.

---

## Docker Compose (Local Stack)

Pass secrets via `docker-compose.override.yml` (gitignored) or a `.env` file:

```yaml
# docker-compose.override.yml
services:
  allsource-query-service:
    environment:
      - GOOGLE_CLIENT_ID=${GOOGLE_CLIENT_ID}
      - GOOGLE_CLIENT_SECRET=${GOOGLE_CLIENT_SECRET}
      - GITHUB_CLIENT_ID=${GITHUB_CLIENT_ID}
      - GITHUB_CLIENT_SECRET=${GITHUB_CLIENT_SECRET}
      - JWT_SECRET=${JWT_SECRET}
      - SECRET_KEY_BASE=${SECRET_KEY_BASE}
      - FRONTEND_URL=http://localhost:3000
```

Then either export the vars in your shell or create a `.env` file in the repo root (gitignored).

---

## OAuth Callback URLs

When registering OAuth apps with Google and GitHub, set the authorized redirect URIs:

### Google Cloud Console

- **Production**: `https://all-source.xyz/api/v1/auth/oauth/google/callback`
- **Development**: `http://localhost:3000/api/v1/auth/oauth/google/callback`

### GitHub Developer Settings

- **Production**: `https://all-source.xyz/api/v1/auth/oauth/github/callback`
- **Development**: `http://localhost:3000/api/v1/auth/oauth/github/callback`

The callback base URL is the `FRONTEND_URL` env var (`getOAuthCallbackBaseURL` in `apps/control-plane/oauth.go`). The frontend proxies the callback to the Control Plane.

---

## How the OAuth Flow Works

```
Browser                Frontend (FRONTEND_URL)      Control Plane (3901)      Provider
  |-- GET /api/v1/auth/oauth/google ----------------->|                      |
  |<-- redirect to Google ---------------------------- |                      |
  |-- callback?code=... -->|                          |                      |
  |                        |-- proxy callback ------->|-- exchange code ---->|
  |                        |                          |<-- access_token -----|
  |                        |                          |-- fetch user info -->|
  |<-- redirect to FRONTEND_URL/api/auth/callback?token=JWT
```

1. `GET /api/v1/auth/oauth/:provider` on the Control Plane redirects to the provider authorization page
2. Provider redirects back to `/api/v1/auth/oauth/:provider/callback` on the frontend, which proxies it to the Control Plane
3. Control Plane exchanges the code for an access token with the provider
4. Control Plane fetches user info (email, name) from the provider API
5. Control Plane issues a JWT and redirects to `FRONTEND_URL/api/auth/callback?token=JWT&new_user=...`

---

## Related Files

- OAuth handlers: `apps/control-plane/oauth.go`
- Runtime config: `apps/query-service/config/runtime.exs` (lines 58-63)
- Control Plane handler: `apps/control-plane/auth.go` (line 211, `OAuthHandler`)
- Fly.io config: `apps/query-service/fly.toml`
