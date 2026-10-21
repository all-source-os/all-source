package main

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"sync/atomic"
	"testing"

	"github.com/prometheus/client_golang/prometheus"
)

func TestQueryMeterPersistsOnlySuccessfulAuthenticatedQueries(t *testing.T) {
	for _, tc := range []struct {
		name       string
		tenant     string
		upstream   int
		meter      int
		want       int
		increments int64
	}{
		{"success", "tenant-real", 200, 200, 200, 1},
		{"empty-result", "tenant-real", 200, 200, 200, 1},
		{"bad-query", "tenant-real", 400, 200, 400, 0},
		{"upstream-failure", "tenant-real", 503, 200, 503, 0},
		{"meter-failure", "tenant-real", 200, 503, 200, 1},
		{"unauthorized", "", 200, 200, 401, 0},
	} {
		t.Run(tc.name, func(t *testing.T) {
			before := queryByteCount(t)
			var increments atomic.Int64
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if strings.HasSuffix(r.URL.Path, "/usage/increment") {
					increments.Add(1)
					if r.URL.Path != "/api/v1/tenants/tenant-real/usage/increment" || r.Method != http.MethodPost {
						t.Errorf("wrong meter target: %s %s", r.Method, r.URL.Path)
					}
					if r.Header.Get("Authorization") != "Bearer test-admin-service-token" {
						t.Error("meter must use existing service credential")
					}
					var body map[string]any
					if err := json.NewDecoder(r.Body).Decode(&body); err != nil || body["type"] != "queries" || body["count"] != float64(1) {
						t.Errorf("wrong meter payload: %#v, %v", body, err)
					}
					w.WriteHeader(tc.meter)
					if _, err := w.Write([]byte(`{"type":"queries","used":1}`)); err != nil {
						t.Error(err)
					}
					return
				}
				if r.URL.Query().Get("tenant_id") != "tenant-real" {
					t.Error("query tenant must come from auth")
				}
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(tc.upstream)
				if _, err := w.Write([]byte(`{"events":[],"count":0}`)); err != nil {
					t.Error(err)
				}
			}))
			defer server.Close()
			cp := newTestCP(t, server.URL, server.URL)
			request := httptest.NewRequestWithContext(context.Background(), http.MethodGet, "/api/v1/events/query?tenant_id=spoof", http.NoBody)
			response := callHandler(cp.ProxyEventsQuery, request, tc.tenant)
			cp.queryMeter.flush(t.Context())
			if response.Code != tc.want || increments.Load() != tc.increments {
				t.Fatalf("status=%d increments=%d body=%s", response.Code, increments.Load(), response.Body.String())
			}
			if tc.meter >= 400 && !strings.Contains(response.Body.String(), `"events"`) {
				t.Error("meter outage must not break a successful query")
			}
			wantBytes := float64(0)
			if tc.tenant != "" {
				wantBytes = float64(len(`{"events":[],"count":0}`))
			}
			if got := queryByteCount(t) - before; got != wantBytes {
				t.Errorf("response byte meter=%v want=%v", got, wantBytes)
			}
		})
	}
}

func queryByteCount(t *testing.T) float64 {
	t.Helper()
	registry := prometheus.NewRegistry()
	registry.MustRegister(queryResponseBytes)
	families, err := registry.Gather()
	if err != nil || len(families) != 1 {
		t.Fatalf("gather response byte counter: %v", err)
	}
	return families[0].Metric[0].Counter.GetValue()
}

func TestQueryMeterCountsConcurrentRequestsWithoutQueryServiceDoubleCounting(t *testing.T) {
	var used atomic.Int64
	var writes atomic.Int64
	core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if strings.HasSuffix(r.URL.Path, "/usage/increment") {
			var body struct {
				Count int64 `json:"count"`
			}
			if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
				t.Error(err)
			}
			used.Add(body.Count)
			writes.Add(1)
		} else if r.URL.Path != "/api/v1/events/query" {
			t.Errorf("unexpected Core path: %s", r.URL.Path)
		}
		w.Header().Set("Content-Type", "application/json")
		if _, err := w.Write([]byte(`{"events":[],"count":0}`)); err != nil {
			t.Error(err)
		}
	}))
	defer core.Close()
	queryService := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		t.Error("Core-direct query must not also invoke Query Service metering")
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer queryService.Close()
	cp := newTestCP(t, core.URL, queryService.URL)
	var workers sync.WaitGroup
	for range 24 {
		workers.Go(func() {
			request := httptest.NewRequestWithContext(context.Background(), http.MethodGet, "/api/v1/events/query", http.NoBody)
			if response := callHandler(cp.ProxyEventsQuery, request, "tenant-real"); response.Code != http.StatusOK {
				t.Errorf("query failed: %d %s", response.Code, response.Body.String())
			}
		})
	}
	workers.Wait()
	cp.queryMeter.flush(t.Context())
	if used.Load() != 24 {
		t.Fatalf("durable increments=%d want=24", used.Load())
	}
	if writes.Load() != 1 {
		t.Fatalf("batch writes=%d want=1", writes.Load())
	}
}
