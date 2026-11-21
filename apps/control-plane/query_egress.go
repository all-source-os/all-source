package main

import (
	"crypto/sha256"
	"fmt"
	"net/url"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
)

var queryShapeBytes = promauto.NewCounterVec(prometheus.CounterOpts{
	Name: "control_plane_event_query_body_bytes_by_shape_total",
	Help: "Event-query response body bytes by bounded selector shape; excludes query values.",
}, []string{"selector", "time_bounded", "payload_filtered"})

var queryShapeRequests = promauto.NewCounterVec(prometheus.CounterOpts{
	Name: "control_plane_event_queries_by_shape_total",
	Help: "Forwarded event-query responses by bounded selector shape.",
}, []string{"selector", "time_bounded", "payload_filtered"})

var tenantQueryBytes = tenantMetrics.NewCounterVec(prometheus.CounterOpts{
	Name: "control_plane_tenant_event_query_body_bytes_total",
	Help: "Event-query response body bytes by tenant, namespace and selector shape.",
}, []string{"tenant", "namespace", "selector", "time_bounded", "payload_filtered"})

var tenantQueries = tenantMetrics.NewCounterVec(prometheus.CounterOpts{
	Name: "control_plane_tenant_event_queries_total",
	Help: "Forwarded event-query responses by tenant, namespace and selector shape.",
}, []string{"tenant", "namespace", "selector", "time_bounded", "payload_filtered"})

var queryConditional = tenantMetrics.NewCounterVec(prometheus.CounterOpts{
	Name: "control_plane_tenant_event_query_conditional_total",
	Help: "Event queries answered 304 Not Modified; metered as zero body bytes.",
}, []string{"tenant", "namespace"})

var queryCallerBytes = tenantMetrics.NewCounterVec(prometheus.CounterOpts{
	Name: "control_plane_tenant_event_query_caller_bytes_total",
	Help: "Event-query response body bytes by tenant and X-Allsource-Caller; callers past 64 per tenant share other.",
}, []string{"tenant", "caller"})

var queryCallerQueries = tenantMetrics.NewCounterVec(prometheus.CounterOpts{
	Name: "control_plane_tenant_event_query_caller_queries_total",
	Help: "Forwarded event-query responses by tenant and X-Allsource-Caller.",
}, []string{"tenant", "caller"})

const (
	maxCallerLabels    = 64
	maxCallerTenants   = 2 * maxTenantLabels
	unattributedCaller = "unattributed"
)

var queryCallers = newCallerLabels()

// callerLabels admits up to maxCallerLabels callers per tenant. Slots are
// keyed by tenant id, so tenants whose series label is other do not share 64
// slots; past maxCallerTenants ids, later tenants share their label's slots.
type callerLabels struct {
	mu     sync.Mutex
	seen   map[string]map[string]struct{}
	shared map[string]map[string]struct{}
}

func newCallerLabels() *callerLabels {
	return &callerLabels{seen: map[string]map[string]struct{}{}, shared: map[string]map[string]struct{}{}}
}

// label returns the caller label for tenantID, whose series label is tenant.
func (c *callerLabels) label(tenantID, tenant, caller string) string {
	switch caller {
	case "":
		return unattributedCaller
	case invalidValue:
		return invalidValue
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	names := c.seen[tenantID]
	if names == nil {
		if len(c.seen) < maxCallerTenants {
			names = map[string]struct{}{}
			c.seen[tenantID] = names
		} else {
			names = c.shared[tenant]
			if names == nil {
				names = map[string]struct{}{}
				c.shared[tenant] = names
			}
		}
	}
	if _, ok := names[caller]; ok {
		return caller
	}
	if len(names) >= maxCallerLabels {
		return overflowLabel
	}
	names[caller] = struct{}{}
	return caller
}

type rateGate struct{ next atomic.Int64 }

func (g *rateGate) allow(now time.Time, every time.Duration) bool {
	next := g.next.Load()
	return now.UnixNano() >= next && g.next.CompareAndSwap(next, now.Add(every).UnixNano())
}

var queryEgressSamples queryEgressSampler

const queryOpaqueNamespace = "opaque"

type queryEgressSampler struct{ next atomic.Int64 }

func queryShape(q url.Values) (selector, bounded, filtered string) {
	selector = "unfiltered"
	for _, key := range []string{"entity_id", "event_type", "event_type_prefix"} {
		if q.Get(key) != "" {
			selector = key
			break
		}
	}
	bounded = strconv.FormatBool(q.Get("since") != "" || q.Get("until") != "")
	filtered = strconv.FormatBool(q.Get("payload_filter") != "")
	return
}

func queryNamespace(value, separator string) string {
	if value == "" {
		return "none"
	}
	prefix, _, found := strings.Cut(value, separator)
	if !found || prefix == "" || len(prefix) > 32 {
		return queryOpaqueNamespace
	}
	for _, c := range prefix {
		valid := c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '_' || c == '-'
		if !valid {
			return queryOpaqueNamespace
		}
	}
	return prefix
}

func (s *queryEgressSampler) sample(q url.Values, bytes int64, now time.Time, caller, readID string) string {
	if bytes < 16*1024 {
		return ""
	}
	next := s.next.Load()
	if now.UnixNano() < next || !s.next.CompareAndSwap(next, now.Add(10*time.Second).UnixNano()) {
		return ""
	}
	selector, bounded, filtered := queryShape(q)
	eventType := q.Get("event_type")
	if eventType == "" {
		eventType = q.Get("event_type_prefix")
	}
	tenantHash := sha256.Sum256([]byte(q.Get("tenant_id")))
	entityHash := sha256.Sum256([]byte(q.Get("entity_id")))
	eventHash := sha256.Sum256([]byte(eventType))
	return fmt.Sprintf("event_query_large_response bytes=%d selector=%s entity_namespace=%q event_namespace=%q tenant_fingerprint=%x entity_fingerprint=%x event_fingerprint=%x time_bounded=%s payload_filtered=%s caller=%q read_id=%q",
		bytes, selector, queryNamespace(q.Get("entity_id"), ":"), queryNamespace(eventType, "."), tenantHash[:6], entityHash[:6], eventHash[:6], bounded, filtered, caller, readID)
}
