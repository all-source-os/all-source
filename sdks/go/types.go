package allsource

import (
	"fmt"
	"time"
)

// Event represents an event in the AllSource event store.
type Event struct {
	ID        string         `json:"id"`
	EntityID  string         `json:"entity_id"`
	EventType string         `json:"event_type"`
	Payload   map[string]any `json:"payload"`
	Metadata  map[string]any `json:"metadata"`
	TenantID  string         `json:"tenant_id,omitempty"`
	Timestamp string         `json:"timestamp"`
	Version   int            `json:"version"`
}

// EventList is a list of events with a count.
type EventList struct {
	Events     []Event `json:"events"`
	Count      int     `json:"count"`
	TotalCount int     `json:"total_count,omitempty"`
	HasMore    bool    `json:"has_more,omitempty"`
	// EntityVersion is the tenant's event count for the entity, sent by Core
	// only when the query filters by entity_id.
	EntityVersion *int64 `json:"entity_version,omitempty"`
	ETag          string `json:"-"`
}

// APIError represents an error returned by the AllSource API.
type APIError struct {
	Code       string `json:"code"`
	Message    string `json:"message"`
	StatusCode int    `json:"status_code"`
	// Detail, ExpectedVersion, CurrentVersion, Generation and Ready carry
	// Core's structured error fields when the body has them.
	Detail          string `json:"detail,omitempty"`
	ExpectedVersion *int64 `json:"expected_version,omitempty"`
	CurrentVersion  *int64 `json:"current_version,omitempty"`
	Generation      *int64 `json:"generation,omitempty"`
	Ready           *bool  `json:"ready,omitempty"`
	// RetryAfter is the parsed Retry-After header, zero when absent.
	RetryAfter time.Duration `json:"-"`
}

func (e *APIError) Error() string {
	return fmt.Sprintf("allsource: %s (status %d): %s", e.Code, e.StatusCode, e.Message)
}

// IsUnauthorized returns true if the error is a 401 Unauthorized.
func (e *APIError) IsUnauthorized() bool { return e.StatusCode == 401 }

// IsForbidden returns true if the error is a 403 Forbidden.
func (e *APIError) IsForbidden() bool { return e.StatusCode == 403 }

// IsNotFound returns true if the error is a 404 Not Found.
func (e *APIError) IsNotFound() bool { return e.StatusCode == 404 }

// IsRateLimited returns true if the error is a 429 Too Many Requests.
func (e *APIError) IsRateLimited() bool { return e.StatusCode == 429 }

// IsServerError returns true if the error is a 5xx server error.
func (e *APIError) IsServerError() bool { return e.StatusCode >= 500 }

// IsRetryable returns true if the error has a status code that is safe to retry.
func (e *APIError) IsRetryable() bool {
	switch e.StatusCode {
	case 408, 429, 500, 502, 503, 504:
		return true
	default:
		return false
	}
}
