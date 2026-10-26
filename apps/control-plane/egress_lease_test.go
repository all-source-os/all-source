package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"slices"
	"strconv"
	"sync"
	"testing"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/go-resty/resty/v2"

	"github.com/allsource/control-plane/internal/domain/entities"
)

const leaseTestTTL = 120 * time.Second

type leaseTestClock struct {
	mu sync.Mutex
	at time.Time
}

func (c *leaseTestClock) now() time.Time {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.at
}

func (c *leaseTestClock) advance(d time.Duration) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.at = c.at.Add(d)
}

type fakeLease struct {
	granted uint64
	expires time.Time
}

// fakeLeaseLedger follows Core's lease accounting, receipts included: a
// settle replayed with the same bytes is not charged again, and one replayed
// with different bytes is a conflict.
type fakeLeaseLedger struct {
	mu       sync.Mutex
	clock    *leaseTestClock
	total    uint64
	leases   map[string]fakeLease
	leased   uint64
	receipts map[string]uint64
	commands []egressCommand
	settled  uint64
}

func (f *fakeLeaseLedger) exchange(_ context.Context, _ string, command egressCommand) (egressAdmission, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	now := f.clock.now()
	for id, lease := range f.leases {
		if !now.Before(lease.expires) {
			delete(f.leases, id)
			f.leased -= lease.granted
		}
	}
	f.commands = append(f.commands, command)
	switch command.Operation {
	case egressOperationLease:
		granted := f.grantLocked(command.ID, command.Bytes, now)
		if granted == 0 {
			return egressAdmission{}, egressError{status: http.StatusPaymentRequired, reason: egressReasonExhausted}
		}
		return f.admissionLocked(granted, now), nil
	case egressOperationSettle:
		if charged, ok := f.receipts[command.ID]; ok {
			if charged != command.Bytes {
				return egressAdmission{}, egressError{status: http.StatusConflict, reason: "conflict"}
			}
		} else {
			charge := command.Bytes
			if lease, ok := f.leases[command.ID]; ok {
				if command.Bytes > lease.granted {
					return egressAdmission{}, egressError{status: http.StatusBadRequest, reason: "invalid"}
				}
				delete(f.leases, command.ID)
				f.leased -= lease.granted
			} else {
				charge = min(command.Bytes, maxEgressLeaseBytes, f.available())
			}
			f.total -= charge
			f.settled += charge
			f.receipts[command.ID] = command.Bytes
		}
		if command.Next == nil {
			return f.admissionLocked(0, now), nil
		}
		return f.admissionLocked(f.grantLocked(command.Next.ID, command.Next.Bytes, now), now), nil
	}
	return egressAdmission{}, errors.New("unexpected operation " + command.Operation)
}

func (f *fakeLeaseLedger) available() uint64 { return subSat(f.total, f.leased) }

func (f *fakeLeaseLedger) grantLocked(id string, want uint64, now time.Time) uint64 {
	if lease, ok := f.leases[id]; ok {
		return lease.granted
	}
	granted := min(want, f.available())
	if granted > 0 {
		f.leases[id] = fakeLease{granted: granted, expires: now.Add(leaseTestTTL)}
		f.leased += granted
	}
	return granted
}

func (f *fakeLeaseLedger) admissionLocked(reserved uint64, now time.Time) egressAdmission {
	var admission egressAdmission
	admission.Protocol = fundedEgressProtocol
	admission.Admission.ReservedBytes = reserved
	admission.Admission.LeaseIncluded = reserved
	if reserved > 0 {
		expiry := now.Add(leaseTestTTL).Unix()
		admission.Admission.LeaseExpiresAt = &expiry
	}
	admission.Admission.Snapshot.IncludedAvailable = f.available()
	return admission
}

func (f *fakeLeaseLedger) operations() []string {
	f.mu.Lock()
	defer f.mu.Unlock()
	ops := make([]string, len(f.commands))
	for i, command := range f.commands {
		ops[i] = command.Operation
	}
	return ops
}

func (f *fakeLeaseLedger) sent() []egressCommand {
	f.mu.Lock()
	defer f.mu.Unlock()
	return slices.Clone(f.commands)
}

func (f *fakeLeaseLedger) settledBytes() uint64 {
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.settled
}

func newLeaseHarness(total uint64) (*leaseBook, *fakeLeaseLedger, *leaseTestClock) {
	clock := &leaseTestClock{at: time.Date(2026, 10, 26, 12, 0, 0, 0, time.UTC)}
	ledger := &fakeLeaseLedger{clock: clock, total: total, leases: map[string]fakeLease{}, receipts: map[string]uint64{}}
	book := newLeaseBook(ledger.exchange)
	book.now = clock.now
	return book, ledger, clock
}

func leaseTestRouter(book *leaseBook, handler gin.HandlerFunc) *gin.Engine {
	router := gin.New()
	cp := &ControlPlane{egressExchange: book.exchange, leases: book}
	router.Use(func(c *gin.Context) { c.Set("auth_tenant_id", c.GetHeader("Test-Tenant")); c.Next() })
	api := router.Group("/api/v1")
	api.Use(cp.enforceEgress)
	api.GET("/events/query", handler)
	return router
}

func serveLeaseRead(router *gin.Engine) *httptest.ResponseRecorder {
	request := httptest.NewRequestWithContext(context.Background(), http.MethodGet, "/api/v1/events/query", http.NoBody)
	request.Header.Set("Test-Tenant", "a")
	response := httptest.NewRecorder()
	router.ServeHTTP(response, request)
	return response
}

// loseNextReply commits the next command matching op at the fake, then
// reports a timeout, as a reply lost after Core wrote its receipt would.
func loseNextReply(book *leaseBook, ledger *fakeLeaseLedger, op string) {
	var once sync.Once
	book.exchange = func(ctx context.Context, tenant string, command egressCommand) (egressAdmission, error) {
		result, err := ledger.exchange(ctx, tenant, command)
		lost := false
		if command.Operation == op {
			once.Do(func() { lost = true })
		}
		if lost {
			return egressAdmission{}, context.DeadlineExceeded
		}
		return result, err
	}
}

func TestLeaseSequentialReadsNeedOneLeaseCall(t *testing.T) {
	book, ledger, _ := newLeaseHarness(1 << 40)
	router := leaseTestRouter(book, func(c *gin.Context) { c.String(200, "abc") })
	for range 100 {
		if response := serveLeaseRead(router); response.Code != http.StatusOK {
			t.Fatalf("status %d: %s", response.Code, response.Body.String())
		}
	}
	if got := ledger.operations(); !slices.Equal(got, []string{"lease"}) {
		t.Fatalf("ledger operations %v, want one lease", got)
	}
}

// The 16 response slots all busy on one tenant must not force a renewal per
// response: a lease covers far more than 16 holds of 16 MiB.
func TestLeaseConcurrentColdStartSingleLedgerCall(t *testing.T) {
	book, ledger, _ := newLeaseHarness(1 << 40)
	book.exchange = func(ctx context.Context, tenant string, command egressCommand) (egressAdmission, error) {
		time.Sleep(20 * time.Millisecond)
		return ledger.exchange(ctx, tenant, command)
	}
	const concurrent = 16
	holds := make([]leaseHold, concurrent)
	errs := make([]error, concurrent)
	var wg sync.WaitGroup
	for i := range concurrent {
		wg.Go(func() { holds[i], errs[i] = book.acquire(t.Context(), "a") })
	}
	wg.Wait()
	for i, err := range errs {
		if err != nil {
			t.Fatalf("acquire %d: %v", i, err)
		}
	}
	for _, hold := range holds {
		book.finish(hold, 1)
	}
	for range concurrent {
		hold, err := book.acquire(t.Context(), "a")
		if err != nil {
			t.Fatal(err)
		}
		book.finish(hold, 1)
	}
	if got := ledger.operations(); !slices.Equal(got, []string{"lease"}) {
		t.Fatalf("ledger operations %v, want one lease for %d concurrent reads twice over", got, concurrent)
	}
}

func TestLeaseSettleChargesEqualBodyBytes(t *testing.T) {
	book, ledger, _ := newLeaseHarness(1 << 40)
	sizes := []int{10, 20, 30}
	next := 0
	router := leaseTestRouter(book, func(c *gin.Context) {
		c.Data(http.StatusOK, "text/plain", bytes.Repeat([]byte("x"), sizes[next]))
		next++
	})
	for range sizes {
		if response := serveLeaseRead(router); response.Code != http.StatusOK {
			t.Fatalf("status %d", response.Code)
		}
	}
	book.sweep(t.Context())
	if got := ledger.settledBytes(); got != 60 {
		t.Fatalf("settled %d bytes, want 60 (sum of bodies)", got)
	}
	if got := ledger.operations(); !slices.Equal(got, []string{"lease", "settle"}) {
		t.Fatalf("ledger operations %v", got)
	}
	book.sweep(t.Context())
	if got := ledger.operations(); len(got) != 2 {
		t.Fatalf("a sweep with nothing owed called the ledger: %v", got)
	}
}

func TestLeaseFlusherRenewsNearExpiry(t *testing.T) {
	book, ledger, clock := newLeaseHarness(1 << 40)
	router := leaseTestRouter(book, func(c *gin.Context) { c.String(200, "abc") })
	serveLeaseRead(router)
	book.sweep(t.Context())
	clock.advance(100 * time.Second)
	book.sweep(t.Context())
	if got := ledger.operations(); !slices.Equal(got, []string{"lease", "settle", "settle"}) {
		t.Fatalf("ledger operations %v, want a renewal inside the 45 s window", got)
	}
	clock.advance(10 * time.Second)
	book.sweep(t.Context())
	if got := ledger.operations(); len(got) != 3 {
		t.Fatalf("renewed a fresh lease: %v", got)
	}
	if response := serveLeaseRead(router); response.Code != http.StatusOK {
		t.Fatalf("renewed lease did not serve: %d", response.Code)
	}
}

func TestLeaseLateSettleChargesExpiredLease(t *testing.T) {
	book, ledger, clock := newLeaseHarness(1 << 40)
	router := leaseTestRouter(book, func(c *gin.Context) { c.String(200, "abcde") })
	serveLeaseRead(router)
	clock.advance(200 * time.Second)
	book.sweep(t.Context())
	if got := ledger.settledBytes(); got != 5 {
		t.Fatalf("expired lease charged %d bytes, want 5", got)
	}
	if response := serveLeaseRead(router); response.Code != http.StatusOK {
		t.Fatalf("lease renewed after expiry did not serve: %d", response.Code)
	}
}

// A settle whose reply is lost after Core wrote its receipt is resent with
// the same id and bytes: charged once, and the tenant keeps being served.
func TestLeaseLostSettleReplyIsResentUnchanged(t *testing.T) {
	book, ledger, _ := newLeaseHarness(1 << 40)
	router := leaseTestRouter(book, func(c *gin.Context) { c.String(200, "abcdefg") })
	serveLeaseRead(router)
	loseNextReply(book, ledger, egressOperationSettle)
	book.sweep(t.Context())
	for range 3 {
		if response := serveLeaseRead(router); response.Code != http.StatusOK {
			t.Fatalf("tenant refused after a lost settle reply: %d %s", response.Code, response.Body.String())
		}
	}
	book.sweep(t.Context())

	var settles []egressCommand
	for _, command := range ledger.sent() {
		if command.Operation == egressOperationSettle {
			settles = append(settles, command)
		}
	}
	if len(settles) < 2 || settles[0].ID != settles[1].ID || settles[0].Bytes != settles[1].Bytes || settles[0].Next.ID != settles[1].Next.ID {
		t.Fatalf("the lost settle was not resent unchanged: %#v", settles)
	}
	if got := ledger.settledBytes(); got != 7*4 {
		t.Fatalf("settled %d bytes, want %d: four reads of 7 bytes, each charged once", got, 7*4)
	}
}

// A charge that lands after its lease was frozen for settling goes to late
// bytes, and a lost reply to their settle does not charge them twice.
func TestLeaseChargeAfterFreezeIsSettledOnce(t *testing.T) {
	book, ledger, _ := newLeaseHarness(1 << 40)
	first, err := book.acquire(t.Context(), "a")
	if err != nil {
		t.Fatal(err)
	}
	book.finish(first, 3)
	inFlight, err := book.acquire(t.Context(), "a")
	if err != nil {
		t.Fatal(err)
	}
	book.sweep(t.Context())
	book.finish(inFlight, 11)
	tl := book.tenantState("a")
	tl.mu.Lock()
	late := tl.late
	tl.mu.Unlock()
	if late != 11 {
		t.Fatalf("late = %d, want the 11 bytes charged after the freeze", late)
	}
	book.sweep(t.Context())
	book.sweep(t.Context())
	if got := ledger.settledBytes(); got != 14 {
		t.Fatalf("settled %d bytes, want 14", got)
	}
}

func TestLeaseLostLateSettleReplyChargesOnce(t *testing.T) {
	book, ledger, clock := newLeaseHarness(1 << 40)
	hold, err := book.acquire(t.Context(), "a")
	if err != nil {
		t.Fatal(err)
	}
	book.sweep(t.Context())
	book.finish(hold, 0)
	clock.advance(time.Second)
	tl := book.tenantState("a")
	tl.mu.Lock()
	tl.closeLeaseLocked()
	tl.late = 40
	tl.mu.Unlock()
	loseNextReply(book, ledger, egressOperationSettle)
	book.sweep(t.Context())
	book.sweep(t.Context())
	if got := ledger.settledBytes(); got != 40 {
		t.Fatalf("settled %d bytes, want 40 charged once", got)
	}
}

// An idle tenant being retired cannot hand out a hold from the lease it is
// settling; a read arriving then waits and gets a fresh lease.
func TestLeaseRetireDoesNotLoseAConcurrentRead(t *testing.T) {
	book, ledger, clock := newLeaseHarness(1 << 40)
	hold, err := book.acquire(t.Context(), "a")
	if err != nil {
		t.Fatal(err)
	}
	book.finish(hold, 0)
	clock.advance(leaseIdleAfter + time.Second)

	entered := make(chan struct{})
	proceed := make(chan struct{})
	var once sync.Once
	book.exchange = func(ctx context.Context, tenant string, command egressCommand) (egressAdmission, error) {
		if command.Operation == egressOperationSettle {
			once.Do(func() { close(entered); <-proceed })
		}
		return ledger.exchange(ctx, tenant, command)
	}
	swept := make(chan struct{})
	go func() { book.sweep(t.Context()); close(swept) }()
	<-entered

	read := make(chan leaseHold)
	go func() {
		hold, err := book.acquire(context.Background(), "a")
		if err != nil {
			t.Error(err)
		}
		read <- hold
	}()
	close(proceed)
	<-swept
	late := <-read
	book.finish(late, 13)
	book.sweep(t.Context())
	if got := ledger.settledBytes(); got != 13 {
		t.Fatalf("settled %d bytes, want the 13 served during the retire", got)
	}
}

func TestLeaseShutdownSettlesWhatIsOwed(t *testing.T) {
	book, ledger, _ := newLeaseHarness(1 << 40)
	hold, err := book.acquire(t.Context(), "a")
	if err != nil {
		t.Fatal(err)
	}
	book.finish(hold, 21)
	book.settleAll(t.Context())
	sent := ledger.sent()
	last := sent[len(sent)-1]
	if ledger.settledBytes() != 21 || last.Operation != egressOperationSettle || last.Next != nil {
		t.Fatalf("shutdown settle %#v, settled %d", last, ledger.settledBytes())
	}
}

func TestLeaseExhaustedAccountDeniesAndBacksOff(t *testing.T) {
	book, ledger, clock := newLeaseHarness(0)
	for range 5 {
		_, err := book.acquire(t.Context(), "a")
		var refusal egressError
		if !errors.As(err, &refusal) || refusal.status != http.StatusPaymentRequired || refusal.reason != egressReasonExhausted {
			t.Fatalf("got %#v", err)
		}
	}
	if got := ledger.operations(); !slices.Equal(got, []string{"lease"}) {
		t.Fatalf("an exhausted tenant called the ledger per request: %v", got)
	}
	clock.advance(leaseExhaustedPause)
	if _, err := book.acquire(t.Context(), "a"); err == nil {
		t.Fatal("an account with nothing left was served")
	}
	if got := ledger.operations(); len(got) != 2 {
		t.Fatalf("no new lease attempt after the pause: %v", got)
	}
}

func TestLeaseOneMiBLeftServesThenDenies(t *testing.T) {
	book, _, _ := newLeaseHarness(1 << 20)
	router := leaseTestRouter(book, func(c *gin.Context) {
		c.Data(http.StatusOK, "application/octet-stream", bytes.Repeat([]byte("x"), 1<<20))
	})
	first := serveLeaseRead(router)
	if first.Code != http.StatusOK || first.Body.Len() != 1<<20 {
		t.Fatalf("small remaining allowance: status %d body %d", first.Code, first.Body.Len())
	}
	second := serveLeaseRead(router)
	if second.Code != http.StatusPaymentRequired {
		t.Fatalf("exhausted account: status %d", second.Code)
	}
}

// Core's snapshot excludes the whole grant, so the header adds back what the
// lease has not spent.
func TestLeaseRemainingHeaderCountsTheUnspentLease(t *testing.T) {
	book, _, _ := newLeaseHarness(1 << 40)
	router := leaseTestRouter(book, func(c *gin.Context) { c.String(200, "abc") })
	response := serveLeaseRead(router)
	got, err := strconv.ParseUint(response.Header().Get("X-Egress-Included-Remaining"), 10, 64)
	if err != nil {
		t.Fatal(err)
	}
	if want := uint64(1<<40 - 3); got != want {
		t.Fatalf("included remaining %d, want %d", got, want)
	}
}

// A lease that took the last included bytes leaves Core's snapshot at zero
// included; the headers still report the unspent included part as included.
func TestLeaseRemainingHeadersFollowTheGrantSplit(t *testing.T) {
	for _, tc := range []struct {
		name                   string
		leaseIncluded, prepaid uint64
		wantIncluded, wantPaid uint64
	}{
		{"all included, no prepaid", 1000, 0, 997, 0},
		{"split across included and prepaid", 600, 50, 597, 450},
	} {
		t.Run(tc.name, func(t *testing.T) {
			clock := &leaseTestClock{at: time.Date(2026, 10, 26, 12, 0, 0, 0, time.UTC)}
			book := newLeaseBook(func(_ context.Context, _ string, command egressCommand) (egressAdmission, error) {
				var admission egressAdmission
				admission.Protocol = fundedEgressProtocol
				admission.Admission.Snapshot.PrepaidAvailable = tc.prepaid
				if command.Operation == egressOperationLease || command.Next != nil {
					admission.Admission.ReservedBytes = 1000
					admission.Admission.LeaseIncluded = tc.leaseIncluded
					expiry := clock.now().Add(leaseTestTTL).Unix()
					admission.Admission.LeaseExpiresAt = &expiry
				}
				return admission, nil
			})
			book.now = clock.now
			router := leaseTestRouter(book, func(c *gin.Context) { c.String(200, "abc") })
			response := serveLeaseRead(router)
			if response.Code != http.StatusOK {
				t.Fatalf("status %d: %s", response.Code, response.Body.String())
			}
			for header, want := range map[string]uint64{
				"X-Egress-Included-Remaining": tc.wantIncluded,
				"X-Egress-Prepaid-Remaining":  tc.wantPaid,
			} {
				got, err := strconv.ParseUint(response.Header().Get(header), 10, 64)
				if err != nil || got != want {
					t.Fatalf("%s = %q, want %d", header, response.Header().Get(header), want)
				}
			}
		})
	}
}

// Core refuses a next lease as busy when other leases hold the balance; that
// must not pause the tenant as if it were out of allowance.
func TestLeaseBusyNextIsNotAnExhaustedPause(t *testing.T) {
	for _, tc := range []struct {
		refused   string
		wantPause bool
	}{{egressReasonBusy, false}, {egressReasonExhausted, true}} {
		book, _, _ := newLeaseHarness(0)
		tl := &tenantLease{}
		pending := &pendingSettle{command: egressCommand{Operation: egressOperationSettle, ID: "a", Bytes: 1, Next: &leaseRequest{ID: "b", Bytes: maxEgressLeaseBytes}}}
		var result egressAdmission
		result.Admission.NextRefused = tc.refused
		if err := book.answer(tl, pending, result, nil); err != nil {
			t.Fatal(err)
		}
		if paused := !tl.exhaustedUntil.IsZero(); paused != tc.wantPause {
			t.Fatalf("next_refused=%s: paused %v, want %v", tc.refused, paused, tc.wantPause)
		}
	}
}

func TestEgressLeaseFlagOffUsesReserveComplete(t *testing.T) {
	for _, flag := range []struct {
		value string
		want  bool
	}{{"", true}, {"on", true}, {"off", false}, {" OFF ", false}} {
		if got := egressLeaseEnabled(flag.value); got != flag.want {
			t.Fatalf("egressLeaseEnabled(%q) = %v, want %v", flag.value, got, flag.want)
		}
	}
	ledger := &testEgressLedger{left: map[string]uint64{"a": 1 << 20}, reservations: map[string]uint64{}}
	router := egressTestRouter(ledger.exchange, func(c *gin.Context) { c.String(200, "abc") })
	if response := serveLeaseRead(router); response.Code != http.StatusOK {
		t.Fatalf("status %d", response.Code)
	}
	if len(ledger.settlements) != 1 || ledger.settlements[0].Operation != egressOperationComplete || ledger.settlements[0].Bytes != 3 {
		t.Fatalf("settlements %#v", ledger.settlements)
	}
}

func TestExchangeEgressLeaseWireShapes(t *testing.T) {
	var bodies []map[string]any
	core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Error(err)
		}
		bodies = append(bodies, body)
		w.Header().Set("Content-Type", "application/json")
		reply := `{"protocol":"funded-egress-v1","admission":{"snapshot":{"included_available":5,"prepaid_available":0},"reserved_bytes":0,"replayed":false}}`
		if body["operation"] == egressOperationLease {
			reply = `{"protocol":"funded-egress-v1","admission":{"snapshot":{"included_available":5,"prepaid_available":0},"reserved_bytes":1000,"replayed":false,"lease_expires_at":1700000120}}`
		}
		if _, err := w.Write([]byte(reply)); err != nil {
			t.Error(err)
		}
	}))
	t.Cleanup(core.Close)
	delegation, err := newDelegationClient(core.URL, core.URL, core.URL,
		func(_, _ string, _ entities.Role) (string, error) { return "tok", nil }, http.DefaultClient)
	if err != nil {
		t.Fatal(err)
	}
	cp := &ControlPlane{delegation: delegation, client: resty.New()}

	granted, err := cp.exchangeEgress(t.Context(), "acme", egressCommand{Operation: egressOperationLease, ID: "lease-1", Bytes: maxEgressLeaseBytes})
	if err != nil {
		t.Fatal(err)
	}
	if granted.Admission.ReservedBytes != 1000 || granted.Admission.LeaseExpiresAt == nil || *granted.Admission.LeaseExpiresAt != 1700000120 {
		t.Fatalf("lease reply %#v", granted.Admission)
	}
	settled, err := cp.exchangeEgress(t.Context(), "acme", egressCommand{
		Operation: egressOperationSettle, ID: "lease-1", Bytes: 42,
		Next: &leaseRequest{ID: "lease-2", Bytes: maxEgressLeaseBytes},
	})
	if err != nil {
		t.Fatal(err)
	}
	if settled.Admission.ReservedBytes != 0 || settled.Admission.LeaseExpiresAt != nil {
		t.Fatalf("settle reply %#v", settled.Admission)
	}
	zero, err := cp.exchangeEgress(t.Context(), "acme", egressCommand{Operation: egressOperationSettle, ID: "lease-3", Bytes: 0})
	if err != nil {
		t.Fatal(err)
	}
	_ = zero
	if len(bodies) != 3 {
		t.Fatalf("core calls = %d", len(bodies))
	}
	if bodies[0]["operation"] != "lease" || bodies[0]["id"] != "lease-1" || bodies[0]["bytes"] != float64(maxEgressLeaseBytes) {
		t.Fatalf("lease body %#v", bodies[0])
	}
	if _, ok := bodies[0]["next"]; ok {
		t.Fatalf("lease body carries next: %#v", bodies[0])
	}
	next, ok := bodies[1]["next"].(map[string]any)
	if !ok {
		t.Fatalf("settle body has no next: %#v", bodies[1])
	}
	if bodies[1]["operation"] != "settle" || bodies[1]["bytes"] != float64(42) || next["id"] != "lease-2" || next["bytes"] != float64(maxEgressLeaseBytes) {
		t.Fatalf("settle body %#v", bodies[1])
	}
	if got, ok := bodies[2]["bytes"]; !ok || got != float64(0) {
		t.Fatalf("a zero-byte settle must still send bytes, Core requires the field: %#v", bodies[2])
	}
}

func TestExchangeEgressRateLimitedWithoutReason(t *testing.T) {
	core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Retry-After", "7")
		w.WriteHeader(http.StatusTooManyRequests)
		if _, err := w.Write([]byte("Too many requests")); err != nil {
			t.Error(err)
		}
	}))
	t.Cleanup(core.Close)
	delegation, err := newDelegationClient(core.URL, core.URL, core.URL,
		func(_, _ string, _ entities.Role) (string, error) { return "tok", nil }, http.DefaultClient)
	if err != nil {
		t.Fatal(err)
	}
	cp := &ControlPlane{delegation: delegation, client: resty.New()}
	_, err = cp.exchangeEgress(t.Context(), "acme", egressCommand{Operation: egressOperationLease, ID: "lease-1", Bytes: 1})
	var refusal egressError
	if !errors.As(err, &refusal) || refusal.status != http.StatusTooManyRequests || refusal.reason != egressReasonRateLimited || refusal.retryAfter != "7" {
		t.Fatalf("got %#v", err)
	}
}

func TestEgressRateLimitedPassesCoreRetryAfter(t *testing.T) {
	for retryAfter, want := range map[string]string{"7": "7", "": "1", "Wed, 21 Oct 2026 07:28:00 GMT": "1"} {
		router := egressTestRouter(func(context.Context, string, egressCommand) (egressAdmission, error) {
			return egressAdmission{}, egressError{status: http.StatusTooManyRequests, reason: egressReasonRateLimited, retryAfter: retryAfter}
		}, func(c *gin.Context) { c.String(200, "secret") })
		response := serveLeaseRead(router)
		if response.Code != http.StatusTooManyRequests || response.Header().Get("Retry-After") != want {
			t.Fatalf("retryAfter %q: status %d Retry-After %q", retryAfter, response.Code, response.Header().Get("Retry-After"))
		}
		if !bytes.Contains(response.Body.Bytes(), []byte(`"reason":"rate_limited"`)) {
			t.Fatalf("body %s", response.Body.String())
		}
	}
}
