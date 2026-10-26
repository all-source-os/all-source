package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"log"
	"net/http"
	"net/url"
	"reflect"
	"strings"
	"time"

	"github.com/dgrijalva/jwt-go"
	"github.com/gin-gonic/gin"
	"github.com/google/uuid"

	"github.com/allsource/control-plane/internal/domain/entities"
)

const managedKeyKind = "managed_api_key"
const delegationKind = "service_delegation"
const delegationClockSkew = 5
const managedScopeRead = "events:read"
const managedScopeWrite = "events:write"
const managedSchemaRead = "schemas:read"
const managedSchemaWrite = "schemas:write"
const managedProjectionWrite = "projections:write"
const ingestEventsPath = "/api/v1/events"
const ingestBatchPath = "/api/v1/events/batch"
const statsAPIPath = "/api/v1/stats"

func delegationShape(claims *Claims) bool {
	return claims.IsAPIKey && (claims.CredentialKind == delegationKind || claims.CredentialKind == "") &&
		claims.Name == "" && claims.Username == "" && claims.ExpiresAt > claims.IssuedAt && claims.ExpiresAt-claims.IssuedAt <= 60
}

// Valid permits bounded peer clock skew only for short service delegation.
func (claims Claims) Valid() error {
	standard := claims.StandardClaims
	now := jwt.TimeFunc().Unix()
	if delegationShape(&claims) && standard.IssuedAt > now && standard.IssuedAt <= now+delegationClockSkew {
		standard.IssuedAt = now
	}
	return standard.Valid()
}

func (claims *Claims) UnmarshalJSON(data []byte) error {
	type wireClaims Claims
	var wire wireClaims
	if err := json.Unmarshal(data, &wire); err != nil {
		return err
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(data, &fields); err != nil {
		return err
	}
	*claims = Claims(wire)
	_, claims.expPresent = fields["exp"]
	return nil
}

func (a *AuthClient) IssueManagedAPIKey(ctx context.Context, tenant, name string, role entities.Role) (string, error) {
	now := time.Now().UTC().Truncate(time.Second)
	expires := now.Add(365 * 24 * time.Hour)
	id := uuid.NewString()
	scopes := []string{managedScopeRead, managedScopeWrite, "queries:execute", "projections:read"}
	claims := &Claims{UserID: id, TenantID: tenant, Name: name, Role: role, IsAPIKey: true, CredentialKind: managedKeyKind,
		StandardClaims: jwt.StandardClaims{IssuedAt: now.Unix(), ExpiresAt: expires.Unix(), Issuer: "allsource"}}
	secret, err := jwt.NewWithClaims(jwt.SigningMethodHS256, claims).SignedString([]byte(a.jwtSecret))
	if err != nil {
		return "", err
	}
	base := a.coreURL
	if a.lifecycleCoreURL != nil {
		base = a.lifecycleCoreURL(tenant)
	}
	if base == "" {
		return "", errors.New("API key authority unavailable")
	}
	body, err := json.Marshal(map[string]any{"tenant_id": tenant, "entity_id": "apikey:" + tenant, "event_type": "apikey.created",
		"payload": map[string]any{"key": map[string]any{"id": id, "tenant_id": tenant, "name": name, "active": true,
			"scopes": scopes, "expires_at": expires.Format(time.RFC3339), "created_at": now.Format(time.RFC3339), "created_by": "onboarding"}}})
	if err != nil {
		return "", err
	}
	ctx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, strings.TrimRight(base, "/")+ingestEventsPath, bytes.NewReader(body))
	if err != nil {
		return "", err
	}
	bearer, err := a.SignDelegationJWT("control-plane", tenant, entities.RoleAdmin)
	if err != nil {
		return "", err
	}
	req.Header.Set("Authorization", "Bearer "+bearer)
	req.Header.Set("Content-Type", "application/json")
	response, err := a.http.Do(req)
	if err != nil {
		return "", errors.New("API key registration failed")
	}
	defer closeResponseBody(response)
	var ack struct {
		ID string `json:"event_id"`
	}
	if (response.StatusCode != http.StatusOK && response.StatusCode != http.StatusCreated) || json.NewDecoder(io.LimitReader(response.Body, 65536)).Decode(&ack) != nil || ack.ID == "" {
		return "", errors.New("API key registration failed")
	}
	return secret, nil
}

type managedKeyRecord struct {
	ID        string   `json:"id"`
	TenantID  string   `json:"tenant_id"`
	Active    *bool    `json:"active"`
	Scopes    []string `json:"scopes"`
	ExpiresAt *string  `json:"expires_at"`
}

type keyLifecycleEvent struct {
	ID        string    `json:"id"`
	TenantID  string    `json:"tenant_id"`
	EntityID  string    `json:"entity_id"`
	EventType string    `json:"event_type"`
	Timestamp time.Time `json:"timestamp"`
	Version   uint64    `json:"version"`
	Payload   struct {
		Key   *managedKeyRecord `json:"key"`
		KeyID string            `json:"key_id"`
	} `json:"payload"`
}

// Lifecycle reads require Core's strict retained-entity contract: tolerant hot
// queries can omit archived events, including a revocation.
func (a *AuthClient) managedKey(ctx context.Context, claims *Claims) (*managedKeyRecord, error) {
	if claims.UserID == "" || claims.TenantID == "" || a.coreURL == "" {
		return nil, errors.New("unregistered API key")
	}
	base := a.coreURL
	if a.lifecycleCoreURL != nil {
		base = a.lifecycleCoreURL(claims.TenantID)
	}
	if base == "" {
		return nil, errors.New("API key authority unavailable")
	}
	u, err := url.Parse(strings.TrimRight(base, "/") + queryEventsPath)
	if err != nil {
		return nil, errors.New("API key authority unavailable")
	}
	entity := "apikey:" + claims.TenantID
	q := u.Query()
	q.Set("tenant_id", claims.TenantID)
	q.Set("entity_id", entity)
	q.Set("limit", "1001")
	q.Set("integrity", "retained-entity-v1")
	u.RawQuery = q.Encode()
	ctx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, u.String(), http.NoBody)
	if err != nil {
		return nil, errors.New("API key authority unavailable")
	}
	bearer, err := a.SignDelegationJWT("control-plane", claims.TenantID, "admin")
	if err != nil {
		return nil, errors.New("API key authority unavailable")
	}
	req.Header.Set("Authorization", "Bearer "+bearer)
	resp, err := a.http.Do(req)
	if err != nil {
		return nil, errors.New("API key authority unavailable")
	}
	defer closeResponseBody(resp)
	if resp.StatusCode != http.StatusOK {
		return nil, errors.New("API key authority unavailable")
	}
	var page struct {
		Events    []keyLifecycleEvent `json:"events"`
		Count     *int                `json:"count"`
		Total     *int                `json:"total_count"`
		HasMore   *bool               `json:"has_more"`
		Version   *uint64             `json:"entity_version"`
		Integrity struct {
			Protocol string `json:"protocol"`
			TenantID string `json:"tenant_id"`
			EntityID string `json:"entity_id"`
		} `json:"archive_integrity"`
	}
	decoder := json.NewDecoder(io.LimitReader(resp.Body, 2*1024*1024))
	if decoder.Decode(&page) != nil || decoder.Decode(new(any)) != io.EOF || page.Count == nil || page.Total == nil || page.HasMore == nil || page.Version == nil || *page.HasMore || *page.Count != len(page.Events) || *page.Total != len(page.Events) || len(page.Events) > 1000 || page.Integrity.Protocol != "retained-entity-v1" || page.Integrity.TenantID != claims.TenantID || page.Integrity.EntityID != entity {
		return nil, errors.New("incomplete API key authority")
	}
	seen := map[string]bool{}
	var record *managedKeyRecord
	var previous uint64
	var numbered bool
	var timestamp time.Time
	revoked := false
	for _, event := range page.Events {
		// Legacy Core events used version 1; complete retained counts cover this prefix.
		legacy := !numbered && event.Version == 1
		if event.ID == "" || seen[event.ID] || event.TenantID != claims.TenantID || event.EntityID != entity || event.Timestamp.IsZero() || event.Timestamp.Before(timestamp) || (!legacy && event.Version != previous+1) {
			return nil, errors.New("invalid API key history")
		}
		seen[event.ID] = true
		previous++
		numbered = numbered || event.Version > 1
		timestamp = event.Timestamp
		switch event.EventType {
		case "apikey.created":
			key := event.Payload.Key
			if key == nil || key.ID == "" || key.TenantID != claims.TenantID {
				return nil, errors.New("invalid API key history")
			}
			if key.ID == claims.UserID {
				if revoked || key.Active == nil || !*key.Active || !validKeyScopes(key.Scopes) || (record != nil && !reflect.DeepEqual(record, key)) {
					return nil, errors.New("invalid API key history")
				}
				record = key
			}
		case "apikey.revoked":
			if event.Payload.KeyID == "" {
				return nil, errors.New("invalid API key history")
			}
			if event.Payload.KeyID == claims.UserID {
				revoked = true
			}
		default:
			return nil, errors.New("invalid API key history")
		}
	}
	if len(page.Events) == 0 || page.Events[len(page.Events)-1].Version != *page.Version {
		return nil, errors.New("unregistered API key")
	}
	if record == nil {
		return nil, errors.New("unregistered API key")
	}
	if revoked {
		return nil, errRevokedAPIKey
	}
	if claims.ExpiresAt == 0 && record.ExpiresAt != nil {
		return nil, errors.New("API key expiration contract mismatch")
	}
	if record.ExpiresAt != nil {
		expires, err := time.Parse(time.RFC3339Nano, *record.ExpiresAt)
		if err != nil || !time.Now().Before(expires) {
			return nil, errors.New("API key expired")
		}
	}
	return record, nil
}

func validKeyScopes(scopes []string) bool {
	if len(scopes) == 0 {
		return false
	}
	for _, scope := range scopes {
		switch scope {
		case managedScopeRead, managedScopeWrite, "queries:execute", "projections:read", managedProjectionWrite, managedSchemaRead, managedSchemaWrite:
		default:
			return false
		}
	}
	return true
}

func credentialType(claims *Claims) (string, error) {
	if !claims.IsAPIKey {
		if claims.CredentialKind != "" || claims.Role == entities.RoleServiceAccount {
			return "", errors.New("invalid credential kind")
		}
		return "human", nil
	}
	if claims.CredentialKind == delegationKind || (claims.CredentialKind == "" && claims.Name == "" && claims.Username == "" && claims.ExpiresAt-claims.IssuedAt <= 60) {
		if claims.Issuer != "allsource" || claims.IssuedAt <= 0 || claims.IssuedAt > time.Now().Unix()+delegationClockSkew || claims.ExpiresAt <= claims.IssuedAt || claims.ExpiresAt-claims.IssuedAt > 60 || claims.Name != "" || claims.Username != "" {
			return "", errors.New("invalid service delegation")
		}
		return delegationKind, nil
	}
	if claims.CredentialKind != "" && claims.CredentialKind != managedKeyKind {
		return "", errors.New("invalid credential kind")
	}
	return managedKeyKind, nil
}

func managedScope(c *gin.Context) string {
	path, method := c.Request.URL.Path, c.Request.Method
	if method == http.MethodGet {
		switch {
		case path == queryEventsPath, path == "/api/v1/event-feed", strings.HasPrefix(path, "/api/v1/entities/"), path == statsAPIPath:
			return managedScopeRead
		case strings.HasPrefix(path, "/api/v1/read-models/"), path == viewsPath, strings.HasPrefix(path, viewsPath+"/"):
			return "projections:read"
		case path == "/api/v1/schemas":
			return managedSchemaRead
		case strings.HasPrefix(path, "/api/v1/prime/"):
			return managedScopeRead
		}
	}
	if path == "/api/v1/schemas/validate" && method == http.MethodPost {
		return managedSchemaRead
	}
	if path == "/api/v1/schemas" && method == http.MethodPost {
		return managedSchemaWrite
	}
	if strings.HasPrefix(path, "/api/v1/read-models/") && method == http.MethodPut {
		return managedProjectionWrite
	}
	if strings.HasPrefix(path, viewsPath+"/") && (method == http.MethodPut || method == http.MethodDelete) {
		return managedProjectionWrite
	}
	if method == http.MethodPost && (path == ingestEventsPath || path == ingestBatchPath) {
		return managedScopeWrite
	}
	if strings.HasPrefix(path, "/api/v1/prime/") {
		if method == http.MethodPost {
			switch path {
			case "/api/v1/prime/recall", "/api/v1/prime/shortest-path", "/api/v1/prime/vectors/search":
				return managedScopeRead
			case "/api/v1/prime/nodes", "/api/v1/prime/edges", "/api/v1/prime/vectors":
				return managedScopeWrite
			}
		}
		if (method == http.MethodPatch || method == http.MethodDelete) && (strings.HasPrefix(path, "/api/v1/prime/nodes/") || strings.HasPrefix(path, "/api/v1/prime/edges/") || strings.HasPrefix(path, "/api/v1/prime/vectors/")) {
			return managedScopeWrite
		}
	}
	if path == "/api/v1/prime/mcp" && method == http.MethodPost {
		body, err := io.ReadAll(io.LimitReader(c.Request.Body, 2*1024*1024+1))
		if err != nil || len(body) > 2*1024*1024 {
			return ""
		}
		c.Request.Body = io.NopCloser(bytes.NewReader(body))
		var rpc struct {
			Method string `json:"method"`
			Params struct {
				Name string `json:"name"`
			} `json:"params"`
		}
		if json.Unmarshal(body, &rpc) != nil {
			return ""
		}
		switch rpc.Method {
		case "initialize", "tools/list", "notifications/initialized":
			return "protocol"
		case "tools/call":
			switch rpc.Params.Name {
			case "prime_add_node", "prime_add_edge", "prime_embed", "prime_forget", "prime_consolidate", "prime_load":
				return managedScopeWrite
			case "prime_stats", "prime_index", "prime_context", "prime_recall", "prime_search", "prime_neighbors", "prime_history", "prime_get_node", "prime_path", "prime_export":
				return managedScopeRead
			}
		}
	}
	return ""
}

func hasKeyScope(scopes []string, required string) bool {
	if required == "protocol" {
		return true
	}
	for _, scope := range scopes {
		if scope == required && required != "" {
			return true
		}
	}
	return false
}

func managedRolePermits(role entities.Role, scope string) bool {
	permission := entities.PermissionRead
	if scope == managedScopeWrite || scope == managedProjectionWrite {
		permission = entities.PermissionWrite
	}
	if scope == managedSchemaWrite {
		permission = entities.PermissionManageSchemas
	}
	return RoleHasPermission(role, permission)
}

func managedKeyAllows(c *gin.Context, key *managedKeyRecord, role entities.Role) bool {
	if c.Request.Method == http.MethodGet && c.Request.URL.Path == "/api/v1/auth/session" {
		return RoleHasPermission(role, entities.PermissionRead)
	}
	scope := managedScope(c)
	return hasKeyScope(key.Scopes, scope) && managedRolePermits(role, scope)
}

// Public event ingestion cannot manufacture the records used to grant access.
func containsKeyLifecycle(value any) bool {
	switch v := value.(type) {
	case map[string]any:
		entity, entityOK := v["entity_id"].(string)
		event, eventOK := v["event_type"].(string)
		if (entityOK && strings.HasPrefix(entity, "apikey:")) || (eventOK && strings.HasPrefix(event, "apikey.")) {
			return true
		}
		if events, exists := v["events"]; exists {
			return containsKeyLifecycle(events)
		}
	case []any:
		for _, event := range v {
			if containsKeyLifecycle(event) {
				return true
			}
		}
	}
	return false
}

func closeResponseBody(response *http.Response) {
	if err := response.Body.Close(); err != nil {
		log.Print("API key authority response close failed")
	}
}
