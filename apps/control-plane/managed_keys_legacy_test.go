package main

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestManagedKeyLegacyHistory(t *testing.T) {
	for _, scenario := range []string{"active", "active-legacy", "active-ten-events", "revoked", "missing", "missing-legacy", "reordered", "legacy-after-numbered", "duplicate-id", "gap", "cross-tenant", "cross-entity"} {
		t.Run(scenario, func(t *testing.T) {
			first := lifecycleCreated()
			lifecycleTestRecord(t, first)["id"] = "older-key"
			second := map[string]any{"id": "older-revoked", "entity_id": "apikey:proof-tenant", "tenant_id": "proof-tenant", "event_type": "apikey.revoked", "version": 1, "timestamp": "2026-01-02T00:00:00Z", "payload": map[string]any{"key_id": "older-key"}}
			active := lifecycleCreated()
			active["id"], active["version"], active["timestamp"] = "current", 3, "2026-01-03T00:00:00Z"
			events := []map[string]any{first, second, active}
			switch scenario {
			case "active-legacy":
				active["version"] = 1
				second["timestamp"], active["timestamp"] = first["timestamp"], first["timestamp"]
			case "active-ten-events":
				for ordinal := 4; ordinal <= 10; ordinal++ {
					olderEvent := lifecycleCreated()
					lifecycleTestRecord(t, olderEvent)["id"] = "older-key"
					olderEvent["id"] = fmt.Sprintf("older-%d", ordinal)
					olderEvent["timestamp"] = "2026-01-02T00:00:00Z"
					events = append(events[:len(events)-1], olderEvent, active)
				}
				active["version"] = 10
			case "revoked":
				active["version"] = 1
				events = append(events, map[string]any{"id": "revoked", "entity_id": "apikey:proof-tenant", "tenant_id": "proof-tenant", "event_type": "apikey.revoked", "version": 1, "timestamp": "2026-01-04T00:00:00Z", "payload": map[string]any{"key_id": "proof-key"}})
			case "missing":
				events = []map[string]any{first, active}
			case "missing-legacy":
				events = []map[string]any{first, second}
			case "reordered":
				events[0], events[1] = events[1], events[0]
			case "legacy-after-numbered":
				second["version"], active["version"] = 2, 1
			case "duplicate-id":
				active["id"] = first["id"]
			case "gap":
				active["version"] = 4
			case "cross-tenant":
				active["tenant_id"] = "other-tenant"
			case "cross-entity":
				active["entity_id"] = "apikey:other-tenant"
			}
			page := lifecyclePage(events...)
			if scenario == "active-legacy" || scenario == "revoked" {
				page["entity_version"] = 1
			}
			if strings.HasPrefix(scenario, "missing") {
				page["entity_version"] = 3
			}
			core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
				if err := json.NewEncoder(w).Encode(page); err != nil {
					t.Error(err)
				}
			}))
			defer core.Close()
			response := lifecycleRequest(t, NewAuthClient("synthetic-secret", core.URL), lifecycleClaims(), "GET", queryEventsPath, "")
			want := http.StatusUnauthorized
			if strings.HasPrefix(scenario, "active") {
				want = http.StatusOK
			}
			if response.Code != want {
				t.Fatalf("got %d, want %d: %s", response.Code, want, response.Body)
			}
		})
	}
}
