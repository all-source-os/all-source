# x402 Auto-Pay Production Readiness Plan

## Overview

Four tasks to close all identified gaps in the x402 auto-pay feature.
Execution order: Task 1 → Task 2 → (Task 3 and Task 4 independently).

---

## Task 1 — Core Code Fixes

### 1a. `RemoteFacilitator.post()` — non-2xx HTTP error handling
- **File:** `internal/infrastructure/x402/remote_facilitator.go`
- Add status check after `defer httpResp.Body.Close()` — read body and return
  `fmt.Errorf("facilitator %s: HTTP %d: %s", path, code, body)` for non-2xx
- Add `"io"` import
- Add explicit comment on the existing 30s timeout documenting it as hung-request protection

### 1b. `RegisterAgentUseCase` — surface `storeCDPWallet` failure + write failure event
- **File:** `internal/application/usecases/register_agent.go`
- Change `storeCDPWallet` to return `error` (currently discards with `_ =`)
- On store failure: zero out `walletAddress` in response (never return an address the lookup
  won't find), call `writeProvisionFailureEvent`
- On `CreateWallet` failure: call `writeProvisionFailureEvent`
- Add `writeProvisionFailureEvent`: writes `agent.cdp_wallet_provision_failed` event to Core
  with `reason` payload

### 1c. CDP client retry on transient failures
- **File:** `internal/infrastructure/clients/cdp_client.go`
- Promote `cenkalti/backoff/v5` from indirect to direct (already in `go.sum`)
- Wrap `cdpDo` HTTP call in `backoff.RetryNotify` — max 3 attempts, 10s total, exponential backoff
- Retry only on 429 and 5xx; use `backoff.Permanent(err)` for 4xx (except 429)
- Run `go mod tidy` after

**Gate:** `make quality-go` must pass before Task 2.

---

## Task 2 — Unit Tests

### 2a. `RemoteFacilitator` test file (new)
- **File:** `internal/infrastructure/x402/remote_facilitator_test.go`
- `newTestRemoteFacilitator(t, handler)` helper using `httptest.NewServer`
- Tests: Verify happy path, Verify non-2xx → error with status code, Settle happy path,
  Settle non-2xx → error, Timeout (50ms override, 200ms sleep server)

### 2b. `RegisterAgentUseCase.WithCDP()` tests (extend existing file)
- **File:** `internal/application/usecases/register_agent_test.go`
- Add `cdpTestCoreClient` (overrides `IngestEvent` + `SetConfig`, tracks calls)
  and `mockCDPProvisioner`
- 4 new cases:
  1. CDP happy path — wallet provisioned, stored, address in response
  2. `CreateWallet` fails — empty wallet_address, provision failure event written
  3. `SetConfig` fails — wallet_address suppressed, failure event written
  4. No CDP — no wallet, no failure event

**Gate:** `make quality-go` must pass before Task 3.

---

## Task 3 — Integration Test Skeleton

- **File:** `internal/infrastructure/x402/integration_test.go`
- Build tag: `//go:build integration` (skipped by standard `./...` runs)
- Skips unless `COINBASE_CDP_KEY_NAME` is set
- Test 1: Create Sepolia wallet — assert ID + address, log address for manual USDC funding
- Test 2: Hit real `x402.org/facilitator` with invalid payment — assert `IsValid == false`
  (proves connectivity without needing valid USDC balance)
- Header comment: required env vars + run command

---

## Task 4 — Documentation

- **File:** `docs/X402_SETUP.md`
- New section "Using Base Sepolia (testnet)":
  - `COINBASE_CDP_NETWORK=base-sepolia`, `BASE_RPC_URL=https://sepolia.base.org`
  - Pricing config with `"networks": ["eip155:84532"]` and Sepolia USDC contract
  - Circle faucet link for test USDC
- New section "End-to-end staging validation" — 6-step procedure:
  1. Deploy with Sepolia env vars
  2. `POST /api/v1/agents/register` → copy `wallet_address`
  3. Fund via Circle faucet
  4. Make request that exceeds quota → observe `200 OK` + `X-Payment-Response`
  5. Inspect settlement in Core: `GET /api/v1/events/query?event_type=x402.payment.settled`
  6. Verify `agent.registered` event includes `wallet_address`
- New section "Operational notes — Nonce deduplication":
  - Per-process in-memory tracker; 5-min `ValidBefore` window bounds replay risk
  - Multi-instance: share nonce state via Redis or rely on facilitator's server-side dedup

---

## Dependency Order

```
Task 1 (code fixes)
    └─→ Task 2 (tests for fixed code)
              ├─→ Task 3 (integration skeleton, independent)
              └─→ Task 4 (docs, independent)
```
