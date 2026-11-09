package main

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

func newTestServer(status int, body string) *httptest.Server {
	return httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(status)
		//nolint:errcheck // test server: client hangup during Write is not actionable here
		w.Write([]byte(body))
	}))
}

func testContext() context.Context { return context.Background() }

type queryCountingCore struct {
	clients.CoreClient
	mu      sync.Mutex
	queries int
}

func (c *queryCountingCore) QueryEvents(_ context.Context, _ clients.QueryEventsRequest) (*clients.QueryEventsResponse, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.queries++
	return &clients.QueryEventsResponse{}, nil
}

func TestCurrentServiceStatus_ColdCacheNeverQueriesCore(t *testing.T) {
	core := &queryCountingCore{}
	cp := &ControlPlane{coreClient: core, heartbeats: newHeartbeatEmitter(core, nil, nil)}

	if out := cp.currentServiceStatus(); len(out) != 0 {
		t.Errorf("cold cache: want no services, got %+v", out)
	}
	if core.queries != 0 {
		t.Errorf("core queries: got %d, want 0 — an unscoped tenant=system read loads that tenant into Core memory", core.queries)
	}
}

func TestProbeOnce_StatusAndBodyMatch(t *testing.T) {
	// Spin up a test server that returns different shapes depending on path.
	t.Run("200 with body match → healthy", func(t *testing.T) {
		h := newHeartbeatEmitter(nil, nil, nil)
		srv := newTestServer(200, `{"status":"healthy"}`)
		defer srv.Close()

		status, err := h.probeOnce(testContext(), probe{
			url:       srv.URL,
			bodyMatch: `"status":"healthy"`,
		})
		if status != "healthy" || err != "" {
			t.Errorf("got (%q, %q), want (healthy, )", status, err)
		}
	})

	t.Run("200 but body missing marker → unhealthy", func(t *testing.T) {
		h := newHeartbeatEmitter(nil, nil, nil)
		srv := newTestServer(200, `{"status":"degraded"}`)
		defer srv.Close()

		status, msg := h.probeOnce(testContext(), probe{
			url:       srv.URL,
			bodyMatch: `"status":"healthy"`,
		})
		if status != "unhealthy" {
			t.Errorf("status: got %q, want unhealthy", status)
		}
		if !strings.Contains(msg, "missing expected marker") {
			t.Errorf("error: got %q, want 'missing expected marker'", msg)
		}
	})

	t.Run("500 → unhealthy with http status in error", func(t *testing.T) {
		h := newHeartbeatEmitter(nil, nil, nil)
		srv := newTestServer(500, `internal server error`)
		defer srv.Close()

		status, msg := h.probeOnce(testContext(), probe{url: srv.URL})
		if status != "unhealthy" {
			t.Errorf("status: got %q, want unhealthy", status)
		}
		if !strings.Contains(msg, "http 500") {
			t.Errorf("error: got %q, want 'http 500'", msg)
		}
	})
}

func TestEmitterSnapshot_ReturnsLiveProbeResultsWithoutCore(t *testing.T) {
	// The whole point of the in-memory cache: snapshot must work even when
	// the Core client is nil / unreachable. This is the regression guard
	// for issue #160 (Core outage shouldn't take the status page down).
	h := newHeartbeatEmitter(nil, nil, nil)
	now := time.Date(2026, 4, 25, 18, 0, 0, 0, time.UTC)

	h.record("core", heartbeatSample{
		status:    "unhealthy",
		latencyMs: 5000,
		errMsg:    "dial tcp: lookup allsource-core.internal: no such host",
		at:        now.Add(-3 * time.Second),
	})
	h.record("control-plane", heartbeatSample{
		status:    "healthy",
		latencyMs: 4,
		at:        now.Add(-2 * time.Second),
	})

	out := h.snapshot(now)
	if len(out) != 2 {
		t.Fatalf("want 2 entries, got %d: %+v", len(out), out)
	}
	// Sorted by service name → control-plane first, then core.
	if out[0].Service != "control-plane" || out[0].Status != "healthy" {
		t.Errorf("control-plane: got %+v, want healthy", out[0])
	}
	if out[1].Service != "core" || out[1].Status != "unhealthy" {
		t.Errorf("core: got %+v, want unhealthy", out[1])
	}
	if out[1].Error == "" {
		t.Error("core: error message should be carried through to the projection")
	}
}

func TestEmitterSnapshot_FlipsToStaleAfterTTL(t *testing.T) {
	h := newHeartbeatEmitter(nil, nil, nil)
	now := time.Date(2026, 4, 25, 18, 0, 0, 0, time.UTC)

	// Recorded as healthy, but the sample is older than heartbeatTTL (25s).
	// snapshot must override "healthy" → "stale" so a stuck probe loop
	// can't masquerade as fresh.
	h.record("auth", heartbeatSample{
		status:    "healthy",
		latencyMs: 10,
		at:        now.Add(-45 * time.Second),
	})

	out := h.snapshot(now)
	if len(out) != 1 || out[0].Status != "stale" {
		t.Errorf("want stale entry for auth, got %+v", out)
	}
}

func TestShouldWriteBeat_FirstBeatForAServiceIsAlwaysWritten(t *testing.T) {
	h := newHeartbeatEmitter(nil, nil, nil)
	now := time.Date(2026, 10, 13, 9, 0, 0, 0, time.UTC)

	if !h.shouldWriteBeat("core", statusHealthy, now) {
		t.Error("a service with no recorded write must be written, or its history starts with a gap")
	}
}

func TestShouldWriteBeat_UnchangedStatusIsNotRewrittenEveryTick(t *testing.T) {
	h := newHeartbeatEmitter(nil, nil, nil)
	now := time.Date(2026, 10, 13, 9, 0, 0, 0, time.UTC)
	h.markBeatWritten("core", statusHealthy, now)

	for _, tick := range []time.Duration{heartbeatInterval, 5 * time.Minute, 59 * time.Minute} {
		if h.shouldWriteBeat("core", statusHealthy, now.Add(tick)) {
			t.Errorf("steady status rewritten after %s", tick)
		}
	}
}

func TestShouldWriteBeat_StatusChangeIsWrittenImmediately(t *testing.T) {
	h := newHeartbeatEmitter(nil, nil, nil)
	now := time.Date(2026, 10, 13, 9, 0, 0, 0, time.UTC)
	h.markBeatWritten("core", statusHealthy, now)

	if !h.shouldWriteBeat("core", statusUnhealthy, now.Add(heartbeatInterval)) {
		t.Error("a transition is the event; it must not wait for the keepalive")
	}
}

func TestShouldWriteBeat_KeepaliveReWritesAnUnchangedStatus(t *testing.T) {
	h := newHeartbeatEmitter(nil, nil, nil)
	now := time.Date(2026, 10, 13, 9, 0, 0, 0, time.UTC)
	h.markBeatWritten("core", statusHealthy, now)

	if !h.shouldWriteBeat("core", statusHealthy, now.Add(heartbeatKeepaliveInterval)) {
		t.Error("without a keepalive, 'nothing changed' is indistinguishable from 'nothing reported'")
	}
}

func TestShouldWriteBeat_EachServiceIsGatedIndependently(t *testing.T) {
	h := newHeartbeatEmitter(nil, nil, nil)
	now := time.Date(2026, 10, 13, 9, 0, 0, 0, time.UTC)
	h.markBeatWritten("core", statusHealthy, now)

	if !h.shouldWriteBeat("auth", statusHealthy, now) {
		t.Error("one service's write must not suppress another's first beat")
	}
}

// countingCore records IngestEvent calls and can be made to fail, so write
// gating can be exercised through probeAndEmit rather than only through
// shouldWriteBeat.
type countingCore struct {
	clients.CoreClient
	mu     sync.Mutex
	calls  int
	failed bool
}

func (c *countingCore) IngestEvent(_ context.Context, _ clients.IngestEventRequest) (*clients.IngestEventResponse, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.calls++
	if c.failed {
		return nil, errors.New("core unavailable")
	}
	return &clients.IngestEventResponse{}, nil
}

func (c *countingCore) callCount() int {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.calls
}

func TestProbeAndEmit_SteadyStatusWritesOnceAcrossManyProbes(t *testing.T) {
	srv := newTestServer(200, `{"status":"healthy"}`)
	defer srv.Close()

	core := &countingCore{}
	h := newHeartbeatEmitter(core, nil, nil)
	p := probe{name: "core", url: srv.URL}

	for i := 0; i < 5; i++ {
		h.probeAndEmit(testContext(), p)
	}

	if got := core.callCount(); got != 1 {
		t.Errorf("ingest calls: got %d, want 1 — a steady service must not write once per probe", got)
	}
	if len(h.snapshot(time.Now())) != 1 {
		t.Error("the in-memory gauge must still be updated on every probe")
	}
}

func TestProbeAndEmit_FailedWriteIsRetriedOnTheNextProbe(t *testing.T) {
	srv := newTestServer(200, `{"status":"healthy"}`)
	defer srv.Close()

	core := &countingCore{failed: true}
	h := newHeartbeatEmitter(core, nil, nil)
	p := probe{name: "core", url: srv.URL}

	h.probeAndEmit(testContext(), p)
	h.probeAndEmit(testContext(), p)

	if got := core.callCount(); got != 2 {
		t.Errorf("ingest calls: got %d, want 2 — a failed write must not be recorded as written", got)
	}
}
