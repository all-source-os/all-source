package main

import (
	"bufio"
	"bytes"
	"context"
	"errors"
	"fmt"
	"log"
	"net"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/google/uuid"
)

const maxEgressResponse = 16 * 1024 * 1024

var egressResponseSlots = make(chan struct{}, 16)

type egressCommand struct {
	Operation string `json:"operation"`
	ID        string `json:"id,omitempty"`
	Bytes     uint64 `json:"bytes,omitempty"`
}

type egressAdmission struct {
	Protocol  string `json:"protocol"`
	Admission struct {
		ReservedBytes uint64 `json:"reserved_bytes"`
		Snapshot      struct {
			IncludedAvailable uint64 `json:"included_available"`
			PrepaidAvailable  uint64 `json:"prepaid_available"`
		} `json:"snapshot"`
	} `json:"admission"`
}

type egressError struct{ status int }

func (e egressError) Error() string { return "egress admission denied" }

type egressExchange func(context.Context, string, egressCommand) (egressAdmission, error)

func (cp *ControlPlane) exchangeEgress(ctx context.Context, tenant string, command egressCommand) (egressAdmission, error) {
	var result egressAdmission
	deadline, cancel := context.WithTimeout(ctx, 3*time.Second)
	defer cancel()
	target := *cp.delegation.coreForTenant(tenant)
	target.Path = strings.TrimRight(target.Path, "/") + "/api/v1/tenants/" + url.PathEscape(tenant) + "/egress"
	payload := map[string]any{"operation": command.Operation, "id": command.ID}
	if command.Operation == "complete" {
		payload["bytes"] = command.Bytes
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
			return result, egressError{status: response.StatusCode()}
		}
		if result.Protocol != "funded-egress-v1" {
			return result, errors.New("unsupported egress ledger")
		}
		return result, nil
	}
	return result, errors.New("egress ledger unavailable")
}

func (cp *ControlPlane) enforceEgress(c *gin.Context) {
	if cp.egressExchange == nil || !billableEgressRequest(c.Request) {
		c.Next()
		return
	}
	tenant, ok := authTenantFromContext(c)
	if !ok {
		c.AbortWithStatusJSON(http.StatusUnauthorized, gin.H{"error": "authentication_required"})
		return
	}
	select {
	case egressResponseSlots <- struct{}{}:
		defer func() { <-egressResponseSlots }()
	default:
		rejectEgress(c, egressError{status: http.StatusTooManyRequests})
		return
	}
	operation := uuid.NewString()
	reserved, err := cp.egressExchange(c.Request.Context(), tenant, egressCommand{Operation: "reserve", ID: operation})
	if err != nil {
		rejectEgress(c, err)
		return
	}
	capacity := reserved.Admission.ReservedBytes
	if capacity == 0 || capacity > maxEgressResponse {
		c.AbortWithStatusJSON(http.StatusServiceUnavailable, gin.H{"error": "invalid_egress_reservation"})
		return
	}
	original := c.Writer
	requestContext, cancel := context.WithTimeout(c.Request.Context(), 10*time.Second)
	defer cancel()
	c.Request = c.Request.WithContext(requestContext)
	buffer := &egressWriter{ResponseWriter: original, headers: original.Header().Clone(), capacity: int(capacity), cancel: cancel, status: http.StatusOK}
	c.Writer = buffer
	defer func() { c.Writer = original }()
	c.Next()
	c.Writer = original
	charge := uint64(buffer.body.Len()) //nolint:gosec // Buffer length is nonnegative and capped at 16 MiB by Write.
	if buffer.failed || requestContext.Err() != nil {
		charge = 0
	}
	settlementContext, stop := context.WithTimeout(context.WithoutCancel(c.Request.Context()), 3*time.Second)
	defer stop()
	settled, err := cp.egressExchange(settlementContext, tenant, egressCommand{Operation: "complete", ID: operation, Bytes: charge})
	if err != nil {
		rejectEgress(c, err)
		return
	}
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
	original.Header().Set("Content-Length", strconv.Itoa(buffer.body.Len()))
	original.Header().Set("X-Egress-Included-Remaining", strconv.FormatUint(settled.Admission.Snapshot.IncludedAvailable, 10))
	original.Header().Set("X-Egress-Prepaid-Remaining", strconv.FormatUint(settled.Admission.Snapshot.PrepaidAvailable, 10))
	original.WriteHeader(buffer.status)
	if _, err := original.Write(buffer.body.Bytes()); err != nil {
		log.Printf("funded response delivery failed: %v", err)
	}
}

// Ingest writes and the empty event-feed position probe return no tenant data,
// so they must not wait for a response slot or a ledger round trip.
func billableEgressRequest(r *http.Request) bool {
	path := r.URL.Path
	switch {
	case r.Method == http.MethodPost && (path == "/api/v1/events" || path == "/api/v1/events/batch"):
		return false
	case r.Method == http.MethodPut && strings.HasPrefix(path, "/api/v1/read-models/"):
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
	for _, prefix := range []string{"/api/v1/auth/", "/api/v1/billing/", "/api/v1/admin/", "/api/v1/webhooks/"} {
		if strings.HasPrefix(path, prefix) {
			return false
		}
	}
	return true
}

func rejectEgress(c *gin.Context, err error) {
	status := http.StatusServiceUnavailable
	var refusal egressError
	if errors.As(err, &refusal) && (refusal.status == http.StatusPaymentRequired || refusal.status == http.StatusTooManyRequests) {
		status = refusal.status
	}
	if status != http.StatusPaymentRequired {
		c.Header("Retry-After", "30")
	}
	c.AbortWithStatusJSON(status, gin.H{"error": "egress_budget_unavailable", "message": "Included allowance and prepaid credit must cover this response. Inspect billing usage or purchase prepaid credit."})
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
