package allsource

import (
	"errors"
	"fmt"
	"net/http"
)

// ErrNotModified matches a *NotModifiedError with errors.Is.
var ErrNotModified = errors.New("allsource: not modified")

// NotModifiedError is returned when a conditional read sent IfNoneMatch and
// the server answered 304. The cached value the caller holds is current.
type NotModifiedError struct {
	ETag string
}

func (e *NotModifiedError) Error() string {
	return fmt.Sprintf("allsource: not modified (etag %s)", e.ETag)
}

// Is reports whether target is ErrNotModified.
func (e *NotModifiedError) Is(target error) bool { return target == ErrNotModified }

// EpochChangedError is a 409 from the event feed: the feed generation changed
// (Core restart or eviction). Rebuild from position 0 of CurrentEpoch and
// commit with Reset set.
type EpochChangedError struct {
	ExpectedEpoch int64
	CurrentEpoch  int64
	API           *APIError
}

func (e *EpochChangedError) Error() string {
	return fmt.Sprintf("allsource: event feed epoch changed from %d to %d", e.ExpectedEpoch, e.CurrentEpoch)
}

func (e *EpochChangedError) Unwrap() error { return e.API }

// VersionConflictError is a 409 from a read-model commit whose
// ExpectedRevision is stale. The commit was not applied.
type VersionConflictError struct {
	ExpectedRevision int64
	CurrentRevision  int64
	API              *APIError
}

func (e *VersionConflictError) Error() string {
	return fmt.Sprintf("allsource: read model at revision %d, commit expected %d", e.CurrentRevision, e.ExpectedRevision)
}

func (e *VersionConflictError) Unwrap() error { return e.API }

// ViewResetError is a 409 from view changes: the cursor predates the view's
// last reset, so the caller must re-read the entity pages.
type ViewResetError struct {
	View       string
	Generation int64
	API        *APIError
}

func (e *ViewResetError) Error() string {
	return fmt.Sprintf("allsource: view %s was reset (generation %d); re-read its entities", e.View, e.Generation)
}

func (e *ViewResetError) Unwrap() error { return e.API }

// InvalidViewError is a 400 for a view definition Core rejected.
type InvalidViewError struct {
	Detail string
	API    *APIError
}

func (e *InvalidViewError) Error() string {
	return "allsource: invalid view definition: " + e.Detail
}

func (e *InvalidViewError) Unwrap() error { return e.API }

// ViewRowNotFoundError is a 404 for one view row. Ready is false while the
// view is still being folded, so a missing row may yet appear.
type ViewRowNotFoundError struct {
	View  string
	Key   string
	Ready bool
	API   *APIError
}

func (e *ViewRowNotFoundError) Error() string {
	return fmt.Sprintf("allsource: view %s has no row %q (ready %t)", e.View, e.Key, e.Ready)
}

func (e *ViewRowNotFoundError) Unwrap() error { return e.API }

func apiErrorWith(err error, status int, code string) (*APIError, bool) {
	var apiErr *APIError
	if !errors.As(err, &apiErr) || apiErr.StatusCode != status {
		return nil, false
	}
	if code != "" && apiErr.Code != code {
		return nil, false
	}
	return apiErr, true
}

func versionConflictFields(apiErr *APIError) (expected, current int64) {
	if apiErr.ExpectedVersion != nil {
		expected = *apiErr.ExpectedVersion
	}
	if apiErr.CurrentVersion != nil {
		current = *apiErr.CurrentVersion
	}
	return expected, current
}

func isThrottled(apiErr *APIError) bool {
	return apiErr.StatusCode == http.StatusTooManyRequests || apiErr.StatusCode == http.StatusServiceUnavailable
}
