package main

import (
	"context"
	"errors"
	"log"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/google/uuid"
	"github.com/prometheus/client_golang/prometheus"
)

const (
	// Core clamps a grant to its own MAX_LEASE_BYTES (1 GiB) and the balance.
	maxEgressLeaseBytes   = 1024 * 1024 * 1024
	egressReasonExhausted = "exhausted"
	egressReasonBusy      = "busy"

	// Core expires a lease at its own clock; the margin covers clock skew.
	leaseSafetyMargin   = 15 * time.Second
	leaseRenewAhead     = 45 * time.Second
	leaseFlushInterval  = 30 * time.Second
	leaseIdleAfter      = 5 * time.Minute
	leaseAwaitBusy      = 3 * time.Second
	leaseSettleOnExit   = 3 * time.Second
	leaseExhaustedPause = 5 * time.Second
)

var egressLeaseRenewals = tenantMetrics.NewCounterVec(prometheus.CounterOpts{
	Name: "control_plane_egress_lease_exchanges_total",
	Help: "Egress lease commands sent to the ledger, by result.",
}, []string{"result"})

// egressLeaseEnabled is false only for the literal value "off".
func egressLeaseEnabled(value string) bool {
	return !strings.EqualFold(strings.TrimSpace(value), "off")
}

// leaseBook serves billable responses from per-tenant leases. A response
// holds its budget in memory; Core sees one durable settle-and-renew per
// tenant per flush instead of a reserve and a complete per response.
//
// A settle is frozen in pending and resent byte-identical until Core answers
// definitively: Core keys its receipt on the settle id, so a retry that
// changed the bytes would be refused as a conflict, and one under a fresh id
// would be charged twice.
type leaseBook struct {
	mu       sync.Mutex
	tenants  map[string]*tenantLease
	exchange egressExchange
	now      func() time.Time
}

type tenantLease struct {
	mu      sync.Mutex
	dropped bool

	// The lease responses are served from. id is empty when there is none.
	id        string
	granted   uint64
	charged   uint64
	held      uint64
	expiresAt time.Time
	partial   bool
	// The part of granted drawn from the included allowance.
	grantedIncluded uint64

	// Bytes charged against leases already settled or settling.
	late uint64

	pending        *pendingSettle
	exhaustedUntil time.Time
	lastUsed       time.Time

	// The last Core snapshot, which already excludes the whole current grant.
	included uint64
	prepaid  uint64

	// One claimant at a time: a renewal, a flush, a retire or the exit settle.
	busy chan struct{}
}

type pendingSettle struct {
	command  egressCommand
	fromLate uint64
}

type leaseHold struct {
	tenant  *tenantLease
	leaseID string
	bytes   uint64
}

func newLeaseBook(exchange egressExchange) *leaseBook {
	return &leaseBook{tenants: make(map[string]*tenantLease), exchange: exchange, now: time.Now}
}

func (b *leaseBook) tenantState(name string) *tenantLease {
	b.mu.Lock()
	defer b.mu.Unlock()
	tl := b.tenants[name]
	if tl == nil {
		tl = &tenantLease{}
		b.tenants[name] = tl
	}
	return tl
}

func (b *leaseBook) acquire(ctx context.Context, tenant string) (leaseHold, error) {
	var lastErr error
	for cycles := 0; ; {
		tl := b.tenantState(tenant)
		tl.mu.Lock()
		if tl.dropped {
			tl.mu.Unlock()
			continue
		}
		if tl.busy != nil {
			done := tl.busy
			tl.mu.Unlock()
			if err := awaitLease(ctx, done); err != nil {
				return leaseHold{}, err
			}
			continue
		}
		now := b.now()
		tl.lastUsed = now
		if now.Before(tl.exhaustedUntil) {
			tl.mu.Unlock()
			return leaseHold{}, egressError{status: http.StatusPaymentRequired, reason: egressReasonExhausted}
		}
		if hold, ok := tl.takeLocked(now); ok {
			tl.mu.Unlock()
			return hold, nil
		}
		if cycles == 2 {
			tl.mu.Unlock()
			if lastErr != nil {
				return leaseHold{}, lastErr
			}
			return leaseHold{}, egressError{status: http.StatusTooManyRequests, reason: egressReasonPoolFull}
		}
		cycles++
		tl.busy = make(chan struct{})
		tl.mu.Unlock()
		lastErr = b.cycle(context.WithoutCancel(ctx), tenant, tl, true)
		b.release(tl)
	}
}

func awaitLease(ctx context.Context, done <-chan struct{}) error {
	timer := time.NewTimer(leaseAwaitBusy)
	defer timer.Stop()
	select {
	case <-done:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	case <-timer.C:
		return errors.New("egress ledger unavailable")
	}
}

func (t *tenantLease) availableLocked(now time.Time) uint64 {
	if t.id == "" || !now.Add(leaseSafetyMargin).Before(t.expiresAt) {
		return 0
	}
	return subSat(subSat(t.granted, t.charged), t.held)
}

// takeLocked holds one response budget. A smaller hold is given only when the
// lease itself is small because Core had no more balance to grant; a response
// that outgrows that hold is then refused with 402.
func (t *tenantLease) takeLocked(now time.Time) (leaseHold, bool) {
	avail := t.availableLocked(now)
	n := uint64(maxEgressResponse)
	if avail < n {
		if !t.partial || avail == 0 {
			return leaseHold{}, false
		}
		n = avail
	}
	t.held += n
	return leaseHold{tenant: t, leaseID: t.id, bytes: n}, true
}

func (b *leaseBook) release(tl *tenantLease) {
	tl.mu.Lock()
	done := tl.busy
	tl.busy = nil
	tl.mu.Unlock()
	close(done)
}

// cycle drains pending and late bytes, then renews the lease when wantLease
// is set or the current one is due. The caller holds the claim.
func (b *leaseBook) cycle(ctx context.Context, tenant string, tl *tenantLease, wantLease bool) error {
	for range 3 {
		tl.mu.Lock()
		if tl.pending == nil {
			tl.pending = tl.nextSettleLocked(b.now(), wantLease)
		}
		pending := tl.pending
		tl.mu.Unlock()
		if pending == nil {
			return nil
		}
		result, err := b.exchange(ctx, tenant, pending.command)
		if err := b.answer(tl, pending, result, err); err != nil {
			return err
		}
	}
	return nil
}

// nextSettleLocked freezes the next command, or returns nil when nothing is
// owed and no lease is wanted. Freezing a settle closes its lease: holds
// still out on it are charged to late when they finish.
func (t *tenantLease) nextSettleLocked(now time.Time, wantLease bool) *pendingSettle {
	due := t.id != "" && (t.charged > 0 || t.expiresAt.Sub(now) < leaseRenewAhead)
	if wantLease && (t.availableLocked(now) >= maxEgressResponse || now.Before(t.exhaustedUntil)) {
		wantLease = false
	}
	switch {
	case t.id != "" && (wantLease || due):
		sent := min(t.charged+t.late, t.granted)
		fromLate := subSat(sent, t.charged)
		command := egressCommand{Operation: egressOperationSettle, ID: t.id, Bytes: sent, Next: &leaseRequest{ID: uuid.NewString(), Bytes: maxEgressLeaseBytes}}
		t.closeLeaseLocked()
		return &pendingSettle{command: command, fromLate: fromLate}
	case t.late > 0:
		sent := min(t.late, maxEgressLeaseBytes)
		return &pendingSettle{command: egressCommand{Operation: egressOperationSettle, ID: uuid.NewString(), Bytes: sent}, fromLate: sent}
	case wantLease:
		return &pendingSettle{command: egressCommand{Operation: egressOperationLease, ID: uuid.NewString(), Bytes: maxEgressLeaseBytes}}
	}
	return nil
}

func (t *tenantLease) closeLeaseLocked() {
	t.id = ""
	t.granted = 0
	t.charged = 0
	t.held = 0
	t.partial = false
	t.grantedIncluded = 0
	t.expiresAt = time.Time{}
}

// answer applies Core's reply to a frozen command. Only a definitive answer
// clears pending; a timeout, a 503 or a 429 leaves it to be resent as is.
func (b *leaseBook) answer(tl *tenantLease, pending *pendingSettle, result egressAdmission, err error) error {
	tl.mu.Lock()
	defer tl.mu.Unlock()
	if err != nil {
		var refusal egressError
		if !errors.As(err, &refusal) || refusal.status == http.StatusTooManyRequests || refusal.status >= 500 {
			egressLeaseRenewals.WithLabelValues("retry").Inc()
			return err
		}
		tl.pending = nil
		if refusal.status == http.StatusPaymentRequired {
			tl.exhaustedUntil = b.now().Add(leaseExhaustedPause)
			egressLeaseRenewals.WithLabelValues("exhausted").Inc()
			return err
		}
		// A conflict or an invalid settle cannot be fixed by resending; the
		// bytes in it are written off here and the log is the record.
		log.Printf("egress lease command dropped: operation=%s bytes=%d status=%d reason=%s", pending.command.Operation, pending.command.Bytes, refusal.status, refusal.reason) //nolint:gosec // G706: operation and reason come from fixed sets.
		tl.late = subSat(tl.late, pending.fromLate)
		egressLeaseRenewals.WithLabelValues("dropped").Inc()
		return err
	}
	tl.pending = nil
	tl.late = subSat(tl.late, pending.fromLate)
	tl.included = result.Admission.Snapshot.IncludedAvailable
	tl.prepaid = result.Admission.Snapshot.PrepaidAvailable
	nextID := pending.command.ID
	requested := pending.command.Bytes
	switch {
	case pending.command.Next != nil:
		nextID, requested = pending.command.Next.ID, pending.command.Next.Bytes
	case pending.command.Operation != egressOperationLease:
		egressLeaseRenewals.WithLabelValues("settled").Inc()
		return nil
	}
	granted := result.Admission.ReservedBytes
	if granted == 0 || result.Admission.LeaseExpiresAt == nil {
		// Busy: other leases hold the balance and will release it.
		if result.Admission.NextRefused == egressReasonBusy {
			egressLeaseRenewals.WithLabelValues("busy").Inc()
			return nil
		}
		tl.exhaustedUntil = b.now().Add(leaseExhaustedPause)
		egressLeaseRenewals.WithLabelValues("exhausted").Inc()
		return nil
	}
	tl.id = nextID
	tl.granted = granted
	tl.partial = granted < requested
	tl.expiresAt = time.Unix(*result.Admission.LeaseExpiresAt, 0)
	tl.grantedIncluded = min(result.Admission.LeaseIncluded, granted)
	tl.exhaustedUntil = time.Time{}
	egressLeaseRenewals.WithLabelValues("granted").Inc()
	return nil
}

// finish returns the hold, records the charge and reports the remaining
// allowance. Core's snapshot excludes the whole grant, so the unspent part
// of the lease is added back, to included first because a settle charges
// included bytes first.
func (b *leaseBook) finish(hold leaseHold, charge uint64) (included, prepaid uint64) {
	tl := hold.tenant
	tl.mu.Lock()
	defer tl.mu.Unlock()
	if hold.leaseID != "" && hold.leaseID == tl.id {
		tl.held = subSat(tl.held, hold.bytes)
		tl.charged += charge
	} else {
		tl.late += charge
	}
	tl.lastUsed = b.now()
	left := subSat(subSat(tl.granted, tl.charged), tl.late)
	leftIncluded := subSat(tl.grantedIncluded, tl.granted-left)
	return tl.included + leftIncluded, tl.prepaid + left - leftIncluded
}

// sweep settles what each tenant owes, renews leases near expiry and retires
// tenants that have been idle with nothing owed.
func (b *leaseBook) sweep(ctx context.Context) {
	for name, tl := range b.snapshotTenants() {
		b.flushTenant(ctx, name, tl)
	}
}

func (b *leaseBook) snapshotTenants() map[string]*tenantLease {
	b.mu.Lock()
	defer b.mu.Unlock()
	states := make(map[string]*tenantLease, len(b.tenants))
	for name, tl := range b.tenants {
		states[name] = tl
	}
	return states
}

func (b *leaseBook) flushTenant(ctx context.Context, name string, tl *tenantLease) {
	now := b.now()
	tl.mu.Lock()
	if tl.busy != nil || tl.dropped {
		tl.mu.Unlock()
		return
	}
	quiet := tl.held == 0 && tl.charged == 0 && tl.late == 0 && tl.pending == nil
	idle := quiet && now.Sub(tl.lastUsed) > leaseIdleAfter
	due := tl.pending != nil || tl.late > 0 || (tl.id != "" && (tl.charged > 0 || tl.expiresAt.Sub(now) < leaseRenewAhead))
	if !idle && !due {
		tl.mu.Unlock()
		return
	}
	tl.busy = make(chan struct{})
	if idle && tl.id != "" {
		tl.pending = &pendingSettle{command: egressCommand{Operation: egressOperationSettle, ID: tl.id}}
		tl.closeLeaseLocked()
	}
	tl.mu.Unlock()

	err := b.cycle(ctx, name, tl, false)
	if err != nil {
		log.Printf("egress lease flush failed: %v", err)
	}
	if idle && err == nil {
		b.retire(name, tl)
	}
	b.release(tl)
}

// retire forgets a tenant whose lease is settled and that owes nothing. The
// caller holds the claim, so no response can take a hold meanwhile.
func (b *leaseBook) retire(name string, tl *tenantLease) {
	tl.mu.Lock()
	settled := tl.id == "" && tl.held == 0 && tl.late == 0 && tl.pending == nil
	if settled {
		tl.dropped = true
	}
	tl.mu.Unlock()
	if !settled {
		return
	}
	b.mu.Lock()
	if b.tenants[name] == tl {
		delete(b.tenants, name)
	}
	b.mu.Unlock()
}

// settleAll settles every tenant at shutdown, in parallel, after the flusher
// has stopped and requests have drained.
func (b *leaseBook) settleAll(ctx context.Context) {
	var wg sync.WaitGroup
	for name, tl := range b.snapshotTenants() {
		wg.Go(func() {
			if err := b.settleOut(ctx, name, tl); err != nil {
				log.Printf("egress lease shutdown settle failed: %v", err)
			}
		})
	}
	wg.Wait()
}

func (b *leaseBook) settleOut(ctx context.Context, name string, tl *tenantLease) error {
	for {
		tl.mu.Lock()
		if tl.busy == nil {
			break
		}
		done := tl.busy
		tl.mu.Unlock()
		if err := awaitLease(ctx, done); err != nil {
			return err
		}
	}
	tl.busy = make(chan struct{})
	if tl.pending == nil && tl.id != "" {
		sent := min(tl.charged+tl.late, tl.granted)
		tl.pending = &pendingSettle{command: egressCommand{Operation: egressOperationSettle, ID: tl.id, Bytes: sent}, fromLate: subSat(sent, tl.charged)}
		tl.closeLeaseLocked()
	}
	tl.mu.Unlock()
	err := b.cycle(ctx, name, tl, false)
	b.release(tl)
	return err
}

func (b *leaseBook) run(ctx context.Context) {
	ticker := time.NewTicker(leaseFlushInterval)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			b.sweep(ctx)
		}
	}
}

func subSat(a, b uint64) uint64 {
	if b >= a {
		return 0
	}
	return a - b
}
