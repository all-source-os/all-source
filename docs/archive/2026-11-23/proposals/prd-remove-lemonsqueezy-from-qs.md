# PRD: Remove LemonSqueezy Billing Code from Query Service

## Overview
The Query Service (Elixir) has a LemonSqueezy webhook endpoint that only logs events and returns 200 — subscription management is already fully owned by the Control Plane (Go), which has its own `webhook_handler.go`, `webhook_lemonsqueezy.go`, and `lemon_squeezy_client.go`. The QS implementation is dead code that creates confusion about where billing lives and requires secrets that don't belong in the QS.

This PRD removes every trace of LemonSqueezy from the Query Service.

## Goals
- Remove all LemonSqueezy code, config, and env var references from the QS
- Eliminate the need to set `LEMON_SQUEEZY_*` secrets on the QS deployment
- Clarify that billing is exclusively a Control Plane concern

## Quality Gates

### Epic-Level (run once on epic completion)
- `quality-elixir-full` CI gate passes (covers `mix compile --warnings-as-errors`, `mix test`, `mix format --check-formatted`)

### Story-Level (checked per story)
- **Deletion stories:** Verify file no longer exists and `mix compile` succeeds
- **Config stories:** Verify no references remain via `grep -r lemon_squeezy apps/query-service/`

## User Stories

### US-001: Delete webhook controller and schema [Backend]
**Description:** As a developer, I want the LemonSqueezy webhook controller and schema removed so there's no dead billing code in the QS.

**Acceptance Criteria:**
- [ ] Delete `lib/query_service_ex_web/controllers/webhook_controller.ex`
- [ ] Delete `lib/query_service_ex_web/schemas/webhooks.ex`
- [ ] Delete `test/query_service_ex_web/controllers/webhook_controller_test.exs`
- [ ] `mix compile --warnings-as-errors` passes with no errors

Mark each item [x] as you complete it. Only close when all are checked.

### US-002: Remove webhook route from router [Backend]
**Description:** As a developer, I want the `/api/webhooks/lemonsqueezy` route removed from the router so requests 404 instead of hitting dead code.

**Acceptance Criteria:**
- [ ] Remove the `post "/lemonsqueezy", WebhookController, :lemonsqueezy` line from `router.ex`
- [ ] Remove any webhook pipeline/scope that becomes empty after this removal
- [ ] Remove `alias QueryServiceExWeb.WebhookController` if present in router
- [ ] `mix compile --warnings-as-errors` passes

Mark each item [x] as you complete it. Only close when all are checked.

### US-003: Remove LemonSqueezy config from all config files [Backend]
**Description:** As a developer, I want all `:lemon_squeezy` config keys removed from QS config files.

**Acceptance Criteria:**
- [ ] Remove `:lemon_squeezy` config block from `config/runtime.exs`
- [ ] Remove `:lemon_squeezy` config block from `config/dev.exs`
- [ ] Remove `:lemon_squeezy` config block from `config/test.exs`
- [ ] Check `config/config.exs` and `config/prod.exs` for any references and remove
- [ ] `mix compile --warnings-as-errors` passes
- [ ] `grep -r lemon_squeezy apps/query-service/config/` returns zero matches

Mark each item [x] as you complete it. Only close when all are checked.

### US-004: Remove LemonSqueezy references from Dockerfile and fly.toml [Integration]
**Description:** As a developer, I want all LemonSqueezy env var references removed from deployment config.

**Acceptance Criteria:**
- [ ] Remove any `LEMON_SQUEEZY_*` env vars or comments from `Dockerfile`
- [ ] Remove any `LEMON_SQUEEZY_*` references from `fly.toml` (already partially done — verify clean)
- [ ] `grep -ri lemon_squeezy apps/query-service/` returns zero matches across entire QS directory

Mark each item [x] as you complete it. Only close when all are checked.

## Functional Requirements
- FR-1: After removal, `POST /api/webhooks/lemonsqueezy` must return 404 (route does not exist)
- FR-2: No QS config key, env var, or source file may reference `lemon_squeezy` or `LEMON_SQUEEZY`
- FR-3: The QS must compile and all existing tests must pass after removal
- FR-4: The Control Plane's existing LemonSqueezy webhook handling is unmodified

## Non-Goals
- Adding a LemonSqueezy webhook endpoint to the Control Plane (it already has one)
- Changing the LemonSqueezy webhook URL in the LemonSqueezy dashboard (ops task, not code)
- Modifying any billing logic in the Control Plane

## Technical Considerations
- Files to delete: `webhook_controller.ex`, `webhooks.ex`, `webhook_controller_test.exs`
- Files to modify: `router.ex`, `runtime.exs`, `dev.exs`, `test.exs`, `Dockerfile`, `fly.toml`
- The router may have a `scope "/webhooks"` that becomes empty — remove the entire scope if so
- Control Plane webhook URL: `POST /api/v1/webhooks/lemonsqueezy` (already exists in `webhook_handler.go`)

## Success Metrics
- Zero references to `lemon_squeezy` or `LEMON_SQUEEZY` anywhere in `apps/query-service/`
- `quality-elixir-full` CI passes
- No `LEMON_SQUEEZY_*` secrets needed on the QS Fly deployment

## Open Questions
- When should the LemonSqueezy dashboard webhook URL be updated to point to the Control Plane? (Ops task, not in scope of this PRD)
