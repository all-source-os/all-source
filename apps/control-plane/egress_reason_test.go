package main

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/go-resty/resty/v2"

	"github.com/allsource/control-plane/internal/domain/entities"
)

func TestExchangeEgressCarriesCoresDenialReason(t *testing.T) {
	core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.Header().Set("Retry-After", "30")
		w.WriteHeader(http.StatusTooManyRequests)
		if _, err := w.Write([]byte(`{"error":"egress_admission_denied","reason":"busy"}`)); err != nil {
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

	_, err = cp.exchangeEgress(t.Context(), "acme", egressCommand{Operation: "reserve", ID: "op-1"})
	var refusal egressError
	if !errors.As(err, &refusal) || refusal.status != http.StatusTooManyRequests || refusal.reason != "busy" {
		t.Fatalf("got %#v", err)
	}
}
