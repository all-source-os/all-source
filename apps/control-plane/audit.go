// Package main provides the control plane for the AllSource event sourcing system.
package main

import (
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
)

// Audit constants
const (
	resourceTenant    = "tenant"
	resourceUser      = "user"
	resourceUnknown   = "unknown"
	actionLogin       = "login"
	actionCreate      = "create"
	resourceOperation = "operation"
	pathHealth        = "/health"
	pathLivez         = "/livez"
	pathMetrics       = "/metrics"
)

// Async audit logging constants
const (
	// AuditChannelCapacity is the buffered channel capacity for async audit events.
	AuditChannelCapacity = 10000
	// AuditSendTimeout is the timeout for sending events when channel is near full.
	AuditSendTimeout = 100 * time.Millisecond
)

// AuditEvent represents an auditable action
type AuditEvent struct {
	Timestamp  time.Time              `json:"timestamp"`
	EventType  string                 `json:"event_type"`
	UserID     string                 `json:"user_id,omitempty"`
	Username   string                 `json:"username,omitempty"`
	TenantID   string                 `json:"tenant_id,omitempty"`
	Action     string                 `json:"action"`
	Resource   string                 `json:"resource,omitempty"`
	ResourceID string                 `json:"resource_id,omitempty"`
	Method     string                 `json:"method"`
	Path       string                 `json:"path"`
	StatusCode int                    `json:"status_code"`
	Duration   float64                `json:"duration_ms"`
	IPAddress  string                 `json:"ip_address,omitempty"`
	UserAgent  string                 `json:"user_agent,omitempty"`
	Error      string                 `json:"error,omitempty"`
	Metadata   map[string]interface{} `json:"metadata,omitempty"`
}

// AuditMetrics holds Prometheus metrics for audit logging
type AuditMetrics struct {
	EventsQueued  prometheus.Counter
	EventsWritten prometheus.Counter
	EventsDropped prometheus.Counter
	QueueSize     prometheus.Gauge
	WriteLatency  prometheus.Histogram
}

// Singleton for audit metrics to avoid duplicate registration in tests
var (
	auditMetricsOnce      sync.Once
	auditMetricsSingleton *AuditMetrics
)

// NewAuditMetrics creates or returns the singleton audit metrics
func NewAuditMetrics() *AuditMetrics {
	auditMetricsOnce.Do(func() {
		auditMetricsSingleton = &AuditMetrics{
			EventsQueued: promauto.NewCounter(prometheus.CounterOpts{
				Name: "control_plane_audit_events_queued_total",
				Help: "Total number of audit events queued for async processing",
			}),
			EventsWritten: promauto.NewCounter(prometheus.CounterOpts{
				Name: "control_plane_audit_events_written_total",
				Help: "Total number of audit events successfully written",
			}),
			EventsDropped: promauto.NewCounter(prometheus.CounterOpts{
				Name: "control_plane_audit_events_dropped_total",
				Help: "Total number of audit events dropped due to full queue",
			}),
			QueueSize: promauto.NewGauge(prometheus.GaugeOpts{
				Name: "control_plane_audit_queue_size",
				Help: "Current number of audit events in the queue",
			}),
			WriteLatency: promauto.NewHistogram(prometheus.HistogramOpts{
				Name:    "control_plane_audit_write_latency_seconds",
				Help:    "Latency of audit event writes",
				Buckets: []float64{0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1},
			}),
		}
	})
	return auditMetricsSingleton
}

// AuditLogger handles audit logging with async support
type AuditLogger struct {
	file    *os.File
	mu      sync.Mutex
	enabled bool
	// Async support
	asyncEnabled bool
	auditChan    chan AuditEvent
	done         chan struct{}
	wg           sync.WaitGroup
	metrics      *AuditMetrics
	// closeMu protects the close operation to allow multiple calls
	closeMu sync.Mutex
	closed  bool
}

// NewAuditLogger creates a new synchronous audit logger (for backward compatibility)
func NewAuditLogger(filePath string) (*AuditLogger, error) {
	if filePath == "" {
		// Audit logging disabled
		return &AuditLogger{enabled: false}, nil
	}

	file, err := os.OpenFile(filePath, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o600) //nolint:gosec // file path is from trusted config
	if err != nil {
		return nil, fmt.Errorf("failed to open audit log file: %w", err)
	}

	return &AuditLogger{
		file:    file,
		enabled: true,
	}, nil
}

// NewAsyncAuditLogger creates a new async audit logger with buffered channel
func NewAsyncAuditLogger(filePath string) (*AuditLogger, error) {
	if filePath == "" {
		// Audit logging disabled
		return &AuditLogger{enabled: false, asyncEnabled: false}, nil
	}

	file, err := os.OpenFile(filePath, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o600) //nolint:gosec // file path is from trusted config
	if err != nil {
		return nil, fmt.Errorf("failed to open audit log file: %w", err)
	}

	logger := &AuditLogger{
		file:         file,
		enabled:      true,
		asyncEnabled: true,
		auditChan:    make(chan AuditEvent, AuditChannelCapacity),
		done:         make(chan struct{}),
		metrics:      NewAuditMetrics(),
	}

	// Start background goroutine to consume from channel
	logger.wg.Add(1)
	go logger.processAuditEvents()

	return logger, nil
}

// processAuditEvents is the background goroutine that consumes from the audit channel
func (a *AuditLogger) processAuditEvents() {
	defer a.wg.Done()

	for {
		select {
		case event, ok := <-a.auditChan:
			if !ok {
				// Channel closed, drain any remaining events
				return
			}
			a.metrics.QueueSize.Dec()
			a.writeEvent(event)
		case <-a.done:
			// Drain remaining events before shutting down
			for {
				select {
				case event, ok := <-a.auditChan:
					if !ok {
						return
					}
					a.metrics.QueueSize.Dec()
					a.writeEvent(event)
				default:
					return
				}
			}
		}
	}
}

// writeEvent performs the actual synchronous write of an audit event
func (a *AuditLogger) writeEvent(event AuditEvent) {
	start := time.Now()

	a.mu.Lock()
	defer a.mu.Unlock()

	data, err := json.Marshal(event)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to marshal audit event: %v\n", err)
		return
	}

	if _, err := a.file.Write(append(data, '\n')); err != nil {
		fmt.Fprintf(os.Stderr, "Failed to write audit event: %v\n", err)
		return
	}

	a.metrics.EventsWritten.Inc()
	a.metrics.WriteLatency.Observe(time.Since(start).Seconds())
}

// Log writes an audit event to the log file (async if enabled, sync otherwise)
func (a *AuditLogger) Log(event AuditEvent) error {
	if !a.enabled {
		return nil
	}

	event.Timestamp = time.Now().UTC()

	// Use async logging if enabled
	if a.asyncEnabled {
		return a.logAsync(event)
	}

	// Synchronous fallback
	return a.logSync(event)
}

// logSync writes an audit event synchronously (original behavior)
func (a *AuditLogger) logSync(event AuditEvent) error {
	a.mu.Lock()
	defer a.mu.Unlock()

	data, err := json.Marshal(event)
	if err != nil {
		return fmt.Errorf("failed to marshal audit event: %w", err)
	}

	if _, err := a.file.Write(append(data, '\n')); err != nil {
		return fmt.Errorf("failed to write audit event: %w", err)
	}

	return nil
}

// logAsync sends an audit event to the buffered channel for async processing
// Handles channel full gracefully with timeout-based backpressure
func (a *AuditLogger) logAsync(event AuditEvent) error {
	// Try non-blocking send first (fast path)
	select {
	case a.auditChan <- event:
		a.metrics.EventsQueued.Inc()
		a.metrics.QueueSize.Inc()
		return nil
	default:
		// Channel full, try with timeout
	}

	// Slow path: wait with timeout
	timer := time.NewTimer(AuditSendTimeout)
	defer timer.Stop()

	select {
	case a.auditChan <- event:
		a.metrics.EventsQueued.Inc()
		a.metrics.QueueSize.Inc()
		return nil
	case <-timer.C:
		// Channel still full after timeout, drop the event
		a.metrics.EventsDropped.Inc()
		return fmt.Errorf("audit channel full, event dropped")
	}
}

// Close closes the audit logger, draining any pending events first
// Safe to call multiple times
func (a *AuditLogger) Close() error {
	a.closeMu.Lock()
	defer a.closeMu.Unlock()

	// Already closed, nothing to do
	if a.closed {
		return nil
	}
	a.closed = true

	if a.asyncEnabled && a.done != nil {
		// Signal shutdown to the background goroutine
		close(a.done)
		// Wait for goroutine to finish draining events
		a.wg.Wait()
		// Close the channel
		close(a.auditChan)
	}

	a.mu.Lock()
	defer a.mu.Unlock()

	if a.file != nil {
		err := a.file.Close()
		a.file = nil
		a.enabled = false
		return err
	}
	return nil
}

// AuditMiddleware logs all requests for audit purposes
func AuditMiddleware(logger *AuditLogger) gin.HandlerFunc {
	return func(c *gin.Context) {
		// Skip health and metrics endpoints
		if c.Request.URL.Path == pathHealth || c.Request.URL.Path == pathLivez || c.Request.URL.Path == pathMetrics {
			c.Next()
			return
		}

		start := time.Now()

		// Process request
		c.Next()

		// Calculate duration
		duration := time.Since(start).Milliseconds()

		// Extract auth context if available
		var userID, username, tenantID string
		if auth, exists := c.Get("auth"); exists {
			if authCtx, ok := auth.(*AuthContext); ok {
				userID = authCtx.UserID
				username = authCtx.Username
				tenantID = authCtx.TenantID
			}
		}

		// Determine action from method and path
		action := determineAction(c.Request.Method, c.Request.URL.Path)

		// Create audit event
		event := AuditEvent{
			EventType:  "api_request",
			UserID:     userID,
			Username:   username,
			TenantID:   tenantID,
			Action:     action,
			Resource:   extractResource(c.Request.URL.Path),
			Method:     c.Request.Method,
			Path:       c.Request.URL.Path,
			StatusCode: c.Writer.Status(),
			Duration:   float64(duration),
			IPAddress:  c.ClientIP(),
			UserAgent:  c.Request.UserAgent(),
		}

		// Add error if request failed
		if c.Writer.Status() >= 400 {
			if errors := c.Errors.ByType(gin.ErrorTypeAny); len(errors) > 0 {
				event.Error = errors.String()
			}
		}

		// Log the event
		if err := logger.Log(event); err != nil {
			// Log to stderr if audit logging fails (don't fail the request)
			fmt.Fprintf(os.Stderr, "Failed to write audit log: %v\n", err)
		}
	}
}

// determineAction extracts a human-readable action from method and path
func determineAction(method, path string) string {
	switch method {
	case "GET":
		return "read"
	case http.MethodPost:
		if contains(path, actionLogin) {
			return actionLogin
		}
		if contains(path, "register") {
			return "register"
		}
		return actionCreate
	case "PUT":
		return "update"
	case "DELETE":
		return "delete"
	case "PATCH":
		return "modify"
	default:
		return method
	}
}

// extractResource extracts the resource type from the path
func extractResource(path string) string {
	if contains(path, "/tenants") {
		return resourceTenant
	}
	if contains(path, "/users") {
		return resourceUser
	}
	if contains(path, "/snapshots") {
		return "snapshot"
	}
	if contains(path, "/backups") {
		return "backup"
	}
	if contains(path, "/cluster") {
		return "cluster"
	}
	if contains(path, "/operations") {
		return resourceOperation
	}
	return resourceUnknown
}

// contains checks if a string contains a substring
func contains(s, substr string) bool {
	return len(s) >= len(substr) && (s == substr || indexOf(s, substr) >= 0)
}

// indexOf returns the index of substr in s, or -1 if not found
func indexOf(s, substr string) int {
	for i := 0; i <= len(s)-len(substr); i++ {
		if s[i:i+len(substr)] == substr {
			return i
		}
	}
	return -1
}

// LogAuthEvent logs an authentication-related event
func (a *AuditLogger) LogAuthEvent(eventType, userID, username, tenantID, details string) {
	event := AuditEvent{
		EventType: eventType,
		UserID:    userID,
		Username:  username,
		TenantID:  tenantID,
		Action:    eventType,
		Metadata: map[string]interface{}{
			"details": details,
		},
	}
	_ = a.Log(event) //nolint:errcheck // audit logging should not fail the operation
}

// LogTenantEvent logs a tenant management event
func (a *AuditLogger) LogTenantEvent(action, tenantID, userID, details string) {
	event := AuditEvent{
		EventType:  "tenant_management",
		Action:     action,
		Resource:   resourceTenant,
		ResourceID: tenantID,
		UserID:     userID,
		Metadata: map[string]interface{}{
			"details": details,
		},
	}
	_ = a.Log(event) //nolint:errcheck // audit logging should not fail the operation
}

// LogOperationEvent logs an operational event (snapshot, backup, etc.)
func (a *AuditLogger) LogOperationEvent(operation, resourceID, userID, status string) {
	event := AuditEvent{
		EventType:  "operation",
		Action:     operation,
		Resource:   operation,
		ResourceID: resourceID,
		UserID:     userID,
		Metadata: map[string]interface{}{
			"status": status,
		},
	}
	_ = a.Log(event) //nolint:errcheck // audit logging should not fail the operation
}
