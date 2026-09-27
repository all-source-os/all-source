package main

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"os"
	"regexp"
	"strings"
	"time"

	"github.com/dgrijalva/jwt-go"
	"github.com/gin-gonic/gin"

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
			ID            string `json:"id"`
			Email         string `json:"email"`
			Name          string `json:"name"`
			EmailVerified bool   `json:"emailVerified"`
		} `json:"user"`
	}
	if err := json.NewDecoder(io.LimitReader(resp.Body, 1<<20)).Decode(&result); err != nil || result.Token == "" || !authUserIDPattern.MatchString(result.User.ID) || result.User.Email == "" {
		c.JSON(502, gin.H{"message": "Invalid authentication response"})
		return
	}

	// Never associate an unverified email with an existing OAuth workspace or
	// ADMIN_EMAILS. Workspace identity comes from the authenticated user ID.
	ctx, cancel := context.WithTimeout(c.Request.Context(), 30*time.Second)
	defer cancel()
	tenantID, isNewUser, err := cp.emailWorkspace(ctx, result.User.ID, result.User.Email, result.User.Name)
	if err != nil {
		c.JSON(503, gin.H{"message": "Workspace setup unavailable. Your account is saved; try signing in again."})
		return
	}
	now := time.Now()
	claims := &Claims{
		UserID: result.User.ID, Username: result.User.Name, Email: result.User.Email,
		EmailVerified: result.User.EmailVerified,
		Name:          result.User.Name, TenantID: tenantID, Role: entities.RoleDeveloper, Provider: "email",
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
	c.JSON(responseStatus, gin.H{"token": token, "new_user": isNewUser, "user": gin.H{
		"id": result.User.ID, "email": result.User.Email, "name": result.User.Name, "tenant_id": tenantID,
	}})
}
