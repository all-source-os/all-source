package http

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/dgrijalva/jwt-go"
	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/domain/entities"
)

const testJWTSecret = "test-admin-secret"

// signTestToken creates a signed JWT for testing purposes.
func signTestToken(t *testing.T, claims *AdminClaims) string {
	t.Helper()
	token := jwt.NewWithClaims(jwt.SigningMethodHS256, claims)
	tokenString, err := token.SignedString([]byte(testJWTSecret))
	if err != nil {
		t.Fatalf("failed to sign test token: %v", err)
	}
	return tokenString
}

func setupAdminRouter() *gin.Engine {
	gin.SetMode(gin.TestMode)
	r := gin.New()

	admin := r.Group("/api/v1/admin")
	admin.Use(AdminAuthMiddleware(testJWTSecret))
	admin.GET("/health", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{"status": "ok"})
	})

	return r
}

func TestAdminAuthMiddleware_RejectsAPICredentials(t *testing.T) {
	router := setupAdminRouter()
	for _, kind := range []string{"", "managed_api_key", "service_delegation"} {
		claims := &AdminClaims{UserID: "synthetic-api-key", TenantID: "system", Role: entities.RoleAdmin,
			IsAPIKey: true, CredentialKind: kind, StandardClaims: jwt.StandardClaims{
				ExpiresAt: time.Now().Add(time.Hour).Unix(), IssuedAt: time.Now().Unix(), Issuer: "allsource"}}
		req := httptest.NewRequestWithContext(t.Context(), http.MethodGet, "/api/v1/admin/health", http.NoBody)
		req.Header.Set("Authorization", "Bearer "+signTestToken(t, claims))
		response := httptest.NewRecorder()
		router.ServeHTTP(response, req)
		if response.Code != http.StatusForbidden {
			t.Fatalf("API credential %q reached admin: %d", kind, response.Code)
		}
	}
}

func TestAdminAuthMiddleware_ValidAdminJWT(t *testing.T) {
	router := setupAdminRouter()

	claims := &AdminClaims{
		UserID:   "admin-user-1",
		Username: "admin",
		TenantID: "system",
		Role:     entities.RoleAdmin,
		StandardClaims: jwt.StandardClaims{
			ExpiresAt: time.Now().Add(time.Hour).Unix(),
			IssuedAt:  time.Now().Unix(),
			Issuer:    "allsource",
		},
	}
	token := signTestToken(t, claims)

	req := httptest.NewRequestWithContext(context.Background(), http.MethodGet, "/api/v1/admin/health", http.NoBody)
	req.Header.Set("Authorization", "Bearer "+token)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("expected status 200, got %d (body: %s)", w.Code, w.Body.String())
	}
}

func TestAdminAuthMiddleware_NonAdminJWT_Returns403(t *testing.T) {
	router := setupAdminRouter()

	roles := []entities.Role{
		entities.RoleDeveloper,
		entities.RoleReadOnly,
		entities.RoleServiceAccount,
	}

	for _, role := range roles {
		t.Run("role_"+string(role), func(t *testing.T) {
			claims := &AdminClaims{
				UserID:   "user-123",
				Username: "regularuser",
				TenantID: "tenant-1",
				Role:     role,
				StandardClaims: jwt.StandardClaims{
					ExpiresAt: time.Now().Add(time.Hour).Unix(),
					IssuedAt:  time.Now().Unix(),
					Issuer:    "allsource",
				},
			}
			token := signTestToken(t, claims)

			req := httptest.NewRequestWithContext(context.Background(), http.MethodGet, "/api/v1/admin/health", http.NoBody)
			req.Header.Set("Authorization", "Bearer "+token)
			w := httptest.NewRecorder()

			router.ServeHTTP(w, req)

			if w.Code != http.StatusForbidden {
				t.Errorf("expected status 403 for role %s, got %d (body: %s)", role, w.Code, w.Body.String())
			}
		})
	}
}

func TestAdminAuthMiddleware_MissingJWT_Returns401(t *testing.T) {
	router := setupAdminRouter()

	req := httptest.NewRequestWithContext(context.Background(), http.MethodGet, "/api/v1/admin/health", http.NoBody)
	// No Authorization header
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusUnauthorized {
		t.Errorf("expected status 401, got %d (body: %s)", w.Code, w.Body.String())
	}
}

func TestAdminAuthMiddleware_InvalidJWT_Returns401(t *testing.T) {
	router := setupAdminRouter()

	req := httptest.NewRequestWithContext(context.Background(), http.MethodGet, "/api/v1/admin/health", http.NoBody)
	req.Header.Set("Authorization", "Bearer invalid-token-string")
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusUnauthorized {
		t.Errorf("expected status 401, got %d (body: %s)", w.Code, w.Body.String())
	}
}

func TestAdminAuthMiddleware_ExpiredJWT_Returns401(t *testing.T) {
	router := setupAdminRouter()

	claims := &AdminClaims{
		UserID:   "admin-user-1",
		Username: "admin",
		TenantID: "system",
		Role:     entities.RoleAdmin,
		StandardClaims: jwt.StandardClaims{
			ExpiresAt: time.Now().Add(-time.Hour).Unix(), // expired
			IssuedAt:  time.Now().Add(-2 * time.Hour).Unix(),
			Issuer:    "allsource",
		},
	}
	token := signTestToken(t, claims)

	req := httptest.NewRequestWithContext(context.Background(), http.MethodGet, "/api/v1/admin/health", http.NoBody)
	req.Header.Set("Authorization", "Bearer "+token)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusUnauthorized {
		t.Errorf("expected status 401 for expired token, got %d (body: %s)", w.Code, w.Body.String())
	}
}

func TestAdminAuthMiddleware_WrongSecret_Returns401(t *testing.T) {
	router := setupAdminRouter()

	claims := &AdminClaims{
		UserID:   "admin-user-1",
		Username: "admin",
		TenantID: "system",
		Role:     entities.RoleAdmin,
		StandardClaims: jwt.StandardClaims{
			ExpiresAt: time.Now().Add(time.Hour).Unix(),
			IssuedAt:  time.Now().Unix(),
			Issuer:    "allsource",
		},
	}
	// Sign with a different secret
	token := jwt.NewWithClaims(jwt.SigningMethodHS256, claims)
	tokenString, err := token.SignedString([]byte("wrong-secret"))
	if err != nil {
		t.Fatalf("failed to sign token: %v", err)
	}

	req := httptest.NewRequestWithContext(context.Background(), http.MethodGet, "/api/v1/admin/health", http.NoBody)
	req.Header.Set("Authorization", "Bearer "+tokenString)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusUnauthorized {
		t.Errorf("expected status 401 for wrong secret, got %d (body: %s)", w.Code, w.Body.String())
	}
}

func TestAdminAuthMiddleware_MalformedAuthHeader_Returns401(t *testing.T) {
	router := setupAdminRouter()

	tests := []struct {
		name   string
		header string
	}{
		{"no_bearer_prefix", "Token abc123"},
		{"empty_bearer", "Bearer "},
		{"just_bearer", "Bearer"},
		{"basic_auth", "Basic dXNlcjpwYXNz"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			req := httptest.NewRequestWithContext(context.Background(), http.MethodGet, "/api/v1/admin/health", http.NoBody)
			req.Header.Set("Authorization", tt.header)
			w := httptest.NewRecorder()

			router.ServeHTTP(w, req)

			if w.Code != http.StatusUnauthorized {
				t.Errorf("expected status 401 for %s, got %d (body: %s)", tt.name, w.Code, w.Body.String())
			}
		})
	}
}

func TestGetAdminAuthContext_AfterMiddleware(t *testing.T) {
	gin.SetMode(gin.TestMode)
	r := gin.New()

	var capturedCtx *AdminAuthContext
	var capturedErr error

	admin := r.Group("/api/v1/admin")
	admin.Use(AdminAuthMiddleware(testJWTSecret))
	admin.GET("/check", func(c *gin.Context) {
		capturedCtx, capturedErr = GetAdminAuthContext(c)
		c.JSON(http.StatusOK, gin.H{"ok": true})
	})

	claims := &AdminClaims{
		UserID:   "admin-42",
		Username: "superadmin",
		TenantID: "system",
		Role:     entities.RoleAdmin,
		StandardClaims: jwt.StandardClaims{
			ExpiresAt: time.Now().Add(time.Hour).Unix(),
			IssuedAt:  time.Now().Unix(),
			Issuer:    "allsource",
		},
	}
	token := signTestToken(t, claims)

	req := httptest.NewRequestWithContext(context.Background(), http.MethodGet, "/api/v1/admin/check", http.NoBody)
	req.Header.Set("Authorization", "Bearer "+token)
	w := httptest.NewRecorder()

	r.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d", w.Code)
	}
	if capturedErr != nil {
		t.Fatalf("GetAdminAuthContext returned error: %v", capturedErr)
	}
	if capturedCtx == nil {
		t.Fatal("GetAdminAuthContext returned nil context")
	}
	if capturedCtx.UserID != "admin-42" {
		t.Errorf("expected UserID admin-42, got %s", capturedCtx.UserID)
	}
	if capturedCtx.Username != "superadmin" {
		t.Errorf("expected Username superadmin, got %s", capturedCtx.Username)
	}
	if capturedCtx.Role != entities.RoleAdmin {
		t.Errorf("expected Role admin, got %s", capturedCtx.Role)
	}
}
