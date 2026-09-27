package usecases

import (
	"context"
	"testing"

	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

type memberCountCore struct {
	clients.CoreClient
	value any
}

func (m memberCountCore) GetConfig(_ context.Context, key string) (*clients.ConfigEntryResponse, error) {
	return &clients.ConfigEntryResponse{Key: key, Value: m.value}, nil
}

func TestMemberCountsSupportAtomicTeamState(t *testing.T) {
	members := []any{map[string]any{"user_id": "one"}, map[string]any{"user_id": "two"}}
	for _, value := range []any{members, map[string]any{"schema_version": 2, "members": members, "invitations": map[string]any{}}} {
		count, ok := memberCountFromCore(t.Context(), memberCountCore{value: value}, "team")
		if !ok || count != 2 {
			t.Fatal("admin member count lost existing team records")
		}
	}
	if _, ok := memberCountFromCore(t.Context(), memberCountCore{value: map[string]any{"schema_version": 3, "members": members}}, "team"); ok {
		t.Fatal("accepted unknown team state")
	}
}
