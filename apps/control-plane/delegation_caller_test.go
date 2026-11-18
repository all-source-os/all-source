package main

import (
	"bytes"
	"context"
	"fmt"
	"log"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/prometheus/client_golang/prometheus"
	dto "github.com/prometheus/client_model/go"
)

func metricCount(t *testing.T, m prometheus.Metric) float64 {
	t.Helper()
	var out dto.Metric
	if err := m.Write(&out); err != nil {
		t.Fatal(err)
	}
	return out.GetCounter().GetValue()
}

const callerTestBody = `{"events":[]}`

func eventsQueryWithHeaders(cp *ControlPlane, tenant string, headers map[string]string) (int, *http.Request) {
	req := httptest.NewRequestWithContext(context.Background(), http.MethodGet, "/api/v1/events/query", http.NoBody)
	for k, v := range headers {
		req.Header.Set(k, v)
	}
	w := callHandler(cp.ProxyEventsQuery, req, tenant)
	return w.Code, req
}

func TestEventsQueryForwardsValidReadAttributionHeaders(t *testing.T) {
	core, srv := newFakeBackend(okJSON(callerTestBody))
	defer srv.Close()
	cp := newTestCP(t, srv.URL, srv.URL)

	code, _ := eventsQueryWithHeaders(cp, "tenant-attr-forward", map[string]string{
		callerHeader: "lh.skill_catalog",
		readIDHeader: "0123456789abcdef",
	})
	if code != http.StatusOK {
		t.Fatalf("status: got %d, want 200", code)
	}
	if got := core.header.Get(callerHeader); got != "lh.skill_catalog" {
		t.Errorf("upstream %s: got %q, want lh.skill_catalog", callerHeader, got)
	}
	if got := core.header.Get(readIDHeader); got != "0123456789abcdef" {
		t.Errorf("upstream %s: got %q, want the read id as sent", readIDHeader, got)
	}
}

func TestInvalidReadAttributionHeadersAreReplacedAndCounted(t *testing.T) {
	core, srv := newFakeBackend(okJSON(callerTestBody))
	defer srv.Close()
	cp := newTestCP(t, srv.URL, srv.URL)

	beforeCaller := metricCount(t, readHeaderInvalid.WithLabelValues(callerHeader))
	beforeReadID := metricCount(t, readHeaderInvalid.WithLabelValues(readIDHeader))

	code, _ := eventsQueryWithHeaders(cp, "tenant-attr-invalid", map[string]string{
		callerHeader: "../../etc",
		readIDHeader: "XYZ",
	})
	if code != http.StatusOK {
		t.Fatalf("status: got %d, want 200", code)
	}
	if got := core.header.Get(callerHeader); got != "invalid" {
		t.Errorf("upstream %s: got %q, want invalid", callerHeader, got)
	}
	if got := core.header.Get(readIDHeader); got != "invalid" {
		t.Errorf("upstream %s: got %q, want invalid", readIDHeader, got)
	}
	if d := metricCount(t, readHeaderInvalid.WithLabelValues(callerHeader)) - beforeCaller; d != 1 {
		t.Errorf("caller invalid counter delta: got %v, want 1", d)
	}
	if d := metricCount(t, readHeaderInvalid.WithLabelValues(readIDHeader)) - beforeReadID; d != 1 {
		t.Errorf("read id invalid counter delta: got %v, want 1", d)
	}
}

func TestEventsQueryMetersWireBytesPerCaller(t *testing.T) {
	_, srv := newFakeBackend(okJSON(callerTestBody))
	defer srv.Close()
	cp := newTestCP(t, srv.URL, srv.URL)

	const tenant = "tenant-attr-caller"
	bytesSeries := queryCallerBytes.WithLabelValues(tenantLabel(tenant), "lh.skill_catalog")
	queriesSeries := queryCallerQueries.WithLabelValues(tenantLabel(tenant), "lh.skill_catalog")
	beforeBytes, beforeQueries := metricCount(t, bytesSeries), metricCount(t, queriesSeries)

	if code, _ := eventsQueryWithHeaders(cp, tenant, map[string]string{callerHeader: "lh.skill_catalog"}); code != http.StatusOK {
		t.Fatalf("status: got %d, want 200", code)
	}
	if d := metricCount(t, bytesSeries) - beforeBytes; d != float64(len(callerTestBody)) {
		t.Errorf("caller bytes delta: got %v, want %d", d, len(callerTestBody))
	}
	if d := metricCount(t, queriesSeries) - beforeQueries; d != 1 {
		t.Errorf("caller queries delta: got %v, want 1", d)
	}
}

func TestEventsQueryWithoutCallerHeaderIsUnattributed(t *testing.T) {
	_, srv := newFakeBackend(okJSON(callerTestBody))
	defer srv.Close()
	cp := newTestCP(t, srv.URL, srv.URL)

	const tenant = "tenant-attr-missing"
	series := queryCallerQueries.WithLabelValues(tenantLabel(tenant), "unattributed")
	before := metricCount(t, series)
	if code, _ := eventsQueryWithHeaders(cp, tenant, nil); code != http.StatusOK {
		t.Fatalf("status: got %d, want 200", code)
	}
	if d := metricCount(t, series) - before; d != 1 {
		t.Errorf("unattributed queries delta: got %v, want 1", d)
	}
}

func TestCallerLabelsAdmitSixtyFourCallersPerTenant(t *testing.T) {
	labels := newCallerLabels()
	for i := range maxCallerLabels {
		name := fmt.Sprintf("caller_%02d", i)
		if got := labels.label("tenant-a", name); got != name {
			t.Fatalf("caller %d: got %q, want %q", i, got, name)
		}
	}
	if got := labels.label("tenant-a", "caller_late"); got != overflowLabel {
		t.Errorf("65th caller: got %q, want %q", got, overflowLabel)
	}
	if got := labels.label("tenant-a", "caller_00"); got != "caller_00" {
		t.Errorf("admitted caller after overflow: got %q, want caller_00", got)
	}
	if got := labels.label("tenant-b", "caller_late"); got != "caller_late" {
		t.Errorf("65th caller of another tenant: got %q, want caller_late", got)
	}
	if got := labels.label("tenant-a", ""); got != unattributedCaller {
		t.Errorf("empty caller: got %q, want %q", got, unattributedCaller)
	}
	if got := labels.label("tenant-a", invalidValue); got != invalidValue {
		t.Errorf("invalid caller: got %q, want %q", got, invalidValue)
	}
}

func TestCallerPatternAcceptsOnlyBoundedNames(t *testing.T) {
	for _, ok := range []string{"lh.skill_catalog", "a", "svc_1.x", strings.Repeat("a", 64)} {
		if !callerPattern.MatchString(ok) {
			t.Errorf("rejected %q", ok)
		}
	}
	for _, bad := range []string{"", "LH.x", "1abc", "../../etc", "has space", strings.Repeat("a", 65)} {
		if callerPattern.MatchString(bad) {
			t.Errorf("accepted %q", bad)
		}
	}
	if !readIDPattern.MatchString("0123456789abcdef") || readIDPattern.MatchString("0123456789ABCDEF") || readIDPattern.MatchString("0123") {
		t.Error("read id pattern must be exactly 16 lowercase hex digits")
	}
}

func TestReadHeaderInvalidLogsOncePerMinute(t *testing.T) {
	var gate rateGate
	now := time.Unix(100, 0)
	if !gate.allow(now, time.Minute) {
		t.Fatal("first call denied")
	}
	if gate.allow(now.Add(59*time.Second), time.Minute) {
		t.Fatal("call inside the minute allowed")
	}
	if !gate.allow(now.Add(time.Minute), time.Minute) {
		t.Fatal("call at one minute denied")
	}
}

func TestLargeQuerySampleCarriesValidatedAttribution(t *testing.T) {
	var sampler queryEgressSampler
	got := sampler.sample(nil, 300000, time.Unix(100, 0), "lh.skill_catalog", "0123456789abcdef")
	if !strings.Contains(got, `caller="lh.skill_catalog"`) || !strings.Contains(got, `read_id="0123456789abcdef"`) {
		t.Fatalf("missing caller or read_id: %s", got)
	}
}

func TestReadCostsAndStreamsForceAuthTenant(t *testing.T) {
	for _, tc := range []struct {
		name     string
		path     string
		upstream string
		handler  func(*ControlPlane) gin.HandlerFunc
	}{
		{"read-costs", "/api/v1/read-costs?limit=10&order=bytes&tenant_id=victim-tenant", "/api/v1/read-costs", func(cp *ControlPlane) gin.HandlerFunc { return cp.ProxyReadCosts }},
		{"streams", "/api/v1/streams?with_bytes=true&tenant_id=victim-tenant", "/api/v1/streams", func(cp *ControlPlane) gin.HandlerFunc { return cp.ProxyStreams }},
	} {
		t.Run(tc.name, func(t *testing.T) {
			core, srv := newFakeBackend(okJSON(`{}`))
			defer srv.Close()
			cp := newTestCP(t, srv.URL, srv.URL)

			req := httptest.NewRequestWithContext(context.Background(), http.MethodGet, tc.path, http.NoBody)
			w := callHandler(tc.handler(cp), req, "tenant-real")
			if w.Code != http.StatusOK {
				t.Fatalf("status: got %d, want 200 (body=%s)", w.Code, w.Body.String())
			}
			if core.path != tc.upstream {
				t.Errorf("upstream path: got %q, want %q", core.path, tc.upstream)
			}
			if vals := core.query["tenant_id"]; len(vals) != 1 || vals[0] != "tenant-real" {
				t.Errorf("upstream tenant_id: got %v, want exactly [tenant-real]", vals)
			}
		})
	}
}

func TestReadCostsAndStreamsAreManagedReadScope(t *testing.T) {
	for _, path := range []string{"/api/v1/read-costs", "/api/v1/streams"} {
		c, _ := gin.CreateTestContext(httptest.NewRecorder())
		c.Request = httptest.NewRequestWithContext(context.Background(), http.MethodGet, path, http.NoBody)
		if got := managedScope(c); got != managedScopeRead {
			t.Errorf("%s scope: got %q, want %q", path, got, managedScopeRead)
		}
	}
}

func TestEventsQueryStripsHeadersOutsideTheForwardList(t *testing.T) {
	core, srv := newFakeBackend(okJSON(callerTestBody))
	defer srv.Close()
	cp := newTestCP(t, srv.URL, srv.URL)

	code, _ := eventsQueryWithHeaders(cp, "tenant-attr-strip", map[string]string{
		"Authorization":   "Bearer ask_inbound_secret",
		"Cookie":          "session=inbound-cookie",
		"X-Forwarded-For": "203.0.113.9",
		"X-Custom-Secret": "inbound-custom",
		"If-None-Match":   `"etag-1"`,
		callerHeader:      "lh.skill_catalog",
	})
	if code != http.StatusOK {
		t.Fatalf("status: got %d, want 200", code)
	}
	for _, name := range []string{"Cookie", "X-Forwarded-For", "X-Custom-Secret"} {
		if got := core.header.Get(name); got != "" {
			t.Errorf("upstream %s: got %q, want it stripped", name, got)
		}
	}
	if strings.Contains(core.auth, "ask_inbound_secret") {
		t.Errorf("upstream Authorization carries the inbound key: %q", core.auth)
	}
	if got := core.header.Get("If-None-Match"); got != `"etag-1"` {
		t.Errorf("upstream If-None-Match: got %q, want it forwarded", got)
	}
	if got := core.header.Get(callerHeader); got != "lh.skill_catalog" {
		t.Errorf("upstream %s: got %q, want lh.skill_catalog", callerHeader, got)
	}
}

func TestReadHeaderInvalidLogCarriesLengthOnly(t *testing.T) {
	_, srv := newFakeBackend(okJSON(callerTestBody))
	defer srv.Close()
	cp := newTestCP(t, srv.URL, srv.URL)
	readHeaderLogGates[callerHeader].next.Store(0)
	readHeaderLogGates[readIDHeader].next.Store(0)

	var logged bytes.Buffer
	log.SetOutput(&logged)
	defer log.SetOutput(os.Stderr)

	const badCaller = "SECRET-CALLER-VALUE"
	const badReadID = "SECRETREADIDVALUE"
	if code, _ := eventsQueryWithHeaders(cp, "tenant-attr-log", map[string]string{
		callerHeader: badCaller,
		readIDHeader: badReadID,
	}); code != http.StatusOK {
		t.Fatalf("status: got %d, want 200", code)
	}

	out := logged.String()
	for _, want := range []string{
		fmt.Sprintf("read_header_invalid header=%s length=%d", callerHeader, len(badCaller)),
		fmt.Sprintf("read_header_invalid header=%s length=%d", readIDHeader, len(badReadID)),
	} {
		if !strings.Contains(out, want) {
			t.Errorf("log missing %q: %s", want, out)
		}
	}
	for _, secret := range []string{badCaller, badReadID, "tenant-attr-log"} {
		if strings.Contains(out, secret) {
			t.Errorf("log carries %q: %s", secret, out)
		}
	}
}
