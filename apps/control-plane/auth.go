package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"

	"net/http"
	"os"
	"strings"
	"sync"
	"time"

	"github.com/allsource/control-plane/internal/logsafe"

	"github.com/dgrijalva/jwt-go"
	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/application/usecases"
	"github.com/allsource/control-plane/internal/domain/entities"
)

// Claims represents JWT claims
type Claims struct {
	UserID   string        `json:"sub"`
	Username string        `json:"username"`
	Email    string        `json:"email,omitempty"`
	Name     string        `json:"name,omitempty"`
	TenantID string        `json:"tenant_id"`
	Role     entities.Role `json:"role"`
	Provider string        `json:"provider,omitempty"`
	IsAPIKey bool          `json:"is_api_key,omitempty"`
	IsDemo   bool          `json:"is_demo,omitempty"`
	// ViewAs marks a read-only "view as tenant" impersonation token minted by
	// SignViewAsJWT. It is the defense-in-depth marker the write-refusal
	// middleware (ViewAsWriteRefusal) hard-rejects on any mutating method, in
	// addition to the readonly role already blocking writes (ADMIN_TENANT_POWER_TOOL
	// §5.2). A normal session NEVER carries this.
	ViewAs bool `json:"view_as,omitempty"`
	// ActAs records the real admin user id behind a view_as token so the audit
	// trail attributes the impersonation to a human even though TenantID points at
	// the viewed tenant. Set only on view_as tokens.
	ActAs string `json:"act_as,omitempty"`
	jwt.StandardClaims
}

// AuthContext holds authentication information for a request
type AuthContext struct {
	UserID   string
	Username string
	TenantID string
	Role     entities.Role
	IsAPIKey bool
}

// roleForEmail returns RoleAdmin if the email is in the ADMIN_EMAILS allowlist
// (comma-separated, case-insensitive), otherwise RoleDeveloper. Used to grant
// the admin app access to specific humans without a database.
func roleForEmail(email string) entities.Role {
	allowlist := os.Getenv("ADMIN_EMAILS")
	if allowlist == "" {
		return entities.RoleDeveloper
	}
	target := strings.ToLower(strings.TrimSpace(email))
	if target == "" {
		return entities.RoleDeveloper
	}
	for _, raw := range strings.Split(allowlist, ",") {
		if strings.EqualFold(strings.TrimSpace(raw), target) {
			return entities.RoleAdmin
		}
	}
	return entities.RoleDeveloper
}

// AuthClient handles authentication with the core service.
// NOTE: JWT_SECRET is shared between CP and QS via HMAC-SHA256 (HS256).
// Adding new services to the fleet requires distributing another copy.
// TODO: Migrate to RS256 with a JWKS endpoint on CP so other services can verify
// tokens without possessing the signing key.
type AuthClient struct {
	jwtSecret string
	coreURL   string
	http      *http.Client

	apiKeyCache   map[string]apiKeyCacheEntry
	apiKeyCacheMu sync.RWMutex
}

// apiKeyCacheTTL is how long a validated ask_ key (resolved via Core /me)
// stays cached. Matches Query Service's 120s window so key-revocation
// latency is uniform. Keys minted by CP itself (via RememberAPIKey) get
// no expiration — they're authoritative until explicitly revoked.
const apiKeyCacheTTL = 120 * time.Second

type apiKeyCacheEntry struct {
	claims *Claims
	// expiresAt.IsZero() means "no expiration" — used for locally-minted keys
	// where CP is the source of truth. For Core /me-resolved entries, this is
	// now + apiKeyCacheTTL.
	expiresAt time.Time
}

func (e apiKeyCacheEntry) fresh(now time.Time) bool {
	return e.expiresAt.IsZero() || now.Before(e.expiresAt)
}

// NewAuthClient creates a new authentication client.
// coreURL is required for validating ask_ API keys against Core's /api/v1/auth/me.
func NewAuthClient(jwtSecret, coreURL string) *AuthClient {
	if jwtSecret == "" {
		jwtSecret = "default-secret-change-in-production"
	}
	return &AuthClient{
		jwtSecret:   jwtSecret,
		coreURL:     coreURL,
		http:        &http.Client{Timeout: 5 * time.Second},
		apiKeyCache: make(map[string]apiKeyCacheEntry),
	}
}

// SignDelegationJWT mints a short-lived JWT scoped to a single data-plane
// request. The delegation layer uses this so backends (Core, Query Service)
// see the authenticated caller's tenant + role, not Control Plane's admin
// service identity — tenant isolation works end-to-end without every
// backend needing to validate ask_ keys directly.
//
// 60 second TTL is comfortably longer than any single forwarded request but
// short enough that a leaked token is near-useless.
func (a *AuthClient) SignDelegationJWT(userID, tenantID string, role entities.Role) (string, error) {
	now := time.Now()
	claims := &Claims{
		UserID:   userID,
		TenantID: tenantID,
		Role:     role,
		IsAPIKey: true,
		StandardClaims: jwt.StandardClaims{
			ExpiresAt: now.Add(60 * time.Second).Unix(),
			IssuedAt:  now.Unix(),
			Issuer:    "allsource",
			Subject:   userID,
		},
	}
	return jwt.NewWithClaims(jwt.SigningMethodHS256, claims).SignedString([]byte(a.jwtSecret))
}

// SignViewAsJWT mints a read-only "view as tenant" impersonation token — a
// deliberate sibling of SignDelegationJWT and DISTINCT from the tenant's real
// session (ADMIN_TENANT_POWER_TOOL §5.1). The admin operator uses it to view the
// product as a tenant sees it, for support/debugging, WITHOUT ever possessing or
// replaying the tenant's actual credentials.
//
// The token is read-only BY CONSTRUCTION, on three independent layers:
//   - role:"readonly" (entities.RoleReadOnly) — write endpoints reject it
//     (RoleHasPermission grants readonly only Read+Metrics, never Write/Admin).
//   - view_as:true — the ViewAsWriteRefusal middleware hard-refuses this token on
//     ANY mutating method (POST/PUT/PATCH/DELETE), independent of role, and alarms
//     the attempt as a Core audit event. Belt and suspenders.
//   - act_as:adminUserID — every read is attributable to the real admin, and the
//     token can be expired/revoked independently of the tenant's session.
//
// sub is the ADMIN's user id (WHO is impersonating), never the tenant's real user
// id. tenant_id is the VIEWED tenant. TTL is usecases.ViewAsTokenTTL (15m, the
// canonical value). The tenant's real session is untouched.
func (a *AuthClient) SignViewAsJWT(adminUserID, targetTenantID string) (string, error) {
	now := time.Now()
	claims := &Claims{
		UserID:   adminUserID,           // WHO is impersonating — never the tenant's real user id
		TenantID: targetTenantID,        // the tenant whose data is being viewed
		Role:     entities.RoleReadOnly, // NOT serviceaccount, NOT admin, NOT the tenant's role
		ViewAs:   true,                  // enforced downstream by ViewAsWriteRefusal
		ActAs:    adminUserID,           // audit trail: the real actor behind the view
		StandardClaims: jwt.StandardClaims{
			ExpiresAt: now.Add(usecases.ViewAsTokenTTL).Unix(),
			IssuedAt:  now.Unix(),
			Issuer:    "allsource",
			Subject:   adminUserID,
		},
	}
	return jwt.NewWithClaims(jwt.SigningMethodHS256, claims).SignedString([]byte(a.jwtSecret))
}

// SignAPIKey generates a long-lived JWT API key for a given tenant and role.
// Used by agent registration, onboarding, and demo flows.
func (a *AuthClient) SignAPIKey(tenantID, username string, role entities.Role) (string, error) {
	now := time.Now()
	claims := &Claims{
		UserID:   tenantID,
		Username: username,
		Name:     username,
		TenantID: tenantID,
		Role:     role,
		IsAPIKey: true,
		StandardClaims: jwt.StandardClaims{
			ExpiresAt: now.Add(365 * 24 * time.Hour).Unix(),
			IssuedAt:  now.Unix(),
			Issuer:    "allsource",
			Subject:   tenantID,
		},
	}
	token := jwt.NewWithClaims(jwt.SigningMethodHS256, claims)
	return token.SignedString([]byte(a.jwtSecret))
}

// ValidateToken validates a JWT token and returns claims
func (a *AuthClient) ValidateToken(tokenString string) (*Claims, error) {
	// Parse the token
	token, err := jwt.ParseWithClaims(tokenString, &Claims{}, func(token *jwt.Token) (interface{}, error) {
		// Validate the signing method
		if _, ok := token.Method.(*jwt.SigningMethodHMAC); !ok {
			return nil, fmt.Errorf("unexpected signing method: %v", token.Header["alg"])
		}
		return []byte(a.jwtSecret), nil
	})

	if err != nil {
		return nil, fmt.Errorf("failed to parse token: %w", err)
	}

	claims, ok := token.Claims.(*Claims)
	if !ok || !token.Valid {
		return nil, errors.New("invalid token")
	}

	// Check expiration
	if claims.ExpiresAt < time.Now().Unix() {
		return nil, errors.New("token expired")
	}

	return claims, nil
}

// RememberAPIKey caches claims for an ask_ key that CP just minted. This
// bypasses the Core /me round-trip on subsequent ValidateAPIKey calls,
// which is the first step toward removing Core's public auth surface
// entirely (bead t-62f3). Entry has no expiration — keys are valid until
// explicitly revoked; revocation invalidation is a follow-up concern.
//
// The AuthMiddleware path still tolerates Core /me as a fallback for
// keys minted before this cache existed (or minted by any future path
// we haven't instrumented), so migration is incremental and zero-risk.
func (a *AuthClient) RememberAPIKey(key string, claims *Claims) {
	if key == "" || claims == nil {
		return
	}
	a.apiKeyCacheMu.Lock()
	a.apiKeyCache[key] = apiKeyCacheEntry{
		claims:    claims,
		expiresAt: time.Time{}, // no expiration
	}
	a.apiKeyCacheMu.Unlock()
}

// ValidateAPIKey validates an ask_-prefixed API key by delegating to Core,
// which is the source of truth for API key existence and tenant binding.
// Returns synthetic Claims derived from Core's /api/v1/auth/me response.
//
// Core's /me endpoint accepts both JWT and ask_ keys and returns the resolved
// identity — we reuse that rather than re-validating the key locally, because
// CP has no access to the key store. A 120s cache absorbs the per-request
// round-trip; invalidation is best-effort via TTL (matching Query Service).
func (a *AuthClient) ValidateAPIKey(ctx context.Context, token string) (*Claims, error) {
	if a.coreURL == "" {
		return nil, errors.New("control plane not configured to validate API keys (coreURL unset)")
	}

	// Cache lookup. Entries minted by RememberAPIKey have no expiration;
	// entries resolved via Core /me expire after apiKeyCacheTTL.
	a.apiKeyCacheMu.RLock()
	if entry, ok := a.apiKeyCache[token]; ok && entry.fresh(time.Now()) {
		a.apiKeyCacheMu.RUnlock()
		return entry.claims, nil
	}
	a.apiKeyCacheMu.RUnlock()

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, strings.TrimRight(a.coreURL, "/")+"/api/v1/auth/me", http.NoBody)
	if err != nil {
		return nil, fmt.Errorf("build /me request: %w", err)
	}
	req.Header.Set("Authorization", "Bearer "+token)

	resp, err := a.http.Do(req)
	if err != nil {
		return nil, fmt.Errorf("call Core /me: %w", err)
	}
	defer func() { _ = resp.Body.Close() }() //nolint:errcheck // close-on-defer, non-actionable

	if resp.StatusCode != http.StatusOK {
		body, readErr := io.ReadAll(io.LimitReader(resp.Body, 512))
		if readErr != nil {
			return nil, fmt.Errorf("invalid api key (Core /me HTTP %d: body read failed: %w)", resp.StatusCode, readErr)
		}
		return nil, fmt.Errorf("invalid api key (Core /me HTTP %d: %s)", resp.StatusCode, strings.TrimSpace(string(body)))
	}

	var me struct {
		ID       string        `json:"id"`
		Username string        `json:"username"`
		Email    string        `json:"email"`
		Role     entities.Role `json:"role"`
		TenantID string        `json:"tenant_id"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&me); err != nil {
		return nil, fmt.Errorf("decode Core /me response: %w", err)
	}

	claims := &Claims{
		UserID:   me.ID,
		Username: me.Username,
		Email:    me.Email,
		Name:     me.Username,
		TenantID: me.TenantID,
		Role:     me.Role,
		IsAPIKey: true,
	}

	a.apiKeyCacheMu.Lock()
	a.apiKeyCache[token] = apiKeyCacheEntry{
		claims:    claims,
		expiresAt: time.Now().Add(apiKeyCacheTTL),
	}
	a.apiKeyCacheMu.Unlock()

	return claims, nil
}

// ExtractToken extracts the token from the Authorization header
func ExtractToken(c *gin.Context) (string, error) {
	authHeader := c.GetHeader("Authorization")
	if authHeader == "" {
		return "", errors.New("no authorization header")
	}

	// Expected format: "Bearer <token>"
	parts := strings.SplitN(authHeader, " ", 2)
	if len(parts) != 2 || parts[0] != "Bearer" {
		return "", errors.New("invalid authorization header format")
	}

	return parts[1], nil
}

// RoleHasPermission checks if a role has a specific permission.
func RoleHasPermission(role entities.Role, perm entities.Permission) bool {
	switch role {
	case entities.RoleAdmin:
		return true
	case entities.RoleDeveloper:
		return perm == entities.PermissionRead || perm == entities.PermissionWrite ||
			perm == entities.PermissionMetrics || perm == entities.PermissionManageSchemas ||
			perm == entities.PermissionManagePipelines
	case entities.RoleReadOnly:
		return perm == entities.PermissionRead || perm == entities.PermissionMetrics
	case entities.RoleServiceAccount:
		return perm == entities.PermissionRead || perm == entities.PermissionWrite
	default:
		return false
	}
}

// AuthMiddleware validates JWT tokens and adds auth context to requests
func AuthMiddleware(authClient *AuthClient) gin.HandlerFunc {
	return func(c *gin.Context) {
		// Skip auth for health, metrics, public cluster health, webhooks, auth, and admin endpoints.
		// Admin routes use their own AdminAuthMiddleware with self-contained JWT validation.
		path := c.Request.URL.Path
		isPublicAuthPath := strings.HasPrefix(path, "/api/v1/auth/") && path != "/api/v1/auth/session"
		isPublicStatusPath := path == "/api/v1/status/services"
		// Public pricing catalog — list prices read from LemonSqueezy for the
		// marketing /pricing page and dashboard billing. No identity needed.
		isPublicCatalog := path == "/api/v1/billing/catalog"
		// Anonymous-trial mint is intentionally unauthenticated (Gap 1 of
		// AGENT_DRIVEN_PRIME_ONBOARDING.md): an agent calls it to get a trial
		// key WITHOUT a prior signed-in session. Exact-match only — sibling
		// /api/v1/agents/claim and /api/v1/agents/register stay auth-gated, so
		// this must not become a HasPrefix("/api/v1/agents/").
		isPublicTrialMint := path == "/api/v1/agents/anonymous-trial"
		isPublicDesignPartnerApplication := path == "/api/v1/design-partners/applications" && c.Request.Method == http.MethodPost
		if path == pathHealth || path == pathLivez || path == pathMetrics || path == "/docs" || path == "/openapi" || path == "/api/v1/cluster/health" || isPublicStatusPath || isPublicCatalog || isPublicTrialMint || isPublicDesignPartnerApplication || strings.HasPrefix(path, "/api/v1/webhooks/") || isPublicAuthPath || strings.HasPrefix(path, "/api/v1/onboard/") || strings.HasPrefix(path, "/api/v1/demo/") || strings.HasPrefix(path, "/api/v1/admin/") || strings.HasPrefix(path, "/x402/") {
			c.Next()
			return
		}

		token, err := ExtractToken(c)
		if err != nil {
			c.JSON(401, gin.H{"error": "unauthorized", "message": err.Error()})
			c.Abort()
			return
		}

		// ask_-prefixed tokens are API keys issued by Core; validate by delegating
		// to Core's /api/v1/auth/me. Everything else is parsed as a local JWT.
		var claims *Claims
		if strings.HasPrefix(token, "ask_") {
			claims, err = authClient.ValidateAPIKey(c.Request.Context(), token)
		} else {
			claims, err = authClient.ValidateToken(token)
		}
		if err != nil {
			c.JSON(401, gin.H{"error": "unauthorized", "message": err.Error()})
			c.Abort()
			return
		}

		// Create auth context
		authCtx := &AuthContext{
			UserID:   claims.UserID,
			Username: claims.Username,
			TenantID: claims.TenantID,
			Role:     claims.Role,
			IsAPIKey: claims.IsAPIKey,
		}

		// Store in context
		c.Set("auth", authCtx)
		c.Set("auth_role", authCtx.Role)          // Separate key for cross-package access
		c.Set("auth_user_id", authCtx.UserID)     // Separate key for cross-package access
		c.Set("auth_tenant_id", authCtx.TenantID) // Separate key for cross-package access
		// Stash the full validated claims so the ViewAsWriteRefusal middleware can
		// read view_as/act_as without re-parsing the token (AuthContext deliberately
		// stays minimal). Only set for locally-validated JWTs; ask_ API keys can't
		// carry view_as so their claims (synthesized from Core /me) are irrelevant
		// to the refusal — but we set them anyway for uniformity.
		c.Set("auth_claims", claims)
		// Also set the short "tenant_id" key the x402 middleware chain
		// (middleware.go, quota_gate.go, autopay.go) reads from. Without
		// this, every x402 path sees an empty tenant_id and the tier gate
		// / quota check / autopay wallet lookup all no-op. Pre-existing
		// bug surfaced during Phase C smoke testing.
		c.Set("tenant_id", authCtx.TenantID)
		c.Next()
	}
}

// RequirePermission returns a middleware that checks for a specific permission
func RequirePermission(perm entities.Permission) gin.HandlerFunc {
	return func(c *gin.Context) {
		authCtx, exists := c.Get("auth")
		if !exists {
			c.JSON(401, gin.H{"error": "unauthorized", "message": "authentication required"})
			c.Abort()
			return
		}

		auth, ok := authCtx.(*AuthContext)
		if !ok {
			c.JSON(500, gin.H{"error": "internal error", "message": "invalid auth context"})
			c.Abort()
			return
		}
		if !RoleHasPermission(auth.Role, perm) {
			c.JSON(403, gin.H{
				"error":   "forbidden",
				"message": fmt.Sprintf("permission denied: %s required", perm),
			})
			c.Abort()
			return
		}

		c.Next()
	}
}

// RequireAdmin returns a middleware that requires admin role
func RequireAdmin() gin.HandlerFunc {
	return RequirePermission(entities.PermissionAdmin)
}

// GetAuthContext retrieves the auth context from the gin context
func GetAuthContext(c *gin.Context) (*AuthContext, error) {
	authCtx, exists := c.Get("auth")
	if !exists {
		return nil, errors.New("no auth context found")
	}

	auth, ok := authCtx.(*AuthContext)
	if !ok {
		return nil, errors.New("invalid auth context type")
	}

	return auth, nil
}

// oauthUserResult holds the result of finding or creating an OAuth user.
type oauthUserResult struct {
	Token     string
	UserID    string
	TenantID  string
	IsNewUser bool
}

// findOrCreateOAuthUser creates or finds a tenant for the OAuth user and signs a JWT.
// inviteToken is optional; if non-empty and valid, the user joins the existing tenant referenced
// by the invite instead of creating a new isolated one.
func (cp *ControlPlane) findOrCreateOAuthUser(provider, providerID, email, name, inviteToken string) (*oauthUserResult, error) {
	// Generate a deterministic user ID from provider info
	userID := fmt.Sprintf("oauth:%s:%s", provider, providerID)

	// If an invite token is present, resolve the tenant from the invite rather than
	// creating a new isolated one for this user.
	if inviteToken != "" {
		invite, resolveErr := cp.resolveInvite(context.Background(), inviteToken, email)
		if resolveErr != nil {
			// Invalid or expired invite — fall through to normal registration.
			log.Printf("warn: invite token %q invalid or expired: %v", inviteToken, resolveErr)
		} else {
			// Valid invite found — provision the user into the existing tenant.
			tenantID := invite.TenantID

			// Record the user as a member of this team.
			if addErr := cp.AddTeamMember(context.Background(), tenantID, TeamMember{
				UserID:   userID,
				Email:    email,
				Name:     name,
				Role:     invite.Role,
				JoinedAt: time.Now().UTC().Format(time.RFC3339),
			}, invite.InvitedBy); addErr != nil {
				log.Printf("warn: AddTeamMember failed for tenant %s user %s: %v", logsafe.String(tenantID), logsafe.String(userID), addErr)
			}

			// Mark invite as accepted by deleting it from config.
			if delErr := cp.coreClient.DeleteConfig(context.Background(), teamInviteConfigKey(inviteToken)); delErr != nil {
				log.Printf("warn: failed to delete invite token %s: %v", logsafe.String(inviteToken), delErr)
			}

			// Sign JWT for this user in the existing tenant.
			now := time.Now()
			claims := &Claims{
				UserID:   userID,
				Username: name,
				Email:    email,
				Name:     name,
				TenantID: tenantID,
				Role:     roleForEmail(email),
				Provider: provider,
				StandardClaims: jwt.StandardClaims{
					ExpiresAt: now.Add(7 * 24 * time.Hour).Unix(),
					IssuedAt:  now.Unix(),
					Issuer:    "allsource",
					Subject:   userID,
				},
			}
			token := jwt.NewWithClaims(jwt.SigningMethodHS256, claims)
			tokenString, signErr := token.SignedString([]byte(cp.authClient.jwtSecret))
			if signErr != nil {
				return nil, fmt.Errorf("failed to sign JWT: %w", signErr)
			}
			return &oauthUserResult{
				Token:     tokenString,
				UserID:    userID,
				TenantID:  tenantID,
				IsNewUser: true,
			}, nil
		}
	}

	// Derive tenant ID from email (same as demo flow). The tenant ID must satisfy
	// Core's validation (alphanumeric, hyphens, underscores) — the raw userID
	// "oauth:email:UUID" contains colons and would be rejected.
	tenantSlug := entities.TenantSlug(email)
	tenantID := tenantSlug

	// New self-service signups (OAuth + email register, which funnels here) start a
	// 14-day trial, NOT a permanent free tier (prompt 048). Shared tier + expiry
	// stamp so this can't drift from onboard / agent-register; the scheduler's
	// trial-expiry sweep suspends the tenant once trial_expires_at passes.
	// NOTE: Core force-maps "already exists" to 4xx below, so a RETURNING user's
	// login does not overwrite their stored subscription — this trial stamp only
	// lands on the genuinely-new tenant Core actually creates (201).
	trialSubscription, _ := usecases.TrialSubscriptionMetadata(time.Now())

	tenantBody := map[string]interface{}{
		"id":   tenantID,
		"name": name,
		"slug": tenantSlug,
		"metadata": map[string]interface{}{
			"subscription": trialSubscription,
			"quota":        usecases.TrialQuotaMetadata(),
		},
	}

	// Create or get tenant from Core
	resp, err := cp.client.R().
		SetBody(tenantBody).
		Post("/api/v1/tenants")

	if err != nil {
		return nil, fmt.Errorf("core service unavailable: %w", err)
	}

	isNewUser := false

	switch {
	case resp.StatusCode() == 201 || resp.StatusCode() == 200:
		var result map[string]interface{}
		if parseErr := json.Unmarshal(resp.Body(), &result); parseErr == nil {
			if id, ok := result["id"].(string); ok && id != "" {
				tenantID = id
			}
			isNewUser = resp.StatusCode() == 201
		}
	case resp.StatusCode() == 409:
		// Tenant already exists — returning user, keep the email-derived tenantID
		isNewUser = false
	case resp.StatusCode() == 400 && strings.Contains(string(resp.Body()), "already exists"):
		// Core currently force-maps repository errors to HTTP 400, so a duplicate
		// tenant on a returning-user login comes back as 400 instead of 409.
		// Treat "already exists" as success so returning users can log in.
		isNewUser = false
	default:
		log.Printf("Core tenant creation failed: HTTP %d, body: %s", resp.StatusCode(), string(resp.Body()))
		return nil, fmt.Errorf("failed to create tenant (HTTP %d): %s", resp.StatusCode(), string(resp.Body()))
	}

	// Sign JWT
	now := time.Now()
	claims := &Claims{
		UserID:   userID,
		Username: name,
		Email:    email,
		Name:     name,
		TenantID: tenantID,
		Role:     roleForEmail(email),
		Provider: provider,
		StandardClaims: jwt.StandardClaims{
			ExpiresAt: now.Add(7 * 24 * time.Hour).Unix(),
			IssuedAt:  now.Unix(),
			Issuer:    "allsource",
			Subject:   userID,
		},
	}

	token := jwt.NewWithClaims(jwt.SigningMethodHS256, claims)
	tokenString, err := token.SignedString([]byte(cp.authClient.jwtSecret))
	if err != nil {
		return nil, fmt.Errorf("failed to sign JWT: %w", err)
	}

	return &oauthUserResult{
		Token:     tokenString,
		UserID:    userID,
		TenantID:  tenantID,
		IsNewUser: isNewUser,
	}, nil
}

// SessionHandler returns current user identity and workspace information.
// GET /api/v1/auth/session
func (cp *ControlPlane) SessionHandler(c *gin.Context) {
	authCtx, exists := c.Get("auth")
	if !exists {
		c.JSON(401, gin.H{"error": "unauthorized"})
		return
	}
	auth, ok := authCtx.(*AuthContext)
	if !ok {
		c.JSON(401, gin.H{"error": "unauthorized"})
		return
	}

	c.JSON(200, gin.H{
		"user": gin.H{
			"id":       auth.UserID,
			"username": auth.Username,
			"email":    "",
		},
		"tenant_id": auth.TenantID,
	})
}

// LoginRequest represents a login request from the frontend.
// Accepts "email" (preferred) or "username" for the identifier field.
type LoginRequest struct {
	Email    string `json:"email"`
	Username string `json:"username"`
	Password string `json:"password" binding:"required"`
}

// RegisterRequest represents a registration request from the frontend.
type RegisterRequest struct {
	Name              string `json:"name" binding:"required"`
	Email             string `json:"email" binding:"required"`
	Password          string `json:"password" binding:"required"`
	TurnstileResponse string `json:"cf_turnstile_response"` // Optional: Cloudflare Turnstile token from the web signup form
}

// LoginHandler handles user login.
// Proxies credential verification to Core, then signs a CP-issued JWT on success.
// TODO: CP should own credential storage (PostgreSQL) instead of proxying to Core.
func (cp *ControlPlane) LoginHandler(c *gin.Context) {
	var req LoginRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(400, gin.H{"error": "invalid request", "message": err.Error()})
		return
	}

	// Frontend sends "email", Core expects "username" — translate
	username := req.Email
	if username == "" {
		username = req.Username
	}
	if username == "" {
		c.JSON(400, gin.H{"error": "invalid request", "message": "email or username is required"})
		return
	}

	// Verify credentials against Core
	resp, err := cp.client.R().
		SetBody(map[string]string{
			"username": username,
			"password": req.Password,
		}).
		Post("/api/v1/auth/login")

	if err != nil {
		c.JSON(503, gin.H{"error": "service_unavailable", "message": "authentication service is temporarily unavailable"})
		return
	}

	if resp.StatusCode() != 200 {
		// Forward Core's error response (wrong password, user not found, etc.)
		var errResult map[string]interface{}
		if json.Unmarshal(resp.Body(), &errResult) == nil {
			c.JSON(resp.StatusCode(), errResult)
			return
		}
		c.JSON(401, gin.H{"error": "invalid_credentials", "message": "Invalid email or password"})
		return
	}

	// Parse Core's response to extract user info
	var coreResp struct {
		Token string `json:"token"`
		User  struct {
			ID       string `json:"id"`
			Username string `json:"username"`
			Email    string `json:"email"`
			Role     string `json:"role"`
			TenantID string `json:"tenant_id"`
		} `json:"user"`
	}
	if err := json.Unmarshal(resp.Body(), &coreResp); err != nil {
		c.JSON(500, gin.H{"error": "internal_error", "message": "failed to parse authentication response"})
		return
	}

	// Sign a CP-issued JWT (consistent with OAuth flow)
	userID := coreResp.User.ID
	if userID == "" {
		userID = fmt.Sprintf("email:%s", username)
	}
	tenantID := coreResp.User.TenantID
	if tenantID == "" {
		tenantID = userID
	}
	displayName := coreResp.User.Username
	if displayName == "" {
		displayName = username
	}

	// Check if tenant is a demo tenant by looking it up from Core
	isDemo := false
	if cp.coreClient != nil {
		if tenantResp, err := cp.coreClient.GetTenant(c.Request.Context(), tenantID); err == nil {
			isDemo = tenantResp.IsDemo
		}
	}

	now := time.Now()
	claims := &Claims{
		UserID:   userID,
		Username: displayName,
		Email:    username,
		Name:     displayName,
		TenantID: tenantID,
		Role:     roleForEmail(username),
		Provider: "email",
		IsDemo:   isDemo,
		StandardClaims: jwt.StandardClaims{
			ExpiresAt: now.Add(7 * 24 * time.Hour).Unix(),
			IssuedAt:  now.Unix(),
			Issuer:    "allsource",
			Subject:   userID,
		},
	}

	token := jwt.NewWithClaims(jwt.SigningMethodHS256, claims)
	tokenString, err := token.SignedString([]byte(cp.authClient.jwtSecret))
	if err != nil {
		c.JSON(500, gin.H{"error": "internal_error", "message": "failed to create session"})
		return
	}

	c.JSON(200, gin.H{
		"token":    tokenString,
		"new_user": false,
		"user": gin.H{
			"id":        userID,
			"email":     username,
			"name":      displayName,
			"tenant_id": tenantID,
		},
	})
}

// RegisterHandler handles user registration.
// Proxies credential creation to Core, creates a tenant, and signs a CP-issued JWT.
// TODO: CP should own credential storage (PostgreSQL) instead of proxying to Core.
func (cp *ControlPlane) RegisterHandler(c *gin.Context) {
	var req RegisterRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(400, gin.H{"error": "invalid request", "message": err.Error()})
		return
	}

	// Verify Cloudflare Turnstile token if configured. Browser signup sends
	// cf_turnstile_response; CLI/agent callers omit it. If the verifier is
	// nil (TURNSTILE_SECRET_KEY unset), this is a no-op.
	if cp.turnstile != nil && req.TurnstileResponse != "" {
		if err := cp.turnstile.Verify(c.Request.Context(), req.TurnstileResponse, clientIP(c)); err != nil {
			c.JSON(403, gin.H{"error": "captcha_failed", "message": "Turnstile verification failed. Please try again."})
			return
		}
	}

	// Register credentials in Core (Core handles password hashing)
	resp, err := cp.client.R().
		SetBody(map[string]string{
			"username": req.Email,
			"email":    req.Email,
			"password": req.Password,
		}).
		Post("/api/v1/auth/register")

	if err != nil {
		c.JSON(503, gin.H{"error": "service_unavailable", "message": "registration service is temporarily unavailable"})
		return
	}

	if resp.StatusCode() != 201 && resp.StatusCode() != 200 {
		body := string(resp.Body())
		// Core returns 400/409 with "already exists" for duplicate usernames
		if resp.StatusCode() == 409 || strings.Contains(strings.ToLower(body), "already exists") {
			c.JSON(409, gin.H{"error": "email_exists", "message": "An account with this email already exists"})
			return
		}
		var errResult map[string]interface{}
		if json.Unmarshal(resp.Body(), &errResult) == nil {
			c.JSON(resp.StatusCode(), errResult)
			return
		}
		c.JSON(resp.StatusCode(), gin.H{"error": "registration_failed", "message": "Registration failed"})
		return
	}

	// Parse Core's response for user ID
	var coreResp struct {
		UserID   string `json:"user_id"`
		Username string `json:"username"`
		Email    string `json:"email"`
		TenantID string `json:"tenant_id"`
	}
	if err := json.Unmarshal(resp.Body(), &coreResp); err != nil {
		coreResp.UserID = fmt.Sprintf("email:%s", req.Email)
	}

	userID := coreResp.UserID
	if userID == "" {
		userID = fmt.Sprintf("email:%s", req.Email)
	}

	// Create tenant (same pattern as OAuth)
	result, err := cp.findOrCreateOAuthUser("email", userID, req.Email, req.Name, "")
	if err != nil {
		log.Printf("Register: findOrCreateOAuthUser failed for %s: %v", logsafe.String(req.Email), err)
		c.JSON(500, gin.H{"error": "internal_error", "message": "failed to create account", "detail": err.Error()})
		return
	}

	c.JSON(201, gin.H{
		"token":    result.Token,
		"new_user": true,
		"user": gin.H{
			"id":        result.UserID,
			"email":     req.Email,
			"name":      req.Name,
			"tenant_id": result.TenantID,
		},
	})
}
