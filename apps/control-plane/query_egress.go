package main

import (
	"crypto/sha256"
	"fmt"
	"net/url"
	"strconv"
	"strings"
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

func (s *queryEgressSampler) sample(q url.Values, bytes int64, now time.Time) string {
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
	return fmt.Sprintf("event_query_large_response bytes=%d selector=%s entity_namespace=%q event_namespace=%q tenant_fingerprint=%x entity_fingerprint=%x event_fingerprint=%x time_bounded=%s payload_filtered=%s",
		bytes, selector, queryNamespace(q.Get("entity_id"), ":"), queryNamespace(eventType, "."), tenantHash[:6], entityHash[:6], eventHash[:6], bounded, filtered)
}
