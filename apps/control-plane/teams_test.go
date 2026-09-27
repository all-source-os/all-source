package main

import (
	"testing"
	"time"
)

func TestDecodeTeamStateRejectsAmbiguousAuthority(t *testing.T) {
	for _, raw := range []string{
		`null`, `{}`, `{"schema_version":2,"members":[]}`,
		`{"schema_version":3,"members":[],"invitations":{}}`,
		`[{"user_id":"u1","role":"admin"},{"user_id":"u1","role":"member"}]`,
		`[{"user_id":"u1","role":"owner"}]`, `[{"user_id":"../u1","role":"admin"}]`,
	} {
		if _, err := decodeTeamState([]byte(raw)); err == nil {
			t.Errorf("accepted malformed team state: %s", raw)
		}
	}
	for _, raw := range []string{`[]`, `[{"user_id":"u1","role":"member"}]`, `{"schema_version":2,"members":[],"invitations":{}}`} {
		if _, err := decodeTeamState([]byte(raw)); err != nil {
			t.Errorf("rejected supported team format: %v", err)
		}
	}
}

func TestTeamInvitationLifetime(t *testing.T) {
	now := time.Now().UTC()
	invite := teamInvitation{Email: "member@example.test", Role: roleMember,
		CreatedAt: now.Add(-time.Hour).Format(time.RFC3339Nano), ExpiresAt: now.Add(time.Hour).Format(time.RFC3339Nano)}
	if !validTeamInvitation(invite, now) {
		t.Fatal("rejected live invitation")
	}
	for _, expiry := range []string{now.Format(time.RFC3339Nano), now.Add(-2 * time.Hour).Format(time.RFC3339Nano), now.Add(8 * 24 * time.Hour).Format(time.RFC3339Nano), "invalid"} {
		invite.ExpiresAt = expiry
		if validTeamInvitation(invite, now) {
			t.Fatal("accepted expired or invalid invitation")
		}
	}
}
