package usecases

import (
	"context"
	"errors"
	"testing"
	"time"

	"github.com/allsource/control-plane/internal/domain"
	"github.com/allsource/control-plane/internal/infrastructure/clients"
	"github.com/allsource/control-plane/internal/infrastructure/persistence"
)

func TestParseAsOf(t *testing.T) {
	cases := []struct {
		name    string
		raw     string
		wantNil bool
		wantErr bool
		want    time.Time
	}{
		{name: "empty means no point in time", raw: "", wantNil: true},
		{name: "whole seconds", raw: "2026-01-02T03:04:05Z", want: time.Date(2026, 1, 2, 3, 4, 5, 0, time.UTC)},
		{name: "nanoseconds kept", raw: "2026-01-02T03:04:05.123456789Z", want: time.Date(2026, 1, 2, 3, 4, 5, 123456789, time.UTC)},
		{name: "offset form", raw: "2026-01-02T05:04:05+02:00", want: time.Date(2026, 1, 2, 3, 4, 5, 0, time.UTC)},
		{name: "prose rejected", raw: "yesterday", wantErr: true},
		{name: "date only rejected", raw: "2026-01-02", wantErr: true},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got, err := ParseAsOf(tc.raw)
			if tc.wantErr {
				if !errors.Is(err, domain.ErrInvalidInput) {
					t.Fatalf("expected ErrInvalidInput, got %v", err)
				}
				if got != nil {
					t.Fatalf("expected nil time on error, got %v", got)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if tc.wantNil {
				if got != nil {
					t.Fatalf("expected nil, got %v", got)
				}
				return
			}
			if !got.Equal(tc.want) {
				t.Fatalf("got %v, want %v", got, tc.want)
			}
		})
	}
}

// replayCore captures StartReplay; other CoreClient methods are not called.
type replayCore struct {
	clients.CoreClient
	got clients.ReplayRequest
}

func (c *replayCore) StartReplay(_ context.Context, req clients.ReplayRequest) (*clients.ReplayResponse, error) {
	c.got = req
	return &clients.ReplayResponse{ReplayID: "r-1", Status: "running"}, nil
}

// The operation record must name the instant Core replays.
func TestStartReplay_OperationRecordMatchesCoreAsOf(t *testing.T) {
	core := &replayCore{}
	uc := NewStartReplayUseCase(
		persistence.NewMemoryOperationRepository(),
		persistence.NewMemoryAuditRepository(),
		core,
	)
	asOf := time.Date(2026, 1, 2, 3, 4, 5, 987654321, time.UTC)

	op, err := uc.Execute(context.Background(), "e-1", &asOf, "t-1", "admin")
	if err != nil {
		t.Fatalf("execute: %v", err)
	}
	if core.got.AsOf == nil || !core.got.AsOf.Equal(asOf) {
		t.Fatalf("core received as_of %v, want %v", core.got.AsOf, asOf)
	}
	recorded, _ := op.Parameters["as_of"].(string) //nolint:errcheck // asserted by the comparison below
	parsed, err := ParseAsOf(recorded)
	if err != nil {
		t.Fatalf("recorded as_of %q does not parse: %v", recorded, err)
	}
	if !parsed.Equal(*core.got.AsOf) {
		t.Fatalf("operation record %q names %v, core replayed %v", recorded, parsed, *core.got.AsOf)
	}
}
