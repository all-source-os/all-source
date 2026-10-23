package main

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"

	"github.com/gin-gonic/gin"
)

type testEgressLedger struct {
	mu           sync.Mutex
	left         map[string]uint64
	reservations map[string]uint64
	settlements  []egressCommand
}

func (l *testEgressLedger) exchange(_ context.Context, tenant string, command egressCommand) (egressAdmission, error) {
	l.mu.Lock()
	defer l.mu.Unlock()
	var result egressAdmission
	result.Protocol = "funded-egress-v1"
	if command.Operation == "reserve" {
		if l.left[tenant] == 0 {
			return result, egressError{status: 402}
		}
		capacity := min(l.left[tenant], maxEgressResponse)
		l.left[tenant] -= capacity
		l.reservations[command.ID] = capacity
		result.Admission.ReservedBytes = capacity
	} else {
		capacity := l.reservations[command.ID]
		l.left[tenant] += capacity - command.Bytes
		delete(l.reservations, command.ID)
		l.settlements = append(l.settlements, command)
	}
	result.Admission.Snapshot.IncludedAvailable = l.left[tenant]
	return result, nil
}

func egressTestRouter(exchange egressExchange, handler gin.HandlerFunc) *gin.Engine {
	router := gin.New()
	cp := &ControlPlane{egressExchange: exchange}
	router.Use(func(c *gin.Context) { c.Set("auth_tenant_id", c.GetHeader("Test-Tenant")); c.Next() })
	api := router.Group("/api/v1")
	api.Use(cp.enforceEgress)
	api.GET("/events/query", handler)
	api.GET("/billing/egress", handler)
	return router
}

func TestEgressHardStopBeforeUpstreamAndIndependentTenant(t *testing.T) {
	ledger := &testEgressLedger{left: map[string]uint64{"a": 6, "b": 3}, reservations: map[string]uint64{}}
	calls := 0
	router := egressTestRouter(ledger.exchange, func(c *gin.Context) { calls++; c.String(200, "abc") })
	for _, sample := range []struct {
		tenant string
		status int
	}{{"a", 200}, {"a", 200}, {"a", 402}, {"b", 200}} {
		request := httptest.NewRequestWithContext(t.Context(), http.MethodGet, "/api/v1/events/query", http.NoBody)
		request.Header.Set("Test-Tenant", sample.tenant)
		response := httptest.NewRecorder()
		router.ServeHTTP(response, request)
		if response.Code != sample.status {
			t.Fatalf("status %d: %s", response.Code, response.Body.String())
		}
	}
	if calls != 3 {
		t.Fatalf("exhausted client reached upstream: %d calls", calls)
	}
}

func TestEgressOversizedResponseNeverLeaksPartialBody(t *testing.T) {
	ledger := &testEgressLedger{left: map[string]uint64{"a": 3}, reservations: map[string]uint64{}}
	router := egressTestRouter(ledger.exchange, func(c *gin.Context) { c.String(200, "private-response") })
	request := httptest.NewRequestWithContext(t.Context(), http.MethodGet, "/api/v1/events/query", http.NoBody)
	request.Header.Set("Test-Tenant", "a")
	response := httptest.NewRecorder()
	router.ServeHTTP(response, request)
	if response.Code != http.StatusPaymentRequired || strings.Contains(response.Body.String(), "private-response") {
		t.Fatalf("oversized body leaked: %d %s", response.Code, response.Body.String())
	}
	if len(ledger.settlements) != 1 || ledger.settlements[0].Bytes != 0 || ledger.left["a"] != 3 {
		t.Fatalf("failed response consumed allowance: %#v", ledger.settlements)
	}
}

func TestEgressUnavailableLedgerDoesNotReachUpstream(t *testing.T) {
	calls := 0
	router := egressTestRouter(func(context.Context, string, egressCommand) (egressAdmission, error) {
		return egressAdmission{}, egressError{status: 503}
	}, func(c *gin.Context) { calls++; c.String(200, "secret") })
	request := httptest.NewRequestWithContext(t.Context(), http.MethodGet, "/api/v1/events/query", http.NoBody)
	request.Header.Set("Test-Tenant", "a")
	response := httptest.NewRecorder()
	router.ServeHTTP(response, request)
	if response.Code != 503 || calls != 0 {
		t.Fatalf("fail-open ledger: status=%d calls=%d", response.Code, calls)
	}
}

func TestEgressSettlementFailureNeverReturnsData(t *testing.T) {
	router := egressTestRouter(func(_ context.Context, _ string, command egressCommand) (egressAdmission, error) {
		if command.Operation == "complete" {
			return egressAdmission{}, egressError{status: 503}
		}
		var result egressAdmission
		result.Admission.ReservedBytes = 100
		return result, nil
	}, func(c *gin.Context) { c.String(200, "private-response") })
	request := httptest.NewRequestWithContext(t.Context(), http.MethodGet, "/api/v1/events/query", http.NoBody)
	request.Header.Set("Test-Tenant", "a")
	response := httptest.NewRecorder()
	router.ServeHTTP(response, request)
	if response.Code != 503 || strings.Contains(response.Body.String(), "private-response") {
		t.Fatalf("unsettled response leaked: %d %s", response.Code, response.Body.String())
	}
}

func TestEgressRecoveryRoutesRemainReachable(t *testing.T) {
	for _, path := range []string{"/api/v1/auth/login", "/api/v1/billing/egress", "/api/v1/webhooks/lemonsqueezy", "/api/v1/cluster/health"} {
		if billableEgressPath(path) {
			t.Fatalf("recovery route requires credit: %s", path)
		}
	}
	for _, path := range []string{"/api/v1/events/query", "/api/v1/billing-evil/events"} {
		if !billableEgressPath(path) {
			t.Fatalf("data route bypasses budget: %s", path)
		}
	}
}

func TestEgressBillingRemainsReachableAfterExhaustion(t *testing.T) {
	router := egressTestRouter(func(context.Context, string, egressCommand) (egressAdmission, error) {
		t.Fatal("billing must remain reachable")
		return egressAdmission{}, nil
	}, func(c *gin.Context) { c.String(200, "usage and prepaid checkout") })
	response := httptest.NewRecorder()
	router.ServeHTTP(response, httptest.NewRequestWithContext(t.Context(), http.MethodGet, "/api/v1/billing/egress", http.NoBody))
	if response.Code != 200 {
		t.Fatalf("billing blocked: %d", response.Code)
	}
}
