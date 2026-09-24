package main

import (
	"bytes"
	"encoding/json"
	"io"
	"net/http"
	"os"
	"regexp"
	"strings"
	"time"

	"github.com/dgrijalva/jwt-go"
	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/application/usecases"
	"github.com/allsource/control-plane/internal/domain/entities"
)

var authUserIDPattern = regexp.MustCompile(`^[a-zA-Z0-9_-]{1,100}$`)

// emailAuthService verifies credentials with the durable auth service, then
// provisions a workspace and mints the JWT understood by CP and Query Service.
// Opaque better-auth session tokens must never be presented as product JWTs.
func (cp *ControlPlane) emailAuthService(c *gin.Context, signup bool, name, email, password string) {
	path := "/api/auth/sign-in/email"
	if signup {
		path = "/api/auth/sign-up/email"
	}
	body, err := json.Marshal(map[string]string{"name": name, "email": strings.ToLower(strings.TrimSpace(email)), "password": password})
	if err != nil {
		c.JSON(500, gin.H{"message": "Unable to create session"})
		return
	}
	req, err := http.NewRequestWithContext(c.Request.Context(), http.MethodPost, strings.TrimRight(os.Getenv("AUTH_SERVICE_URL"), "/")+path, bytes.NewReader(body)) //nolint:gosec // G704 false positive: URL is operator-set env plus a constant path, not user input
	if err != nil {
		c.JSON(503, gin.H{"message": "Authentication service is unavailable"})
		return
	}
	req.Header.Set("Content-Type", "application/json")
	client := &http.Client{Timeout: 15 * time.Second, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}
	resp, err := client.Do(req) //nolint:gosec // G704 false positive: see above
	if err != nil {
		c.JSON(503, gin.H{"message": "Authentication service is unavailable"})
		return
	}
	defer func() { _ = resp.Body.Close() }() //nolint:errcheck // close-on-defer, non-actionable
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		status := resp.StatusCode
		message := "Unable to authenticate. Check your details or sign in if already registered."
		if status >= 500 || status < 400 {
			status = http.StatusBadGateway
			message = "Authentication service is unavailable"
		}
		// Do not forward upstream debug details, cookies or credentials.
		c.JSON(status, gin.H{"message": message})
		return
	}
	var result struct {
		Token string `json:"token"`
		User  struct {
			ID    string `json:"id"`
			Email string `json:"email"`
			Name  string `json:"name"`
		} `json:"user"`
	}
	if err := json.NewDecoder(io.LimitReader(resp.Body, 1<<20)).Decode(&result); err != nil || result.Token == "" || !authUserIDPattern.MatchString(result.User.ID) || result.User.Email == "" {
		c.JSON(502, gin.H{"message": "Invalid authentication response"})
		return
	}

	// Never associate an unverified email with an existing OAuth workspace or
	// ADMIN_EMAILS. Workspace identity comes from the authenticated user ID.
	tenantID := "email-" + result.User.ID
	subscription, _ := usecases.TrialSubscriptionMetadata(time.Now())
	tenantResp, err := cp.client.R().SetContext(c.Request.Context()).SetBody(map[string]interface{}{
		"id": tenantID, "slug": tenantID, "name": result.User.Name,
		"quota_preset": "trial",
	}).Post("/api/v1/tenants")
	if err != nil {
		c.JSON(503, gin.H{"message": "Workspace setup unavailable. Your account is saved; try signing in again."})
		return
	}
	status := tenantResp.StatusCode()
	exists := status == 409 || (status == 400 && strings.Contains(string(tenantResp.Body()), "already exists"))
	if status != 200 && status != 201 && !exists {
		c.JSON(502, gin.H{"message": "Workspace setup failed. Your account is saved; try signing in again."})
		return
	}
	// Core's create DTO does not accept metadata. Persist the trial separately.
	// On a retry, inspect the existing workspace before changing anything so a
	// paid subscription is never overwritten and a partial setup can recover.
	if exists {
		tenantResp, err = cp.client.R().SetContext(c.Request.Context()).Get("/api/v1/tenants/" + tenantID)
		if err != nil || tenantResp.StatusCode() != 200 {
			c.JSON(503, gin.H{"message": "Unable to load workspace. Try signing in again."})
			return
		}
	}
	var tenant struct {
		CreatedAt time.Time              `json:"created_at"`
		Metadata  map[string]interface{} `json:"metadata"`
	}
	if err := json.Unmarshal(tenantResp.Body(), &tenant); err != nil {
		c.JSON(502, gin.H{"message": "Invalid workspace response"})
		return
	}
	if tenant.Metadata == nil {
		tenant.Metadata = map[string]interface{}{}
	}
	if _, configured := tenant.Metadata["subscription"]; !configured {
		if !tenant.CreatedAt.IsZero() {
			subscription, _ = usecases.TrialSubscriptionMetadata(tenant.CreatedAt)
		}
		tenant.Metadata["subscription"] = subscription
		tenant.Metadata["quota"] = usecases.TrialQuotaMetadata()
		// Query Service's tenant/usage response reads the plural legacy key.
		// Keep both consumers consistent until that wire format is unified.
		tenant.Metadata["quotas"] = usecases.TrialQuotaMetadata()
		updated, updateErr := cp.client.R().SetContext(c.Request.Context()).SetBody(map[string]interface{}{"metadata": tenant.Metadata}).Put("/api/v1/tenants/" + tenantID)
		if updateErr != nil || updated.StatusCode() != 200 {
			c.JSON(503, gin.H{"message": "Trial setup unavailable. Your account is saved; try signing in again."})
			return
		}
	}
	now := time.Now()
	claims := &Claims{
		UserID: result.User.ID, Username: result.User.Name, Email: result.User.Email,
		Name: result.User.Name, TenantID: tenantID, Role: entities.RoleDeveloper, Provider: "email",
		StandardClaims: jwt.StandardClaims{Subject: result.User.ID, Issuer: "allsource", IssuedAt: now.Unix(), ExpiresAt: now.Add(7 * 24 * time.Hour).Unix()},
	}
	token, err := jwt.NewWithClaims(jwt.SigningMethodHS256, claims).SignedString([]byte(cp.authClient.jwtSecret))
	if err != nil {
		c.JSON(500, gin.H{"message": "Unable to create session"})
		return
	}
	responseStatus := http.StatusOK
	if signup {
		responseStatus = http.StatusCreated
	}
	c.Header("Cache-Control", "no-store")
	c.JSON(responseStatus, gin.H{"token": token, "new_user": status == 201, "user": gin.H{
		"id": result.User.ID, "email": result.User.Email, "name": result.User.Name, "tenant_id": tenantID,
	}})
}
