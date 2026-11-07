package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"sort"
	"strings"
	"time"

	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/domain/entities"
	httphandlers "github.com/allsource/control-plane/internal/interfaces/http"
)

// registerClientKeyRoutes mounts the operator routes on the admin group, so
// AdminAuthMiddleware has already refused every API credential.
func registerClientKeyRoutes(admin *gin.RouterGroup, a *AuthClient) {
	admin.POST("/tenants/:id/client-keys", MaxBodySize(16*1024), a.registerClientKeyHandler)
	admin.GET("/tenants/:id/client-keys", a.listClientKeysHandler)
	admin.DELETE("/tenants/:id/client-keys/:kid", a.revokeClientKeyHandler)
}

func clientKeyOperator(c *gin.Context) (string, bool) {
	operator, err := httphandlers.GetAdminAuthContext(c)
	if err != nil || operator.Role != entities.RoleAdmin || operator.UserID == "" {
		c.AbortWithStatusJSON(http.StatusForbidden, gin.H{"error": "forbidden", "message": "client keys are managed by platform operators only"})
		return "", false
	}
	return operator.UserID, true
}

func clientKeyTenant(c *gin.Context) (string, bool) {
	tenant := c.Param("id")
	if !validTenantID(tenant) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid_tenant", "message": "tenant id is invalid"})
		return "", false
	}
	return tenant, true
}

func (a *AuthClient) appendLifecycleEvent(ctx context.Context, tenant, entity, eventType string, payload any) error {
	base := a.lifecycleBase(tenant)
	if base == "" {
		return errKeyAuthorityUnavailable
	}
	body, err := json.Marshal(map[string]any{"tenant_id": tenant, "entity_id": entity, "event_type": eventType, "payload": payload})
	if err != nil {
		return err
	}
	ctx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, strings.TrimRight(base, "/")+ingestEventsPath, bytes.NewReader(body))
	if err != nil {
		return err
	}
	bearer, err := a.SignDelegationJWT("control-plane", tenant, entities.RoleAdmin)
	if err != nil {
		return err
	}
	req.Header.Set("Authorization", "Bearer "+bearer)
	req.Header.Set("Content-Type", "application/json")
	response, err := a.http.Do(req)
	if err != nil {
		return errKeyAuthorityUnavailable
	}
	defer closeResponseBody(response)
	var ack struct {
		ID string `json:"event_id"`
	}
	if (response.StatusCode != http.StatusOK && response.StatusCode != http.StatusCreated) || json.NewDecoder(io.LimitReader(response.Body, 65536)).Decode(&ack) != nil || ack.ID == "" {
		return errors.New("client key write was not acknowledged")
	}
	return nil
}

func clientKeyAuthorityFailure(c *gin.Context, err error) {
	if errors.Is(err, errKeyAuthorityUnavailable) {
		c.Header("Retry-After", "5")
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": "authority_unavailable", "message": "client key authority is unavailable; retry shortly"})
		return
	}
	c.JSON(http.StatusConflict, gin.H{"error": "authority_invalid", "message": err.Error()})
}

func (a *AuthClient) registerClientKeyHandler(c *gin.Context) {
	operator, ok := clientKeyOperator(c)
	if !ok {
		return
	}
	tenant, ok := clientKeyTenant(c)
	if !ok {
		return
	}
	var request struct {
		ClientID string                     `json:"client_id"`
		JWK      map[string]json.RawMessage `json:"jwk"`
		Scopes   []string                   `json:"scopes"`
	}
	decoder := json.NewDecoder(c.Request.Body)
	decoder.DisallowUnknownFields()
	if decoder.Decode(&request) != nil || decoder.Decode(new(any)) != io.EOF {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid_request", "message": "body must be {client_id, jwk, scopes}"})
		return
	}
	if _, private := request.JWK["d"]; private {
		c.JSON(http.StatusBadRequest, gin.H{"error": "private_key_refused", "message": "send the public JWK only; a JWK with d is refused"})
		return
	}
	var jwk ecJWK
	encoded, err := json.Marshal(request.JWK)
	if err == nil {
		err = json.Unmarshal(encoded, &jwk)
	}
	if err == nil {
		_, err = jwk.publicKey()
	}
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid_jwk", "message": "jwk must be a public P-256 key: kty EC, crv P-256, 32-byte base64url x and y on the curve"})
		return
	}
	if !validClientID(request.ClientID) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid_client_id", "message": "client_id must be 1-200 characters of letters, digits and . _ @ : / -"})
		return
	}
	if !validKeyScopes(request.Scopes) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid_scopes", "message": "scopes must be a non-empty subset of the managed key scopes"})
		return
	}
	jwk = ecJWK{Kty: jwk.Kty, Crv: jwk.Crv, X: jwk.X, Y: jwk.Y}
	kid := jwk.thumbprint()
	states, err := a.clientKeyStates(c.Request.Context(), tenant)
	if err != nil {
		clientKeyAuthorityFailure(c, err)
		return
	}
	active := true
	record := clientKeyRecord{Kid: kid, ClientID: request.ClientID, TenantID: tenant, Alg: assertionAlg, JWK: jwk,
		Role: entities.RoleServiceAccount, Scopes: request.Scopes, Active: &active}
	if existing := states[kid]; existing != nil {
		switch {
		case existing.revoked:
			c.JSON(http.StatusConflict, gin.H{"error": "kid_revoked", "kid": kid, "message": "this key was revoked; generate a new keypair"})
		case existing.record != nil && sameClientKey(existing.record, &record) && !existing.invalid:
			c.JSON(http.StatusOK, gin.H{"kid": kid, "tenant_id": tenant, "client_id": request.ClientID, "already_registered": true})
		default:
			c.JSON(http.StatusConflict, gin.H{"error": "kid_conflict", "kid": kid, "message": "this key is already registered with a different client or scopes; revoke it and use a new keypair"})
		}
		return
	}
	record.CreatedAt = time.Now().UTC().Format(time.RFC3339)
	record.CreatedBy = operator
	if err := a.appendLifecycleEvent(c.Request.Context(), tenant, clientKeyEntityPrefix+tenant, clientKeyRegistered, record); err != nil {
		clientKeyAuthorityFailure(c, err)
		return
	}
	a.asserted.forget(tenant, kid)
	logClientKeyWrite("registered", tenant, kid, operator)
	c.JSON(http.StatusCreated, gin.H{"kid": kid, "tenant_id": tenant, "client_id": request.ClientID, "alg": assertionAlg, "role": record.Role, "scopes": record.Scopes})
}

func (a *AuthClient) revokeClientKeyHandler(c *gin.Context) {
	operator, ok := clientKeyOperator(c)
	if !ok {
		return
	}
	tenant, ok := clientKeyTenant(c)
	if !ok {
		return
	}
	kid := c.Param("kid")
	states, err := a.clientKeyStates(c.Request.Context(), tenant)
	if err != nil {
		clientKeyAuthorityFailure(c, err)
		return
	}
	state := states[kid]
	switch {
	case state == nil || state.record == nil && !state.revoked:
		c.JSON(http.StatusNotFound, gin.H{"error": "not_found", "message": "no client key with this kid on this tenant"})
		return
	case state.revoked:
		c.JSON(http.StatusOK, gin.H{"kid": kid, "tenant_id": tenant, "revoked_at": state.revocation.RevokedAt, "already_revoked": true})
		return
	}
	revocation := clientKeyRevocation{Kid: kid, RevokedAt: time.Now().UTC().Format(time.RFC3339), RevokedBy: operator}
	if err := a.appendLifecycleEvent(c.Request.Context(), tenant, clientKeyEntityPrefix+tenant, clientKeyRevoked, revocation); err != nil {
		clientKeyAuthorityFailure(c, err)
		return
	}
	a.asserted.forget(tenant, kid)
	logClientKeyWrite("revoked", tenant, kid, operator)
	c.JSON(http.StatusOK, gin.H{"kid": kid, "tenant_id": tenant, "revoked_at": revocation.RevokedAt})
}

func (a *AuthClient) listClientKeysHandler(c *gin.Context) {
	if _, ok := clientKeyOperator(c); !ok {
		return
	}
	tenant, ok := clientKeyTenant(c)
	if !ok {
		return
	}
	states, err := a.clientKeyStates(c.Request.Context(), tenant)
	if err != nil {
		clientKeyAuthorityFailure(c, err)
		return
	}
	keys := make([]gin.H, 0, len(states))
	for kid, state := range states {
		entry := gin.H{"kid": kid, "revoked": state.revoked, "invalid": state.invalid, "active": state.usable()}
		if record := state.record; record != nil {
			entry["client_id"] = record.ClientID
			entry["alg"] = record.Alg
			entry["role"] = record.Role
			entry["scopes"] = record.Scopes
			entry["created_at"] = record.CreatedAt
			entry["created_by"] = record.CreatedBy
			entry["jwk"] = gin.H{"kty": record.JWK.Kty, "crv": record.JWK.Crv, "x": record.JWK.X, "y": record.JWK.Y}
		}
		if state.revoked {
			entry["revoked_at"] = state.revocation.RevokedAt
			entry["revoked_by"] = state.revocation.RevokedBy
		}
		keys = append(keys, entry)
	}
	sort.Slice(keys, func(i, j int) bool {
		left, leftOK := keys[i]["kid"].(string)
		right, rightOK := keys[j]["kid"].(string)
		return leftOK && rightOK && left < right
	})
	c.JSON(http.StatusOK, gin.H{"tenant_id": tenant, "client_keys": keys})
}
