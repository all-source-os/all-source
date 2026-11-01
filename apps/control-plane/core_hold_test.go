package main

import (
	"bytes"
	"context"
	"errors"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"strconv"
	"sync/atomic"
	"testing"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/go-resty/resty/v2"

	"github.com/allsource/control-plane/internal/domain/entities"
	"github.com/allsource/control-plane/internal/infrastructure/corehold"
)

// downCore reserves a loopback address with nothing listening on it, so a dial
// is refused until up starts serving there.
type downCore struct {
	addr string
	url  string
}

func newDownCore(t *testing.T) *downCore {
	t.Helper()
	listener, err := (&net.ListenConfig{}).Listen(t.Context(), "tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	addr := listener.Addr().String()
	if err := listener.Close(); err != nil {
		t.Fatal(err)
	}
	return &downCore{addr: addr, url: "http://" + addr}
}

func (d *downCore) up(t *testing.T, handler http.Handler) {
	t.Helper()
	listener, err := (&net.ListenConfig{}).Listen(context.Background(), "tcp", d.addr)
	if err != nil {
		t.Errorf("reopen %s: %v", d.addr, err)
		return
	}
	server := &http.Server{Handler: handler, ReadHeaderTimeout: time.Second}
	go func() { _ = server.Serve(listener) }() //nolint:errcheck // Serve returns ErrServerClosed on Close
	t.Cleanup(func() { _ = server.Close() })   //nolint:errcheck // test cleanup
}

func (d *downCore) upAfter(t *testing.T, delay time.Duration, handler http.Handler) {
	t.Helper()
	done := make(chan struct{})
	t.Cleanup(func() { <-done })
	go func() {
		defer close(done)
		time.Sleep(delay)
		d.up(t, handler)
	}()
}

func holdTestCP(t *testing.T, coreURL string) *ControlPlane {
	t.Helper()
	t.Cleanup(corehold.MarkUp)
	signer := func(userID, tenantID string, role entities.Role) (string, error) {
		return "test-token:" + userID + ":" + tenantID + ":" + string(role), nil
	}
	d, err := newDelegationClient(coreURL, coreURL, coreURL, signer, NewPooledHTTPClient())
	if err != nil {
		t.Fatal(err)
	}
	return &ControlPlane{delegation: d, client: resty.NewWithClient(NewPooledHTTPClient()).SetTimeout(5 * time.Second).SetBaseURL(coreURL)}
}

func ingestRequest(ctx context.Context) *http.Request {
	body := bytes.NewBufferString(`{"event_type":"order.created","entity_id":"o-1","payload":{"amount":10}}`)
	return httptest.NewRequestWithContext(ctx, http.MethodPost, "/api/v1/events", body)
}

func TestDelegationHoldsThroughRefusedDialUntilCoreIsUp(t *testing.T) {
	core := newDownCore(t)
	var seen atomic.Int32
	core.upAfter(t, 300*time.Millisecond, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if _, err := io.ReadAll(r.Body); err != nil {
			t.Error(err)
		}
		seen.Add(1)
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"event_id":"e-1","version":1}`)) //nolint:errcheck // test response
	}))
	cp := holdTestCP(t, core.url)

	w := callHandler(cp.ProxyIngestSingle, ingestRequest(t.Context()), "tenant-a")

	if w.Code != http.StatusOK {
		t.Fatalf("status %d: %s", w.Code, w.Body.String())
	}
	if got := seen.Load(); got != 1 {
		t.Fatalf("Core saw the write %d times, want 1", got)
	}
}

func TestDelegationRetriesNotReadyThenSucceeds(t *testing.T) {
	var seen atomic.Int32
	core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		if seen.Add(1) == 1 {
			w.Header().Set("Retry-After", "1")
			w.WriteHeader(http.StatusServiceUnavailable)
			_, _ = w.Write([]byte(`{"error":"not_ready"}`)) //nolint:errcheck // test response
			return
		}
		_, _ = w.Write([]byte(`{"event_id":"e-1","version":1}`)) //nolint:errcheck // test response
	}))
	t.Cleanup(core.Close)
	cp := holdTestCP(t, core.URL)

	w := callHandler(cp.ProxyIngestSingle, ingestRequest(t.Context()), "tenant-a")

	if w.Code != http.StatusOK || !bytes.Contains(w.Body.Bytes(), []byte("e-1")) {
		t.Fatalf("status %d: %s", w.Code, w.Body.String())
	}
	if got := seen.Load(); got != 2 {
		t.Fatalf("Core calls %d, want 2", got)
	}
}

func TestDelegationForwardsOtherServiceUnavailableWithoutRetry(t *testing.T) {
	var seen atomic.Int32
	core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		seen.Add(1)
		w.WriteHeader(http.StatusServiceUnavailable)
		_, _ = w.Write([]byte(`{"error":"overloaded"}`)) //nolint:errcheck // test response
	}))
	t.Cleanup(core.Close)
	cp := holdTestCP(t, core.URL)

	w := callHandler(cp.ProxyIngestSingle, ingestRequest(t.Context()), "tenant-a")

	if w.Code != http.StatusServiceUnavailable || w.Body.String() != `{"error":"overloaded"}` {
		t.Fatalf("status %d: %s", w.Code, w.Body.String())
	}
	if got := seen.Load(); got != 1 {
		t.Fatalf("Core calls %d, want 1", got)
	}
}

func TestDelegationDoesNotResendWriteAfterReset(t *testing.T) {
	var seen atomic.Int32
	core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if _, err := io.ReadAll(r.Body); err != nil {
			t.Error(err)
		}
		seen.Add(1)
		hijacker, ok := w.(http.Hijacker)
		if !ok {
			t.Error("response writer cannot hijack")
			return
		}
		conn, _, err := hijacker.Hijack()
		if err != nil {
			t.Error(err)
			return
		}
		if tcp, ok := conn.(*net.TCPConn); ok {
			_ = tcp.SetLinger(0) //nolint:errcheck // a failed linger still closes the connection
		}
		_ = conn.Close() //nolint:errcheck // the close is the reset under test
	}))
	t.Cleanup(core.Close)
	cp := holdTestCP(t, core.URL)

	w := callHandler(cp.ProxyIngestSingle, ingestRequest(t.Context()), "tenant-a")

	if w.Code != http.StatusBadGateway {
		t.Fatalf("status %d: %s", w.Code, w.Body.String())
	}
	if got := seen.Load(); got != 1 {
		t.Fatalf("Core received the write %d times, want exactly 1", got)
	}
}

func TestDelegationAnswersServiceUnavailableWhenCoreStaysDown(t *testing.T) {
	core := newDownCore(t)
	cp := holdTestCP(t, core.url)
	ctx, cancel := context.WithTimeout(t.Context(), 300*time.Millisecond)
	defer cancel()

	w := callHandler(cp.ProxyIngestSingle, ingestRequest(ctx), "tenant-a")

	if w.Code != http.StatusServiceUnavailable || w.Header().Get("Retry-After") == "" {
		t.Fatalf("status %d Retry-After %q: %s", w.Code, w.Header().Get("Retry-After"), w.Body.String())
	}
}

func apiKeyRouter(client *AuthClient) *gin.Engine {
	router := gin.New()
	router.Use(AuthMiddleware(client))
	router.GET("/api/v1/events/query", func(c *gin.Context) { c.String(http.StatusOK, "ok") })
	return router
}

func serveAPIKey(ctx context.Context, router *gin.Engine, key string) *httptest.ResponseRecorder {
	req := httptest.NewRequestWithContext(ctx, http.MethodGet, "/api/v1/events/query", http.NoBody)
	req.Header.Set("Authorization", "Bearer "+key)
	w := httptest.NewRecorder()
	router.ServeHTTP(w, req)
	return w
}

func TestAPIKeyWhileCoreIsDownIsServiceUnavailableNotUnauthorized(t *testing.T) {
	t.Cleanup(corehold.MarkUp)
	core := newDownCore(t)
	client := NewAuthClient("test-secret", core.url)
	ctx, cancel := context.WithTimeout(t.Context(), 300*time.Millisecond)
	defer cancel()

	w := serveAPIKey(ctx, apiKeyRouter(client), "ask_not_cached")

	if w.Code != http.StatusServiceUnavailable || w.Header().Get("Retry-After") == "" {
		t.Fatalf("status %d Retry-After %q: %s", w.Code, w.Header().Get("Retry-After"), w.Body.String())
	}
}

func TestAPIKeyHoldsUntilCoreAnswers(t *testing.T) {
	t.Cleanup(corehold.MarkUp)
	core := newDownCore(t)
	core.upAfter(t, 300*time.Millisecond, http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"id":"u-1","username":"u","role":"developer","tenant_id":"tenant-a"}`)) //nolint:errcheck // test response
	}))
	client := NewAuthClient("test-secret", core.url)

	w := serveAPIKey(t.Context(), apiKeyRouter(client), "ask_not_cached")

	if w.Code != http.StatusOK {
		t.Fatalf("status %d: %s", w.Code, w.Body.String())
	}
}

func TestAPIKeyRejectedByCoreStaysUnauthorized(t *testing.T) {
	core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusUnauthorized)
	}))
	t.Cleanup(core.Close)
	client := NewAuthClient("test-secret", core.URL)

	w := serveAPIKey(t.Context(), apiKeyRouter(client), "ask_bogus")

	if w.Code != http.StatusUnauthorized {
		t.Fatalf("status %d: %s", w.Code, w.Body.String())
	}
}

func TestEgressLeaseAcquireHoldsThroughShortRefusal(t *testing.T) {
	core := newDownCore(t)
	var seen atomic.Int32
	core.upAfter(t, 300*time.Millisecond, http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		seen.Add(1)
		w.Header().Set("Content-Type", "application/json")
		expires := time.Now().Add(2 * time.Minute).Unix()
		_, _ = w.Write([]byte(`{"protocol":"funded-egress-v1","admission":{"snapshot":{"included_available":0,"prepaid_available":0},"reserved_bytes":1073741824,"lease_expires_at":` + strconv.FormatInt(expires, 10) + `}}`)) //nolint:errcheck // test response
	}))
	cp := holdTestCP(t, core.url)
	book := newLeaseBook(cp.exchangeEgress)

	hold, err := book.acquire(t.Context(), "acme")

	if err != nil {
		t.Fatal(err)
	}
	if hold.bytes != maxEgressResponse {
		t.Fatalf("hold bytes %d", hold.bytes)
	}
	if got := seen.Load(); got != 1 {
		t.Fatalf("lease commands %d, want 1", got)
	}
}

func TestEgressExchangeDoesNotResendAfterReset(t *testing.T) {
	var seen atomic.Int32
	core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		seen.Add(1)
		hijacker, ok := w.(http.Hijacker)
		if !ok {
			t.Error("response writer cannot hijack")
			return
		}
		conn, _, err := hijacker.Hijack()
		if err != nil {
			t.Error(err)
			return
		}
		_ = conn.Close() //nolint:errcheck // the close is the failure under test
	}))
	t.Cleanup(core.Close)
	cp := holdTestCP(t, core.URL)

	_, err := cp.exchangeEgress(t.Context(), "acme", egressCommand{Operation: egressOperationLease, ID: "lease-1", Bytes: 1})

	var refusal egressError
	if err == nil || errors.As(err, &refusal) {
		t.Fatalf("got %#v", err)
	}
	if got := seen.Load(); got != 1 {
		t.Fatalf("ledger calls %d, want 1", got)
	}
}
