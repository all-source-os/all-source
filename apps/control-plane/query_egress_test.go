package main

import (
	"net/url"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func TestQueryShapeKeepsMetricCardinalityBounded(t *testing.T) {
	for _, tc := range []struct {
		query, selector, bounded, filtered string
	}{
		{"", "unfiltered", "false", "false"},
		{"entity_id=user%3Aprivate&since=now&payload_filter=secret", "entity_id", "true", "true"},
		{"event_type=custom.secret&until=then", "event_type", "true", "false"},
		{"event_type_prefix=custom.secret", "event_type_prefix", "false", "false"},
	} {
		q, err := url.ParseQuery(tc.query)
		if err != nil {
			t.Fatal(err)
		}
		a, b, c := queryShape(q)
		if a != tc.selector || b != tc.bounded || c != tc.filtered {
			t.Fatalf("shape=%q,%q,%q", a, b, c)
		}
	}
}

func TestLargeQuerySampleRedactsValuesAndLimitsFrequency(t *testing.T) {
	var sampler queryEgressSampler
	q := url.Values{"entity_id": {"team-queue:private-customer"}, "event_type_prefix": {"team_queue.work_item.private"}, "payload_filter": {"sensitive-payload"}}
	now := time.Unix(100, 0)
	if got := sampler.sample(q, 1024, now); got != "" {
		t.Fatalf("small response sampled: %s", got)
	}
	got := sampler.sample(q, 300000, now)
	if !strings.Contains(got, "bytes=300000") || !strings.Contains(got, `entity_namespace="team-queue"`) {
		t.Fatalf("missing attribution: %s", got)
	}
	for _, secret := range []string{"private-customer", "sensitive-payload", "work_item.private"} {
		if strings.Contains(got, secret) {
			t.Fatalf("query value leaked: %s", got)
		}
	}
	if sampler.sample(q, 300000, now.Add(9*time.Second)) != "" || sampler.sample(q, 300000, now.Add(10*time.Second)) == "" {
		t.Fatal("sample deadline not enforced")
	}
	for _, value := range []string{"private@example.com", "bad\nnamespace:secret", strings.Repeat("x", 33) + ":secret"} {
		if queryNamespace(value, ":") != "opaque" {
			t.Fatal("unsafe namespace retained")
		}
	}
}

func TestLargeQuerySampleConcurrentBudget(t *testing.T) {
	var sampler queryEgressSampler
	var count atomic.Int64
	var workers sync.WaitGroup
	for range 64 {
		workers.Go(func() {
			if sampler.sample(nil, 300000, time.Unix(100, 0)) != "" {
				count.Add(1)
			}
		})
	}
	workers.Wait()
	if count.Load() != 1 {
		t.Fatalf("concurrent samples=%d", count.Load())
	}
}
