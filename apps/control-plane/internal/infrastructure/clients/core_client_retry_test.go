package clients

import (
	"context"
	"errors"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"

	"github.com/go-resty/resty/v2"
)

func TestCoreRetryConditionResendsWritesOnlyWhenCoreWasNotReached(t *testing.T) {
	refused := &net.OpError{Op: "dial", Net: "tcp", Err: errors.New("connection refused")}
	reset := &net.OpError{Op: "read", Net: "tcp", Err: errors.New("connection reset by peer")}
	response := func(method string, status int) *resty.Response {
		return &resty.Response{Request: &resty.Request{Method: method}, RawResponse: &http.Response{StatusCode: status}}
	}
	cases := []struct {
		name   string
		resp   *resty.Response
		err    error
		resend bool
	}{
		{"post refused dial", response(http.MethodPost, 0), refused, true},
		{"post reset", response(http.MethodPost, 0), reset, false},
		{"post eof", response(http.MethodPost, 0), io.ErrUnexpectedEOF, false},
		{"post 500", response(http.MethodPost, http.StatusInternalServerError), nil, false},
		{"put 503", response(http.MethodPut, http.StatusServiceUnavailable), nil, false},
		{"delete 502", response(http.MethodDelete, http.StatusBadGateway), nil, false},
		{"get 500", response(http.MethodGet, http.StatusInternalServerError), nil, true},
		{"get reset", response(http.MethodGet, 0), reset, true},
		{"get 404", response(http.MethodGet, http.StatusNotFound), nil, false},
		{"nil response refused", nil, refused, true},
	}
	for _, tc := range cases {
		if got := coreRetryCondition(tc.resp, tc.err); got != tc.resend {
			t.Errorf("%s: resend %v, want %v", tc.name, got, tc.resend)
		}
	}
}

func TestCoreClientSendsFailedIngestOnce(t *testing.T) {
	var seen atomic.Int32
	core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		seen.Add(1)
		w.WriteHeader(http.StatusInternalServerError)
	}))
	t.Cleanup(core.Close)
	client := NewCoreClientWithURL(core.URL, "")

	if _, err := client.IngestEvent(context.Background(), IngestEventRequest{EventType: "order.created", EntityID: "o-1"}); err == nil {
		t.Fatal("ingest against a failing Core succeeded")
	}
	if got := seen.Load(); got != 1 {
		t.Fatalf("Core received the write %d times, want 1", got)
	}
}
