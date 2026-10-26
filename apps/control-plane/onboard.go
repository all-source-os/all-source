package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/google/uuid"

	"github.com/allsource/control-plane/internal/application/dto"
	"github.com/allsource/control-plane/internal/application/usecases"
	"github.com/allsource/control-plane/internal/domain"
	"github.com/allsource/control-plane/internal/domain/entities"
)

// envValueTrue is the canonical truthy value accepted by the boolean env-var
// gates in this package (alongside "1" and "yes"), compared case-insensitively
// after trimming.
const envValueTrue = "true"

// demoEnabled reports whether demo-account provisioning is turned on. It gates
// DemoStartHandler, which CREATES A TENANT on every call. Default OFF: with
// DEMO_ENABLED unset/false the endpoint mints nothing and returns 403, so a
// stray liveness/status probe can never litter /tenants with "Demo User"
// tenants again (the gap-1 recurrence guard — DEMO_ENABLED must be explicitly
// set to enable the demo flow). Accepts true/1/yes (case-insensitive).
//
// LIVENESS/STATUS PROBES MUST NEVER CALL /api/v1/demo/* — minting a tenant from
// a health check is a defect (see TestNoStatusPathReferencesDemo). Demo
// provisioning is an opt-in, never an ambient side effect.
func demoEnabled() bool {
	switch strings.ToLower(strings.TrimSpace(os.Getenv("DEMO_ENABLED"))) {
	case envValueTrue, "1", "yes":
		return true
	default:
		return false
	}
}

// OnboardRequest represents a request to start onboarding.
//
// DiscoverySource/DiscoveryPrompt are the GEO layer-4 self-report capture
// (see geo_selfreport.go). Both are OPTIONAL and the endpoint's behavior is
// unchanged when they are absent — same status, same response keys, same
// values, no event written. That is not a nicety: this endpoint is published
// in apps/web/public/llms.txt and live agents call it, so a required field
// here would break callers in the wild. TestOnboardRequestBindsWithoutTheGeoFields
// pins that.
type OnboardRequest struct {
	Email string `json:"email" binding:"required"`
	Name  string `json:"name"`
	// How the caller found AllSource — one of the ids in
	// docs/contracts/geo-events/discovery-sources.json ("chatgpt", "search").
	// An absent or unrecognized value is silently ignored, never an error.
	DiscoverySource string `json:"discovery_source"`
	// What they asked the assistant, verbatim. Only stored for the AI sources.
	// This is the highest-value field in the layer: an agent's actual prompt is
	// first-party buyer language no probe harness can synthesize.
	DiscoveryPrompt string `json:"discovery_prompt"`
}

// OnboardHandler provisions a new tenant with an API key and sample events.
// POST /api/v1/onboard/start
func (cp *ControlPlane) OnboardHandler(c *gin.Context) {
	var req OnboardRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request", "message": err.Error()})
		return
	}

	// Generate a deterministic tenant ID from email
	tenantID := fmt.Sprintf("onboard-%s", entities.TenantSlug(req.Email))

	name := req.Name
	if name == "" {
		name = strings.Split(req.Email, "@")[0]
	}

	// New self-service signups start a 14-day trial, NOT a permanent free tier
	// (prompt 048 — the marketing/catalog already say "no free plan"). The tier
	// label + trial_expires_at come from the shared usecases.TrialSubscriptionMetadata
	// so onboard, OAuth, and agent-register can't drift; the scheduler's
	// trial-expiry sweep suspends the tenant when trial_expires_at passes.
	subscription, _ := usecases.TrialSubscriptionMetadata(time.Now())

	// Create tenant via the CreateTenantUseCase
	tenantResp, err := cp.container.CreateTenantUC.Execute(dto.CreateTenantRequest{
		ID:          tenantID,
		Name:        name,
		Description: fmt.Sprintf("Onboarded tenant for %s", req.Email),
		Metadata: map[string]interface{}{
			"email":        req.Email,
			"subscription": subscription,
			"quota":        usecases.TrialQuotaMetadata(),
		},
	})
	if err != nil {
		// If tenant already exists, that's fine — return a helpful error
		if errors.Is(err, domain.ErrTenantAlreadyExists) {
			c.JSON(http.StatusConflict, gin.H{
				"error":   "tenant_exists",
				"message": "A tenant with this email already exists. Use /api/v1/auth/login to authenticate.",
			})
			return
		}
		c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to create tenant", "message": err.Error()})
		return
	}

	// Generate API key via shared signing function
	apiKey, err := cp.authClient.IssueManagedAPIKey(c.Request.Context(), tenantResp.ID, name, entities.RoleDeveloper)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to generate API key"})
		return
	}

	// Ingest sample events into Core for the new tenant
	sampleEvents := buildSampleEvents(tenantResp.ID)
	var ingestedCount int
	for _, event := range sampleEvents {
		resp, postErr := cp.client.R().
			SetBody(event).
			Post("/api/v1/events")
		if postErr == nil && resp.StatusCode() < 400 {
			ingestedCount++
		}
	}

	// GEO layer 4 — record how this caller found us, if they told us.
	//
	// Written AFTER the tenant and key exist, and its failure is only logged:
	// the endpoint's job is to mint a tenant, and losing an attribution row is
	// vastly cheaper than failing a signup over telemetry. Nothing above this
	// line depends on it.
	if envelope, ok := buildGeoSelfReport(
		tenantResp.ID, usecases.TrialTierName, req.DiscoverySource, req.DiscoveryPrompt, time.Now(),
	); ok {
		resp, postErr := cp.client.R().SetBody(envelope).Post("/api/v1/events")
		if postErr != nil || resp.StatusCode() >= 400 {
			log.Printf("[geo] geo.selfreport.captured not stored for %s: %v", tenantResp.ID, postErr)
		}
	}

	// Build sample curl commands
	baseURL := cp.getPublicBaseURL(c)
	curls := buildSampleCurls(baseURL, apiKey, tenantResp.ID)

	c.JSON(http.StatusCreated, gin.H{
		"tenant_id":        tenantResp.ID,
		"api_key":          apiKey,
		"sample_events":    ingestedCount,
		"tier":             usecases.TrialTierName,
		"trial_expires_at": subscription["trial_expires_at"],
		"events_quota":     usecases.TrialEventsQuota,
		"getting_started":  curls,
	})
}

// DemoStartHandler provisions a demo account through the normal registration flow.
// Generates demo credentials, registers them via Core auth, creates a demo tenant,
// seeds sample data, and returns the email + password for the user to log in normally.
//
// THIS HANDLER CREATES A TENANT ON EVERY CALL. It is gated behind DEMO_ENABLED
// (default OFF) so a stray status/liveness probe can never mint a "Demo User"
// tenant — the gap-1 demo-litter recurrence guard. A liveness/status probe must
// NEVER reference /api/v1/demo/* (enforced by TestNoStatusPathReferencesDemo).
// To clean up demo tenants already created, use POST /api/v1/admin/tenants/reap-demo.
//
// POST /api/v1/demo/start
func (cp *ControlPlane) DemoStartHandler(c *gin.Context) {
	// Gate: demo provisioning is opt-in. With DEMO_ENABLED unset/false the
	// endpoint creates nothing and returns 403 — no tenant, no events, no litter.
	if !demoEnabled() {
		c.JSON(http.StatusForbidden, gin.H{
			"error":   "demo_disabled",
			"message": "demo provisioning is disabled (set DEMO_ENABLED=true to enable)",
		})
		return
	}

	// Generate unique demo credentials
	demoSlug := uuid.New().String()[:8]
	email := fmt.Sprintf("demo-%s@demo.allsource.dev", demoSlug)
	password := fmt.Sprintf("demo-%s-%s", demoSlug, uuid.New().String()[:8])
	name := "Demo User"

	// Compute tenant ID deterministically from the email BEFORE registration
	// so we can pass the same ID to both Core auth (user.tenant_id) and Core tenants.
	tenantID := entities.TenantSlug(email)

	// Step 1: Register credentials in Core (same path as normal RegisterHandler)
	// Include tenant_id so Core's user record references the right tenant.
	regResp, err := cp.client.R().
		SetBody(map[string]interface{}{
			"username":  email,
			"email":     email,
			"password":  password,
			"tenant_id": tenantID,
		}).
		Post("/api/v1/auth/register")

	if err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": "service_unavailable", "message": "registration service is temporarily unavailable"})
		return
	}

	if regResp.StatusCode() != 201 && regResp.StatusCode() != 200 {
		c.JSON(http.StatusInternalServerError, gin.H{
			"error":   "failed to create demo account",
			"message": fmt.Sprintf("registration returned HTTP %d", regResp.StatusCode()),
		})
		return
	}

	// Step 2: Create tenant with is_demo: true and enterprise quotas
	// Use the same tenantID that was registered with the user.
	tenantBody := map[string]interface{}{
		"id":      tenantID,
		"name":    name,
		"slug":    tenantID,
		"is_demo": true,
		"metadata": map[string]interface{}{
			"email": email,
			"subscription": map[string]interface{}{
				"tier":   "enterprise",
				"status": "active",
			},
			"quotas": map[string]interface{}{
				"events_quota":  -1,
				"queries_quota": -1,
			},
		},
	}

	tenantResp, err := cp.client.R().
		SetBody(tenantBody).
		Post("/api/v1/tenants")
	if err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": "core service unavailable", "message": err.Error()})
		return
	}

	switch {
	case tenantResp.StatusCode() == 201 || tenantResp.StatusCode() == 200:
		// Tenant created successfully — tenantID already set
	case tenantResp.StatusCode() == 409:
		// Tenant already exists — tenantID already set
	default:
		c.JSON(http.StatusInternalServerError, gin.H{
			"error":   "failed to create demo tenant",
			"message": fmt.Sprintf("Core returned HTTP %d", tenantResp.StatusCode()),
		})
		return
	}

	// Step 3: Seed sample data
	sampleEvents := buildSampleEvents(tenantID)
	var ingestedCount int
	for _, event := range sampleEvents {
		r, postErr := cp.client.R().
			SetBody(event).
			Post("/api/v1/events")
		if postErr == nil && r.StatusCode() < 400 {
			ingestedCount++
		}
	}

	// Trigger Core's rich demo seed (1000 events with embeddings, idempotent)
	seedResp, seedErr := cp.client.R().
		Post("/api/v1/demo/seed")
	if seedErr == nil && seedResp.StatusCode() < 400 {
		var seedResult map[string]interface{}
		if json.Unmarshal(seedResp.Body(), &seedResult) == nil {
			if count, ok := seedResult["event_count"].(float64); ok {
				ingestedCount += int(count)
			}
		}
	}

	// Step 4: Seed audit log entries so the Audit Log page isn't empty
	auditEvents := buildDemoAuditEntries(tenantID, email)
	for _, ae := range auditEvents {
		cp.client.R().SetBody(ae).Post("/api/v1/events") //nolint:errcheck // best-effort seeding
	}

	// Return credentials — the client logs in through the normal login flow
	c.JSON(http.StatusCreated, gin.H{
		"email":         email,
		"password":      password,
		"is_demo":       true,
		"sample_events": ingestedCount,
	})
}

// getPublicBaseURL returns the base URL for curl examples.
func (cp *ControlPlane) getPublicBaseURL(c *gin.Context) string {
	scheme := "https"
	if c.Request.TLS == nil {
		scheme = "http"
	}
	return fmt.Sprintf("%s://%s", scheme, c.Request.Host)
}

// buildSampleEvents returns 7 sample events that demonstrate common event sourcing patterns.
func buildSampleEvents(tenantID string) []map[string]interface{} {
	userID := uuid.New().String()
	orderID := uuid.New().String()

	return []map[string]interface{}{
		{
			"event_type": "user.signed_up",
			"entity_id":  userID,
			"tenant_id":  tenantID,
			"payload": map[string]interface{}{
				"email":    "alice@example.com",
				"plan":     "free",
				"referral": "organic",
			},
			"metadata": map[string]interface{}{
				"source":  "onboarding_sample",
				"ip":      "192.168.1.1",
				"browser": "Chrome/120",
			},
		},
		{
			"event_type": "user.profile_updated",
			"entity_id":  userID,
			"tenant_id":  tenantID,
			"payload": map[string]interface{}{
				"field":     "display_name",
				"old_value": "",
				"new_value": "Alice Smith",
			},
			"metadata": map[string]interface{}{
				"source": "onboarding_sample",
			},
		},
		{
			"event_type": "order.created",
			"entity_id":  orderID,
			"tenant_id":  tenantID,
			"payload": map[string]interface{}{
				"user_id":  userID,
				"total":    49.99,
				"currency": "USD",
				"items": []map[string]interface{}{
					{"sku": "WIDGET-001", "qty": 2, "price": 24.99},
				},
			},
			"metadata": map[string]interface{}{
				"source": "onboarding_sample",
			},
		},
		{
			"event_type": "payment.processed",
			"entity_id":  orderID,
			"tenant_id":  tenantID,
			"payload": map[string]interface{}{
				"amount":         49.99,
				"currency":       "USD",
				"payment_method": "card",
				"last_four":      "4242",
			},
			"metadata": map[string]interface{}{
				"source": "onboarding_sample",
			},
		},
		{
			"event_type": "order.fulfilled",
			"entity_id":  orderID,
			"tenant_id":  tenantID,
			"payload": map[string]interface{}{
				"tracking_number": "1Z999AA10123456784",
				"carrier":         "UPS",
			},
			"metadata": map[string]interface{}{
				"source": "onboarding_sample",
			},
		},
		{
			"event_type": "user.feature_used",
			"entity_id":  userID,
			"tenant_id":  tenantID,
			"payload": map[string]interface{}{
				"feature":  "dashboard",
				"duration": 120,
				"actions":  5,
			},
			"metadata": map[string]interface{}{
				"source": "onboarding_sample",
			},
		},
		{
			"event_type": "notification.sent",
			"entity_id":  userID,
			"tenant_id":  tenantID,
			"payload": map[string]interface{}{
				"channel":  "email",
				"template": "order_confirmation",
				"order_id": orderID,
			},
			"metadata": map[string]interface{}{
				"source": "onboarding_sample",
			},
		},
	}
}

// buildDemoAuditEntries creates sample audit log events so the Audit Log page
// shows realistic data for demo accounts. The QS audit log reads events with
// entity_id "audit:{tenant_id}" and event_type "audit.{action}".
func buildDemoAuditEntries(tenantID, email string) []map[string]interface{} {
	now := time.Now()

	entries := []struct {
		action  string
		details map[string]interface{}
		ago     time.Duration
	}{
		{"api_key.created", map[string]interface{}{"key_name": "Production API Key", "scopes": []string{"events:read", "events:write"}}, 72 * time.Hour},
		{"member.invited", map[string]interface{}{"email": "dev@example.com", "role": "member"}, 48 * time.Hour},
		{"webhook.created", map[string]interface{}{"url": "https://hooks.example.com/events", "events": []string{"order.created"}}, 24 * time.Hour},
		{"api_key.created", map[string]interface{}{"key_name": "Staging Key", "scopes": []string{"events:read"}}, 12 * time.Hour},
		{"plan.changed", map[string]interface{}{"from": "free", "to": "enterprise"}, 6 * time.Hour},
		{"api_key.revoked", map[string]interface{}{"key_name": "Old Test Key", "reason": "expired"}, 2 * time.Hour},
	}

	events := make([]map[string]interface{}, 0, len(entries))
	for _, e := range entries {
		events = append(events, map[string]interface{}{
			"entity_id":  fmt.Sprintf("audit:%s", tenantID),
			"event_type": fmt.Sprintf("audit.%s", e.action),
			"tenant_id":  tenantID,
			"payload": map[string]interface{}{
				"actor":       email,
				"action":      e.action,
				"details":     e.details,
				"recorded_at": now.Add(-e.ago).Format(time.RFC3339),
			},
			"metadata": map[string]interface{}{
				"source": "demo_seed",
			},
		})
	}
	return events
}

// buildSampleCurls returns sample curl commands for the new tenant.
func buildSampleCurls(baseURL, apiKey, tenantID string) map[string]string {
	return map[string]string{
		"ingest_event": fmt.Sprintf(
			`curl -X POST %s/api/v1/events -H "Authorization: Bearer %s" -H "Content-Type: application/json" -d '{"event_type":"user.action","entity_id":"user-123","payload":{"action":"click","page":"/home"}}'`,
			baseURL, apiKey,
		),
		"query_events": fmt.Sprintf(
			`curl %s/api/v1/events -H "Authorization: Bearer %s"`,
			baseURL, apiKey,
		),
		"query_by_type": fmt.Sprintf(
			`curl "%s/api/v1/events?event_type=order.created" -H "Authorization: Bearer %s"`,
			baseURL, apiKey,
		),
		"list_streams": fmt.Sprintf(
			`curl %s/api/v1/streams -H "Authorization: Bearer %s"`,
			baseURL, apiKey,
		),
		"list_event_types": fmt.Sprintf(
			`curl %s/api/v1/event-types -H "Authorization: Bearer %s"`,
			baseURL, apiKey,
		),
	}
}
