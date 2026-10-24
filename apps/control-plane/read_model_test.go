package main

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"

	"github.com/gin-gonic/gin"
)

func TestReadModelProxyMetersSuccessfulReadsAndBindsTenant(t *testing.T) {
	for _, tc := range []struct {
		name, method, model, tenant string
		upstream, want, queries     int
	}{
		{"feed", "GET", "", "reader", 200, 200, 1},
		{"model", "GET", "board", "reader", 200, 200, 1},
		{"commit", "PUT", "board", "reader", 200, 200, 0},
		{"refused", "GET", "board", "reader", 409, 409, 0},
		{"unauthorized", "GET", "board", "", 200, 401, 0},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var queries, reads atomic.Int64
			before := queryByteCount(t)
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if strings.HasSuffix(r.URL.Path, "/usage/increment") {
					queries.Add(1)
					if r.URL.Path != "/api/v1/tenants/reader/usage/increment" {
						t.Errorf("wrong usage tenant: %s", r.URL.Path)
					}
					if _, err := w.Write([]byte(`{"type":"queries","used":1}`)); err != nil {
						t.Errorf("write usage response: %v", err)
					}
					return
				}
				reads.Add(1)
				path := "/api/v1/event-feed"
				if tc.model != "" {
					path = "/api/v1/read-models/" + tc.model
				}
				if r.URL.Path != path || r.Method != tc.method || r.URL.Query().Get("tenant_id") != "" {
					t.Errorf("wrong delegated target: %s %s", r.Method, r.URL)
				}
				if !strings.Contains(r.Header.Get("Authorization"), ":reader:") {
					t.Error("delegated token must bind authenticated tenant")
				}
				w.WriteHeader(tc.upstream)
				if _, err := w.Write([]byte(`{"rows":{},"revision":1}`)); err != nil {
					t.Errorf("write model response: %v", err)
				}
			}))
			defer server.Close()
			cp := newTestCP(t, server.URL, server.URL)
			request := httptest.NewRequestWithContext(t.Context(), tc.method, "/api/v1/read-models/board?tenant_id=spoof&limit=1", strings.NewReader(`{}`))
			response := callHandler(func(c *gin.Context) {
				if tc.model != "" {
					c.Params = gin.Params{{Key: "name", Value: tc.model}}
				}
				cp.ProxyReadModel(c)
			}, request, tc.tenant)
			cp.queryMeter.flush(t.Context())
			if response.Code != tc.want || queries.Load() != int64(tc.queries) {
				t.Fatalf("status=%d queries=%d, want status=%d queries=%d", response.Code, queries.Load(), tc.want, tc.queries)
			}
			if tc.tenant == "" && reads.Load() != 0 {
				t.Fatal("unauthenticated request reached Core")
			}
			if got := queryByteCount(t) - before; got != 0 {
				t.Fatalf("read-model bytes polluted event-history metric: %v", got)
			}
		})
	}
}

func TestReadModelRoutesStopBeforeUpstreamWhenBudgetExhausted(t *testing.T) {
	for _, path := range []string{"/api/v1/event-feed", "/api/v1/read-models/board"} {
		t.Run(path, func(t *testing.T) {
			ledger := &testEgressLedger{left: map[string]uint64{"reader": 3}, reservations: map[string]uint64{}}
			calls := 0
			router := egressTestRouter(ledger.exchange, func(c *gin.Context) {
				calls++
				c.String(http.StatusOK, "abc")
			})
			for _, want := range []int{http.StatusOK, http.StatusPaymentRequired, http.StatusPaymentRequired} {
				request := httptest.NewRequestWithContext(t.Context(), http.MethodGet, path, http.NoBody)
				request.Header.Set("Test-Tenant", "reader")
				response := httptest.NewRecorder()
				router.ServeHTTP(response, request)
				if response.Code != want {
					t.Fatalf("status=%d want=%d", response.Code, want)
				}
			}
			if calls != 1 || ledger.left["reader"] != 0 {
				t.Fatalf("hard stop failed: calls=%d bytes=%d", calls, ledger.left["reader"])
			}
		})
	}
}
