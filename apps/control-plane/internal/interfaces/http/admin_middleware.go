package http

import (
	"errors"
	"fmt"
	"net/http"
	"strings"
	"time"

	"github.com/dgrijalva/jwt-go"
	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/domain/entities"
)

// AdminClaims represents JWT claims for admin authentication.
// Mirrors the main Claims struct but lives in the http package to avoid circular imports.
type AdminClaims struct {
	UserID         string        `json:"sub"`
	Username       string        `json:"username"`
	Email          string        `json:"email,omitempty"`
	Name           string        `json:"name,omitempty"`
	TenantID       string        `json:"tenant_id"`
	Role           entities.Role `json:"role"`
	Provider       string        `json:"provider,omitempty"`
	IsAPIKey       bool          `json:"is_api_key,omitempty"`
	CredentialKind string        `json:"credential_kind,omitempty"`
	IsDemo         bool          `json:"is_demo,omitempty"`
	// ViewAs/ActAs mirror the main Claims fields so a view_as token is fully
	// decoded here too. A view_as token carries role:readonly, so it is ALWAYS
	// rejected by the admin-role check below before reaching any admin handler —
	// these fields exist for parity and defense in depth, not because an admin
	// route ever sees a view_as token.
	ViewAs bool   `json:"view_as,omitempty"`
	ActAs  string `json:"act_as,omitempty"`
	jwt.StandardClaims
}

// AdminAuthContext holds authentication information for an admin request.
type AdminAuthContext struct {
	UserID   string
	Username string
	Email    string
	TenantID string
	Role     entities.Role
}

// AdminAuthMiddleware returns a Gin middleware that validates a JWT token
// and requires the caller to have the "admin" role. It is designed to protect
// the /api/v1/admin/* route group.
//
// Returns 401 for missing or invalid tokens, 403 for non-admin roles.
func AdminAuthMiddleware(jwtSecret string) gin.HandlerFunc {
	return func(c *gin.Context) {
		// Extract token from Authorization header
		token, err := extractBearerToken(c)
		if err != nil {
			c.JSON(http.StatusUnauthorized, gin.H{
				"error":   "unauthorized",
				"message": err.Error(),
			})
			c.Abort()
			return
		}

		// Validate the JWT
		claims, err := validateAdminToken(token, jwtSecret)
		if err != nil {
			c.JSON(http.StatusUnauthorized, gin.H{
				"error":   "unauthorized",
				"message": "invalid or expired token",
			})
			c.Abort()
			return
		}

		if claims.IsAPIKey || claims.CredentialKind != "" {
			c.AbortWithStatusJSON(http.StatusForbidden, gin.H{"error": "forbidden", "message": "API credentials cannot access administrative routes"})
			return
		}

		if claims.Role != entities.RoleAdmin {
			c.JSON(http.StatusForbidden, gin.H{
				"error":   "forbidden",
				"message": fmt.Sprintf("admin role required, current role: %s", claims.Role),
			})
			c.Abort()
			return
		}

		// Store admin auth context for downstream handlers
		adminCtx := &AdminAuthContext{
			UserID:   claims.UserID,
			Username: claims.Username,
			Email:    claims.Email,
			TenantID: claims.TenantID,
			Role:     claims.Role,
		}
		c.Set("admin_auth", adminCtx)
		// Also set the standard auth keys for compatibility with existing handlers
		c.Set("auth_role", claims.Role)
		c.Set("auth_user_id", claims.UserID)

		c.Next()
	}
}

// extractBearerToken extracts the JWT from the Authorization header.
func extractBearerToken(c *gin.Context) (string, error) {
	authHeader := c.GetHeader("Authorization")
	if authHeader == "" {
		return "", errors.New("missing authorization header")
	}

	parts := strings.SplitN(authHeader, " ", 2)
	if len(parts) != 2 || parts[0] != "Bearer" {
		return "", errors.New("invalid authorization header format")
	}

	if parts[1] == "" {
		return "", errors.New("missing token")
	}

	return parts[1], nil
}

// validateAdminToken parses and validates a JWT token string.
func validateAdminToken(tokenString, jwtSecret string) (*AdminClaims, error) {
	token, err := jwt.ParseWithClaims(tokenString, &AdminClaims{}, func(token *jwt.Token) (interface{}, error) {
		if _, ok := token.Method.(*jwt.SigningMethodHMAC); !ok {
			return nil, fmt.Errorf("unexpected signing method: %v", token.Header["alg"])
		}
		return []byte(jwtSecret), nil
	})
	if err != nil {
		return nil, fmt.Errorf("failed to parse token: %w", err)
	}

	claims, ok := token.Claims.(*AdminClaims)
	if !ok || !token.Valid {
		return nil, errors.New("invalid token")
	}

	if claims.ExpiresAt <= time.Now().Unix() {
		return nil, errors.New("token expired")
	}

	return claims, nil
}

// GetAdminAuthContext retrieves the admin auth context from the gin context.
func GetAdminAuthContext(c *gin.Context) (*AdminAuthContext, error) {
	val, exists := c.Get("admin_auth")
	if !exists {
		return nil, errors.New("no admin auth context found")
	}

	ctx, ok := val.(*AdminAuthContext)
	if !ok {
		return nil, errors.New("invalid admin auth context type")
	}

	return ctx, nil
}
