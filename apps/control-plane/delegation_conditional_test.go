package main

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"

	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/domain/entities"
)

type conditionalCore struct {
	increments  atomic.Int64
	ifNoneMatch atomic.Value
	encoding    atomic.Value
}

func newConditionalCore(t *testing.T) (*conditionalCore, *httptest.Server) {
	t.Helper()
	core := &conditionalCore{}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if strings.HasSuffix(r.URL.Path, "/usage/increment") {
			core.increments.Add(1)
			_, _ = w.Write([]byte(`{"type":"queries","used":1}`)) //nolint:errcheck // test response
			return
		}
		core.ifNoneMatch.Store(r.Header.Get("If-None-Match"))
		core.encoding.Store(r.Header.Get("Accept-Encoding"))
		w.Header().Set("ETag", `"rev-7"`)
		w.Header().Set("Cache-Control", "private, max-age=0")
		w.Header().Set("RateLimit-Policy", `"core";q=40;w=1`)
		w.Header().Set("RateLimit", `"core";r=39;t=1`)
		w.Header().Set("Retry-After", "2")
		w.Header().Set("X-Core-Internal", "secret")
		if r.Header.Get("If-None-Match") == `"rev-7"` {
			w.WriteHeader(http.StatusNotModified)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"events":[],"count":0}`)) //nolint:errcheck // test response
	}))
	t.Cleanup(server.Close)
	return core, server
}

func conditionalRouter(t *testing.T, coreURL string, ledger *countingLedger, mode admissionMode) *gin.Engine {
	t.Helper()
	cp := newTestCP(t, coreURL, coreURL)
	cp.egressExchange = ledger.exchange
	cp.admission = newTenantAdmission(mode, defaultAdmissionLimits(), nil)
	router := gin.New()
	router.Use(func(c *gin.Context) {
		c.Set("auth_tenant_id", "conditional-tenant")
		c.Set("auth_user_id", "user-conditional")
		c.Set("auth_role", entities.RoleDeveloper)
		c.Next()
	})
	router.Use(cp.enforceEgress)
	router.GET(queryEventsPath, cp.ProxyEventsQuery)
	t.Cleanup(func() { cp.queryMeter.flush(t.Context()) })
	return router
}

func TestCore304ReachesClientWithETagAndMetersZeroBytes(t *testing.T) {
	core, server := newConditionalCore(t)
	ledger := &countingLedger{}
	router := conditionalRouter(t, server.URL, ledger, admissionShadow)
	labels := map[string]string{"tenant": "conditional-tenant", "namespace": "skill"}
	conditionalBefore := counterValue(t, queryConditional, labels)
	bytesBefore := queryByteCount(t)
	shapeBefore := counterValue(t, tenantQueries, labels)

	request := httptest.NewRequestWithContext(t.Context(), http.MethodGet, queryEventsPath+"?event_type_prefix=skill.", http.NoBody)
	request.Header.Set("If-None-Match", `"rev-7"`)
	request.Header.Set("Accept-Encoding", "gzip")
	response := httptest.NewRecorder()
	router.ServeHTTP(response, request)

	if response.Code != http.StatusNotModified || response.Header().Get("ETag") != `"rev-7"` || response.Body.Len() != 0 {
		t.Fatalf("304 passthrough: %d etag=%q body=%q", response.Code, response.Header().Get("ETag"), response.Body.String())
	}
	if response.Header().Get("Content-Length") != "" {
		t.Fatalf("304 carries Content-Length %q", response.Header().Get("Content-Length"))
	}
	if core.ifNoneMatch.Load() != `"rev-7"` || core.encoding.Load() != "gzip" {
		t.Fatalf("conditional headers not forwarded: inm=%v ae=%v", core.ifNoneMatch.Load(), core.encoding.Load())
	}
	if got := counterValue(t, queryConditional, labels) - conditionalBefore; got != 1 {
		t.Fatalf("conditional counter delta=%v", got)
	}
	if got := queryByteCount(t) - bytesBefore; got != 0 {
		t.Fatalf("304 metered %v body bytes", got)
	}
	if got := counterValue(t, tenantQueries, labels) - shapeBefore; got != 0 {
		t.Fatalf("304 counted as a data response: %v", got)
	}
	if len(ledger.bytes) != 1 || ledger.bytes[0] != 0 {
		t.Fatalf("304 settled with bytes %v", ledger.bytes)
	}
	if core.increments.Load() != 0 {
		t.Fatal("304 recorded as a billable query")
	}
}

func TestCoreResponseHeadersPassThroughAllowlist(t *testing.T) {
	_, server := newConditionalCore(t)
	for _, mode := range []admissionMode{admissionShadow, admissionEnforce} {
		t.Run(string(mode), func(t *testing.T) {
			router := conditionalRouter(t, server.URL, &countingLedger{}, mode)
			labels := map[string]string{"tenant": "conditional-tenant", "namespace": "skill", "selector": "event_type_prefix"}
			bytesBefore := counterValue(t, tenantQueryBytes, labels)

			request := httptest.NewRequestWithContext(t.Context(), http.MethodGet, queryEventsPath+"?event_type_prefix=skill.", http.NoBody)
			response := httptest.NewRecorder()
			router.ServeHTTP(response, request)

			if response.Code != http.StatusOK {
				t.Fatalf("status %d: %s", response.Code, response.Body.String())
			}
			for name, want := range map[string]string{"ETag": `"rev-7"`, "Cache-Control": "private, max-age=0", "Retry-After": "2", "Content-Type": "application/json"} {
				if got := response.Header().Get(name); got != want {
					t.Errorf("%s=%q want %q", name, got, want)
				}
			}
			if response.Header().Get("X-Core-Internal") != "" {
				t.Error("non-allowlisted Core header leaked")
			}
			for _, name := range []string{"RateLimit", "RateLimit-Policy"} {
				values := strings.Join(response.Header().Values(name), ", ")
				if !strings.Contains(values, `"core"`) || strings.Contains(values, `"data"`) != (mode == admissionEnforce) {
					t.Errorf("%s in %s: %q", name, mode, values)
				}
			}
			if got := counterValue(t, tenantQueryBytes, labels) - bytesBefore; got != float64(len(`{"events":[],"count":0}`)) {
				t.Fatalf("tenant/namespace byte counter delta=%v", got)
			}
		})
	}
}

func TestCore2xxStillRunsMeterAndCompressedBodyKeepsEncoding(t *testing.T) {
	var increments atomic.Int64
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if strings.HasSuffix(r.URL.Path, "/usage/increment") {
			increments.Add(1)
			_, _ = w.Write([]byte(`{}`)) //nolint:errcheck // test response
			return
		}
		w.Header().Set("Content-Encoding", "gzip")
		w.Header().Set("Vary", "Accept-Encoding")
		_, _ = w.Write([]byte("\x1f\x8bcompressed")) //nolint:errcheck // test response
	}))
	defer server.Close()
	cp := newTestCP(t, server.URL, server.URL)
	request := httptest.NewRequestWithContext(t.Context(), http.MethodGet, queryEventsPath, http.NoBody)
	request.Header.Set("Accept-Encoding", "gzip")
	response := callHandler(cp.ProxyEventsQuery, request, "gzip-tenant")
	cp.queryMeter.flush(t.Context())
	if response.Code != 200 || response.Header().Get("Content-Encoding") != "gzip" || response.Header().Get("Vary") != "Accept-Encoding" || response.Body.String() != "\x1f\x8bcompressed" {
		t.Fatalf("compressed passthrough: %d %v %q", response.Code, response.Header(), response.Body.String())
	}
	if increments.Load() != 1 {
		t.Fatalf("2xx meter increments=%d", increments.Load())
	}
}
