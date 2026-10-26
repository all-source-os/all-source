package main

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"net"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/google/uuid"
)

const (
	maxEgressResponse       = 16 * 1024 * 1024
	egressOperationReserve  = "reserve"
	egressOperationComplete = "complete"
	egressOperationLease    = "lease"
	egressOperationSettle   = "settle"
	fundedEgressProtocol    = "funded-egress-v1"
)

var egressResponseSlots = make(chan struct{}, 16)

const viewChangesGrace = 5 * time.Second

// viewChangesLane holds view long-polls apart from egressResponseSlots, so an
// idle wait never takes a slot a normal read needs.
var viewChangesLane = newLongPollLane(64, 8)

type longPollLane struct {
	mu        sync.Mutex
	slots     chan struct{}
	held      map[string]int
	perTenant int
}

func newLongPollLane(slots, perTenant int) *longPollLane {
	return &longPollLane{slots: make(chan struct{}, slots), held: map[string]int{}, perTenant: perTenant}
}

func (l *longPollLane) acquire(tenant string) bool {
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.held[tenant] >= l.perTenant {
		return false
	}
	select {
	case l.slots <- struct{}{}:
		l.held[tenant]++
		return true
	default:
		return false
	}
}

func (l *longPollLane) release(tenant string) {
	l.mu.Lock()
	defer l.mu.Unlock()
	<-l.slots
	l.held[tenant]--
	if l.held[tenant] <= 0 {
		delete(l.held, tenant)
	}
}

func (l *longPollLane) occupancy(tenant string) (occupied, held int) {
	l.mu.Lock()
	defer l.mu.Unlock()
	return len(l.slots), l.held[tenant]
}

// viewChangesHold reports how long Core may hold a view changes request. A
// zero or absent wait is an ordinary read and takes the ordinary lane.
func viewChangesHold(r *http.Request, route string) (time.Duration, bool) {
	if r.Method != http.MethodGet || route != viewChangesRoute {
		return 0, false
	}
	seconds, ok := viewChangesWait(r.URL.Query().Get("wait"))
	if !ok || seconds == 0 {
		return 0, false
	}
	return time.Duration(seconds) * time.Second, true
}

var errInvalidEgressReservation = errors.New("invalid egress reservation")

type egressCommand struct {
	Operation string        `json:"operation"`
	ID        string        `json:"id,omitempty"`
	Bytes     uint64        `json:"bytes,omitempty"`
	Next      *leaseRequest `json:"next,omitempty"`
}

type leaseRequest struct {
	ID    string `json:"id"`
	Bytes uint64 `json:"bytes"`
}

type egressAdmission struct {
	Protocol  string `json:"protocol"`
	Admission struct {
		ReservedBytes  uint64 `json:"reserved_bytes"`
		LeaseExpiresAt *int64 `json:"lease_expires_at,omitempty"`
		LeaseIncluded  uint64 `json:"lease_included,omitempty"`
		NextRefused    string `json:"next_refused,omitempty"`
		Snapshot       struct {
			IncludedAvailable uint64 `json:"included_available"`
			PrepaidAvailable  uint64 `json:"prepaid_available"`
		} `json:"snapshot"`
	} `json:"admission"`
}

type egressError struct {
	status     int
	reason     string
	retryAfter string
}

const (
	egressReasonPoolFull    = "pool_full"
	egressReasonRateLimited = "rate_limited"
	egressReasonUnavailable = "ledger_unavailable"
)

func (e egressError) Error() string { return "egress admission denied" }

type egressExchange func(context.Context, string, egressCommand) (egressAdmission, error)

func (cp *ControlPlane) exchangeEgress(ctx context.Context, tenant string, command egressCommand) (egressAdmission, error) {
	var result egressAdmission
	deadline, cancel := context.WithTimeout(ctx, 3*time.Second)
	defer cancel()
	target := *cp.delegation.coreForTenant(tenant)
	target.Path = strings.TrimRight(target.Path, "/") + "/api/v1/tenants/" + url.PathEscape(tenant) + "/egress"
	payload := map[string]any{"operation": command.Operation, "id": command.ID}
	if command.Operation != egressOperationReserve {
		payload["bytes"] = command.Bytes
	}
	if command.Next != nil {
		payload["next"] = command.Next
	}
	for attempt := 0; attempt < 2; attempt++ {
		response, err := cp.client.R().SetContext(deadline).SetBody(payload).SetResult(&result).Post(target.String())
		if err != nil {
			if deadline.Err() != nil {
				return result, deadline.Err()
			}
			continue
		}
		if response.StatusCode() == http.StatusServiceUnavailable {
			continue
		}
		if !response.IsSuccess() {
			var denial struct {
				Reason string `json:"reason"`
			}
			if json.Unmarshal(response.Body(), &denial) != nil {
				denial.Reason = ""
			}
			refusal := egressError{status: response.StatusCode(), reason: denial.Reason}
			if refusal.status == http.StatusTooManyRequests && refusal.reason == "" {
				// Core's per-credential limiter answers plain text, with no ledger reason.
				refusal.reason = egressReasonRateLimited
				refusal.retryAfter = response.Header().Get("Retry-After")
			}
			return result, refusal
		}
		if result.Protocol != fundedEgressProtocol {
			return result, errors.New("unsupported egress ledger")
		}
		return result, nil
	}
	return result, errors.New("egress ledger unavailable")
}

func (cp *ControlPlane) enforceEgress(c *gin.Context) {
	if cp.egressExchange == nil || !billableEgressPath(c.Request.URL.Path) {
		c.Next()
		return
	}
	billable := billableEgressRequest(c.Request)
	tenant, ok := authTenantFromContext(c)
	if !ok {
		if !billable {
			c.Next()
			return
		}
		c.AbortWithStatusJSON(http.StatusUnauthorized, gin.H{"error": "authentication_required"})
		return
	}
	lane := admissionLane(c.Request, c.FullPath())
	wait, longPoll := viewChangesHold(c.Request, c.FullPath())
	poolOccupied, poolCapacity := len(egressResponseSlots), cap(egressResponseSlots)
	if longPoll {
		poolOccupied, poolCapacity = len(viewChangesLane.slots), cap(viewChangesLane.slots)
	}
	ticket, refusal, headers := cp.admission.admit(tenant, lane, poolOccupied, poolCapacity, time.Now())
	if cp.admission.enforcing() {
		for name, values := range headers {
			c.Writer.Header()[name] = values
		}
	}
	if refusal != nil {
		c.AbortWithStatusJSON(refusal.status, writeAdmissionRefusal(c.Writer, refusal))
		return
	}
	if longPoll {
		// The lane's per-tenant cap bounds held requests; counting them as
		// in-flight would let idle waits refuse the tenant's other reads.
		ticket.release(0, 0)
		ticket = nil
	}
	if !billable {
		defer func() { ticket.release(uint64(max(0, c.Writer.Size())), c.Writer.Status()) }() //nolint:gosec // Size is clamped nonnegative.
		c.Next()
		return
	}
	var charged uint64
	status := 0
	defer func() { ticket.release(charged, status) }()
	if longPoll {
		defer func() { cp.admission.chargeBytes(tenant, charged) }()
		if !viewChangesLane.acquire(tenant) {
			admissionRefused.WithLabelValues(lane, admissionScopeGlobal, tenantLabel(tenant)).Inc()
			rejectEgress(c, egressError{status: http.StatusTooManyRequests, reason: egressReasonPoolFull})
			return
		}
		defer viewChangesLane.release(tenant)
	} else {
		select {
		case egressResponseSlots <- struct{}{}:
			defer func() { <-egressResponseSlots }()
		default:
			admissionRefused.WithLabelValues(lane, admissionScopeGlobal, tenantLabel(tenant)).Inc()
			rejectEgress(c, egressError{status: http.StatusTooManyRequests, reason: egressReasonPoolFull})
			return
		}
	}
	hold, err := cp.reserveEgress(c.Request.Context(), tenant)
	if errors.Is(err, errInvalidEgressReservation) {
		c.AbortWithStatusJSON(http.StatusServiceUnavailable, gin.H{"error": "invalid_egress_reservation"})
		return
	}
	if err != nil {
		rejectEgress(c, err)
		return
	}
	capacity := hold.capacity
	original := c.Writer
	deadline := 10 * time.Second
	if longPoll {
		deadline = wait + viewChangesGrace
	}
	requestContext, cancel := context.WithTimeout(c.Request.Context(), deadline)
	defer cancel()
	c.Request = c.Request.WithContext(requestContext)
	buffer := &egressWriter{ResponseWriter: original, headers: original.Header().Clone(), capacity: int(capacity), cancel: cancel, status: http.StatusOK} //nolint:gosec // A hold never exceeds maxEgressResponse.
	c.Writer = buffer
	defer func() { c.Writer = original }()
	c.Next()
	c.Writer = original
	// A conditional hit carries no body on the wire, whatever the handler wrote.
	if buffer.status == http.StatusNotModified || buffer.status == http.StatusNoContent {
		buffer.body.Reset()
	}
	charge := uint64(buffer.body.Len()) //nolint:gosec // Buffer length is nonnegative and capped at 16 MiB by Write.
	if buffer.failed || requestContext.Err() != nil {
		charge = 0
	}
	included, prepaid, err := hold.settle(c.Request.Context(), charge)
	if err != nil {
		rejectEgress(c, err)
		return
	}
	charged = charge
	if buffer.failed {
		if capacity < maxEgressResponse {
			rejectEgress(c, egressError{status: http.StatusPaymentRequired})
			return
		}
		c.AbortWithStatusJSON(http.StatusRequestEntityTooLarge, gin.H{"error": "response_budget_exceeded", "message": "Request a smaller bounded page; streaming is not supported on this route."})
		return
	}
	if requestContext.Err() != nil {
		c.AbortWithStatusJSON(http.StatusGatewayTimeout, gin.H{"error": "response_deadline_exceeded"})
		return
	}
	for name, values := range buffer.headers {
		original.Header()[name] = values
	}
	if buffer.status != http.StatusNotModified && buffer.status != http.StatusNoContent {
		original.Header().Set("Content-Length", strconv.Itoa(buffer.body.Len()))
	}
	original.Header().Set("X-Egress-Included-Remaining", strconv.FormatUint(included, 10))
	original.Header().Set("X-Egress-Prepaid-Remaining", strconv.FormatUint(prepaid, 10))
	status = buffer.status
	original.WriteHeader(buffer.status)
	if buffer.body.Len() == 0 {
		return
	}
	if _, err := original.Write(buffer.body.Bytes()); err != nil {
		log.Printf("funded response delivery failed: %v", err)
	}
}

// settle returns the remaining allowance for the response headers.
type egressHold struct {
	capacity uint64
	settle   func(base context.Context, charge uint64) (included, prepaid uint64, err error)
}

func (cp *ControlPlane) reserveEgress(ctx context.Context, tenant string) (egressHold, error) {
	if cp.leases != nil {
		hold, err := cp.leases.acquire(ctx, tenant)
		if err != nil {
			return egressHold{}, err
		}
		return egressHold{
			capacity: hold.bytes,
			settle: func(_ context.Context, charge uint64) (uint64, uint64, error) {
				included, prepaid := cp.leases.finish(hold, charge)
				return included, prepaid, nil
			},
		}, nil
	}
	operation := uuid.NewString()
	reserved, err := cp.egressExchange(ctx, tenant, egressCommand{Operation: egressOperationReserve, ID: operation})
	if err != nil {
		return egressHold{}, err
	}
	capacity := reserved.Admission.ReservedBytes
	if capacity == 0 || capacity > maxEgressResponse {
		return egressHold{}, errInvalidEgressReservation
	}
	return egressHold{
		capacity: capacity,
		settle: func(base context.Context, charge uint64) (uint64, uint64, error) {
			settlementContext, stop := context.WithTimeout(context.WithoutCancel(base), 3*time.Second)
			defer stop()
			settled, err := cp.egressExchange(settlementContext, tenant, egressCommand{Operation: egressOperationComplete, ID: operation, Bytes: charge})
			if err != nil {
				return 0, 0, err
			}
			return settled.Admission.Snapshot.IncludedAvailable, settled.Admission.Snapshot.PrepaidAvailable, nil
		},
	}, nil
}

// Ingest writes, view definition writes and the empty event-feed position probe return no tenant data,
// so they must not wait for a response slot or a ledger round trip.
func billableEgressRequest(r *http.Request) bool {
	path := r.URL.Path
	switch {
	case r.Method == http.MethodPost && (path == "/api/v1/events" || path == "/api/v1/events/batch"):
		return false
	case r.Method == http.MethodPut && strings.HasPrefix(path, "/api/v1/read-models/"):
		return false
	case (r.Method == http.MethodPut || r.Method == http.MethodDelete) && strings.HasPrefix(path, viewsPath+"/"):
		return false
	case r.Method == http.MethodGet && path == eventFeedPath && r.URL.Query().Get("limit") == "0":
		return false
	}
	return billableEgressPath(path)
}

func billableEgressPath(path string) bool {
	if path == "/api/v1/cluster/health" {
		return false
	}
	if !strings.HasPrefix(path, "/api/v1/") {
		return false
	}
	if path == tenantLimitsPath {
		return false
	}
	for _, prefix := range []string{"/api/v1/auth/", "/api/v1/billing/", "/api/v1/admin/", "/api/v1/webhooks/"} {
		if strings.HasPrefix(path, prefix) {
			return false
		}
	}
	return true
}

// Global-pool and ledger refusals answer the same in every admission mode.
// A 429 here means a full concurrency pool, whose slots free as soon as an
// in-flight response settles, so it asks for a one-second wait; only
// per-tenant admission refusals carry a computed Retry-After.
func rejectEgress(c *gin.Context, err error) {
	status := http.StatusServiceUnavailable
	reason := egressReasonUnavailable
	retryAfter := "1"
	var refusal egressError
	if errors.As(err, &refusal) && (refusal.status == http.StatusPaymentRequired || refusal.status == http.StatusTooManyRequests) {
		status = refusal.status
		if refusal.reason != "" {
			reason = refusal.reason
		}
		if refusal.reason == egressReasonRateLimited {
			if seconds, parseErr := strconv.Atoi(refusal.retryAfter); parseErr == nil && seconds > 0 {
				retryAfter = strconv.Itoa(seconds)
			}
		}
	}
	body := gin.H{"error": "egress_budget_unavailable", "reason": reason}
	switch status {
	case http.StatusPaymentRequired:
		body["message"] = "Included allowance and prepaid credit must cover this response. Inspect billing usage or purchase prepaid credit."
	case http.StatusTooManyRequests:
		c.Header("Retry-After", retryAfter)
		body["message"] = "Too many responses are in flight; retry shortly."
	default:
		c.Header("Retry-After", "30")
		body["message"] = "The egress ledger is unavailable; retry later."
	}
	log.Printf("egress refused status=%d reason=%s path=%s", status, reason, c.Request.URL.Path) //nolint:gosec // G706: reason is from a fixed set or Core's own denial enum.
	c.AbortWithStatusJSON(status, body)
}

type egressWriter struct {
	gin.ResponseWriter
	headers  http.Header
	body     bytes.Buffer
	capacity int
	status   int
	failed   bool
	cancel   context.CancelFunc
}

func (w *egressWriter) Header() http.Header    { return w.headers }
func (w *egressWriter) Status() int            { return w.status }
func (w *egressWriter) Size() int              { return w.body.Len() }
func (w *egressWriter) Written() bool          { return w.body.Len() > 0 || w.status != http.StatusOK }
func (w *egressWriter) WriteHeader(status int) { w.status = status }
func (w *egressWriter) WriteHeaderNow()        {}
func (w *egressWriter) WriteString(value string) (int, error) {
	if w.failed || len(value) > w.capacity-w.body.Len() {
		w.failed = true
		w.cancel()
		return 0, fmt.Errorf("response exceeds funded byte reservation")
	}
	return w.body.WriteString(value)
}
func (w *egressWriter) Write(value []byte) (int, error) {
	if w.failed || len(value) > w.capacity-w.body.Len() {
		w.failed = true
		w.cancel()
		return 0, fmt.Errorf("response exceeds funded byte reservation")
	}
	return w.body.Write(value)
}
func (w *egressWriter) Flush()              { w.failed = true; w.cancel() }
func (w *egressWriter) Pusher() http.Pusher { return nil }
func (w *egressWriter) Hijack() (net.Conn, *bufio.ReadWriter, error) {
	w.failed = true
	w.cancel()
	return nil, nil, errors.New("unbounded upgraded responses are not supported")
}
