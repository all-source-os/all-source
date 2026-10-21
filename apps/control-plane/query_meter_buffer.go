package main

import (
	"context"
	"fmt"
	"log"
	"sync"
	"time"

	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
)

const queryMeterInterval = 30 * time.Second
const queryMeterThreshold = 100
const queryMeterTenantLimit = 1024
const queryMeterPendingLimit = 1_000_000

var queryMeterPending = promauto.NewGauge(prometheus.GaugeOpts{
	Name: "control_plane_query_meter_pending_queries",
	Help: "Accepted query increments awaiting Core persistence; volatile until flushed.",
})

type queryUsageMeter struct {
	mu      sync.Mutex
	flushMu sync.Mutex
	pending map[string]uint64
	closed  bool
	wake    chan struct{}
	stop    chan struct{}
	done    chan struct{}
	cancel  context.CancelFunc
	persist func(context.Context, string, uint64) error
}

func newQueryUsageMeter(persist func(context.Context, string, uint64) error) *queryUsageMeter {
	ctx, cancel := context.WithCancel(context.Background())
	meter := &queryUsageMeter{pending: make(map[string]uint64), wake: make(chan struct{}, 1),
		stop: make(chan struct{}), done: make(chan struct{}), cancel: cancel, persist: persist}
	go meter.run(ctx)
	return meter
}

func (meter *queryUsageMeter) record(tenant string) error {
	meter.mu.Lock()
	defer meter.mu.Unlock()
	count, exists := meter.pending[tenant]
	if meter.closed || (!exists && len(meter.pending) >= queryMeterTenantLimit) || count >= queryMeterPendingLimit {
		return fmt.Errorf("query meter buffer is unavailable")
	}
	meter.pending[tenant] = count + 1
	queryMeterPending.Inc()
	if count+1 == queryMeterThreshold {
		select {
		case meter.wake <- struct{}{}:
		default:
		}
	}
	return nil
}

func (meter *queryUsageMeter) run(ctx context.Context) {
	defer close(meter.done)
	ticker := time.NewTicker(queryMeterInterval)
	defer ticker.Stop()
	for {
		select {
		case <-ticker.C:
			meter.flush(ctx)
		case <-meter.wake:
			meter.flush(ctx)
		case <-meter.stop:
			meter.flush(ctx)
			return
		}
	}
}

// One snapshot per tenant batch avoids one full metadata event per query.
// Failed batches stay pending. A lost acknowledgement can overcount on retry;
// this follows the existing buffered usage model, not a billing ledger.
func (meter *queryUsageMeter) flush(parent context.Context) {
	meter.flushMu.Lock()
	defer meter.flushMu.Unlock()
	ctx, cancel := context.WithTimeout(parent, 3*time.Second)
	defer cancel()
	meter.mu.Lock()
	batch := make(map[string]uint64, len(meter.pending))
	for tenant, count := range meter.pending {
		batch[tenant] = count
	}
	meter.mu.Unlock()
	for tenant, count := range batch {
		if ctx.Err() != nil {
			return
		}
		if err := meter.persist(ctx, tenant, count); err != nil {
			queryMeterWrites.WithLabelValues("failure").Inc()
			log.Printf("query usage batch persistence failed: %v", err)
			continue
		}
		queryMeterWrites.WithLabelValues("success").Inc()
		meter.mu.Lock()
		meter.pending[tenant] -= count
		if meter.pending[tenant] >= queryMeterThreshold {
			select {
			case meter.wake <- struct{}{}:
			default:
			}
		}
		if meter.pending[tenant] == 0 {
			delete(meter.pending, tenant)
		}
		queryMeterPending.Sub(float64(count))
		meter.mu.Unlock()
	}
}

func (meter *queryUsageMeter) close() error {
	meter.mu.Lock()
	if !meter.closed {
		meter.closed = true
		close(meter.stop)
	}
	meter.mu.Unlock()
	defer meter.cancel()
	timer := time.NewTimer(3 * time.Second)
	defer timer.Stop()
	select {
	case <-meter.done:
	case <-timer.C:
		meter.cancel()
		<-meter.done
	}
	meter.mu.Lock()
	defer meter.mu.Unlock()
	if len(meter.pending) > 0 {
		return fmt.Errorf("query usage remains unflushed at shutdown")
	}
	return nil
}
