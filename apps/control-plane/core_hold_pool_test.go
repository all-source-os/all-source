package main

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strconv"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/infrastructure/corehold"
)

func markCoreDown(t *testing.T) {
	t.Helper()
	ctx, cancel := context.WithCancel(t.Context())
	cancel()
	corehold.Hold(ctx, func() bool { return true })
	t.Cleanup(corehold.MarkUp)
}

func TestReadsWaitingForCoreDoNotFillTheResponsePool(t *testing.T) {
	ledger := &testEgressLedger{left: map[string]uint64{"acme": 1 << 40}, reservations: map[string]uint64{}}
	coreBack := make(chan struct{})
	router := egressTestRouter(ledger.exchange, func(c *gin.Context) {
		<-coreBack
		c.String(http.StatusOK, "{}")
	})
	markCoreDown(t)

	readers := 2 * cap(egressResponseSlots)
	codes := make([]int, readers)
	var started sync.WaitGroup
	var done sync.WaitGroup
	for i := range readers {
		started.Add(1)
		done.Add(1)
		go func() {
			defer done.Done()
			req := httptest.NewRequestWithContext(t.Context(), http.MethodGet, "/api/v1/events/query?entity_id=e-"+strconv.Itoa(i), http.NoBody)
			req.Header.Set("Test-Tenant", "acme")
			w := httptest.NewRecorder()
			started.Done()
			router.ServeHTTP(w, req)
			codes[i] = w.Code
		}()
	}
	started.Wait()
	time.Sleep(300 * time.Millisecond)
	corehold.MarkUp()
	close(coreBack)
	done.Wait()

	for i, code := range codes {
		if code != http.StatusOK {
			t.Fatalf("reader %d got %d, want 200", i, code)
		}
	}
}

func TestReadsWaitingForCoreGetServiceUnavailableWhenItStaysDown(t *testing.T) {
	ledger := &testEgressLedger{left: map[string]uint64{"acme": 1 << 40}, reservations: map[string]uint64{}}
	var handled atomic.Int32
	router := egressTestRouter(ledger.exchange, func(c *gin.Context) {
		handled.Add(1)
		c.String(http.StatusOK, "{}")
	})
	stopProbing := make(chan struct{})
	probing := make(chan struct{})
	go func() {
		defer close(probing)
		for {
			select {
			case <-stopProbing:
				return
			case <-time.After(100 * time.Millisecond):
				ctx, cancel := context.WithCancel(context.Background())
				cancel()
				corehold.Hold(ctx, func() bool { return true })
			}
		}
	}()
	t.Cleanup(func() { close(stopProbing); <-probing })
	markCoreDown(t)

	ctx, cancel := context.WithTimeout(t.Context(), 400*time.Millisecond)
	defer cancel()
	req := httptest.NewRequestWithContext(ctx, http.MethodGet, "/api/v1/events/query?entity_id=e-1", http.NoBody)
	req.Header.Set("Test-Tenant", "acme")
	w := httptest.NewRecorder()
	router.ServeHTTP(w, req)

	if w.Code != http.StatusServiceUnavailable || w.Header().Get("Retry-After") == "" {
		t.Fatalf("status %d Retry-After %q, want 503 with Retry-After", w.Code, w.Header().Get("Retry-After"))
	}
	if got := handled.Load(); got != 0 {
		t.Fatalf("handler ran %d times while Core was down", got)
	}
}

func TestLeaseWaiterOutlastsTheHoldersCoreHold(t *testing.T) {
	core := newDownCore(t)
	t.Cleanup(corehold.MarkUp)
	var seen atomic.Int32
	outage := 3500 * time.Millisecond
	core.upAfter(t, outage, http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		seen.Add(1)
		w.Header().Set("Content-Type", "application/json")
		expires := time.Now().Add(2 * time.Minute).Unix()
		_, _ = w.Write([]byte(`{"protocol":"funded-egress-v1","admission":{"snapshot":{"included_available":0,"prepaid_available":0},"reserved_bytes":1073741824,"lease_expires_at":` + strconv.FormatInt(expires, 10) + `}}`)) //nolint:errcheck // test response
	}))
	cp := holdTestCP(t, core.url)
	book := newLeaseBook(cp.exchangeEgress)

	errs := make(chan error, 2)
	for range 2 {
		go func() {
			_, err := book.acquire(t.Context(), "acme")
			errs <- err
		}()
		time.Sleep(100 * time.Millisecond)
	}
	for range 2 {
		if err := <-errs; err != nil {
			t.Fatalf("acquire during a %s outage: %v", outage, err)
		}
	}
}
