package main

import (
	"context"
	"fmt"
	"sync/atomic"
	"testing"
	"time"
)

func TestQueryMeterBatchThresholdAndShutdownFlush(t *testing.T) {
	persisted := make(chan uint64, 2)
	meter := newQueryUsageMeter(func(_ context.Context, _ string, count uint64) error {
		persisted <- count
		return nil
	})
	t.Cleanup(func() {
		if err := meter.close(); err != nil {
			t.Error(err)
		}
	})
	for range queryMeterThreshold {
		if err := meter.record("tenant"); err != nil {
			t.Fatal(err)
		}
	}
	select {
	case count := <-persisted:
		if count != 100 {
			t.Fatalf("batched=%d want=100", count)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("batch threshold did not flush")
	}
	if err := meter.record("tenant"); err != nil {
		t.Fatal(err)
	}
	if err := meter.close(); err != nil {
		t.Fatal(err)
	}
	if err := meter.record("tenant"); err == nil {
		t.Fatal("closed meter accepted a query")
	}
	if count := <-persisted; count != 1 {
		t.Fatalf("shutdown batch=%d want=1", count)
	}
}

func TestQueryMeterRetainsFailedBatch(t *testing.T) {
	var healthy atomic.Bool
	var used atomic.Uint64
	meter := newQueryUsageMeter(func(_ context.Context, _ string, count uint64) error {
		if !healthy.Load() {
			return fmt.Errorf("fixture Core unavailable")
		}
		used.Add(count)
		return nil
	})
	t.Cleanup(func() {
		if err := meter.close(); err != nil {
			t.Error(err)
		}
	})
	if err := meter.record("tenant"); err != nil {
		t.Fatal(err)
	}
	meter.flush(t.Context())
	if err := meter.record("tenant"); err != nil {
		t.Fatal(err)
	}
	healthy.Store(true)
	meter.flush(t.Context())
	if used.Load() != 2 {
		t.Fatalf("recovered usage=%d want=2", used.Load())
	}
}

func TestQueryMeterKeepsQueriesArrivingDuringPersistence(t *testing.T) {
	started := make(chan struct{}, 1)
	release := make(chan struct{})
	var used atomic.Uint64
	meter := newQueryUsageMeter(func(ctx context.Context, _ string, count uint64) error {
		started <- struct{}{}
		select {
		case <-release:
			used.Add(count)
			return nil
		case <-ctx.Done():
			return ctx.Err()
		}
	})
	t.Cleanup(func() {
		if err := meter.close(); err != nil {
			t.Error(err)
		}
	})
	if err := meter.record("tenant"); err != nil {
		t.Fatal(err)
	}
	done := make(chan struct{})
	go func() { meter.flush(t.Context()); close(done) }()
	select {
	case <-started:
	case <-time.After(5 * time.Second):
		t.Fatal("persistence did not start")
	}
	if err := meter.record("tenant"); err != nil {
		t.Fatal(err)
	}
	close(release)
	select {
	case <-done:
	case <-time.After(5 * time.Second):
		t.Fatal("flush did not finish")
	}
	if err := meter.close(); err != nil {
		t.Fatal(err)
	}
	if used.Load() != 2 {
		t.Fatalf("usage=%d want=2", used.Load())
	}
}

func TestQueryMeterBufferIsBoundedByTenantCount(t *testing.T) {
	meter := newQueryUsageMeter(func(context.Context, string, uint64) error { return nil })
	t.Cleanup(func() {
		if err := meter.close(); err != nil {
			t.Error(err)
		}
	})
	for index := range queryMeterTenantLimit {
		if err := meter.record(fmt.Sprintf("tenant-%d", index)); err != nil {
			t.Fatal(err)
		}
	}
	if err := meter.record("overflow"); err == nil {
		t.Fatal("unbounded tenant growth was accepted")
	}
}

func TestQueryMeterFailedThresholdDoesNotRetryOnEveryRead(t *testing.T) {
	meter := &queryUsageMeter{pending: make(map[string]uint64), wake: make(chan struct{}, 1)}
	for range queryMeterThreshold {
		if err := meter.record("tenant"); err != nil {
			t.Fatal(err)
		}
	}
	<-meter.wake
	for range 1_000 {
		if err := meter.record("tenant"); err != nil {
			t.Fatal(err)
		}
	}
	if len(meter.wake) != 0 {
		t.Fatal("failed batch created per-read flush retries")
	}
	meter.pending["tenant"] = queryMeterPendingLimit
	if err := meter.record("tenant"); err == nil {
		t.Fatal("per-tenant pending limit was exceeded")
	}
}
