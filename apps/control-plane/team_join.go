package main

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"time"

	"github.com/dgrijalva/jwt-go"
	"github.com/gin-gonic/gin"
)

// TeamJoinHandler accepts a code for the exact authenticated subject. It returns
// a replacement session to the product server, which sets its HttpOnly cookie.
// This membership operation is not a consequential-action approval credential.
func (cp *ControlPlane) TeamJoinHandler(c *gin.Context) {
	c.Header("Cache-Control", "no-store")
	actor := mustAuthContext(c)
	if actor == nil {
		return
	}
	value, _ := c.Get("auth_claims")
	claims, ok := value.(*Claims)
	if !ok || claims == nil || actor.IsAPIKey || actor.ViewAs || claims.IsDemo ||
		!actor.EmailVerified || actor.Email == "" || actor.Provider == "" {
		c.JSON(403, gin.H{"message": "Sign in with a verified email matching the invitation to join this workspace."})
		return
	}
	c.Request.Body = http.MaxBytesReader(c.Writer, c.Request.Body, 4096)
	decoder := json.NewDecoder(c.Request.Body)
	decoder.DisallowUnknownFields()
	var request struct {
		Token string `json:"token"`
	}
	if decoder.Decode(&request) != nil || decoder.Decode(&struct{}{}) != io.EOF || !teamInviteTokenPattern.MatchString(request.Token) {
		c.JSON(400, gin.H{"message": "Enter a valid invitation code."})
		return
	}
	ctx, cancel := context.WithTimeout(c.Request.Context(), 30*time.Second)
	defer cancel()
	tenant, _, err := cp.acceptTeamInvitation(ctx, request.Token, actor.UserID, actor.Email, actor.Username)
	if err != nil {
		respondTeamError(c, err)
		return
	}
	// Preserve identity, role and expiry. Switching teams must not extend a
	// session or turn a team administrator into a platform administrator.
	updated := *claims
	updated.TenantID = tenant
	token, err := jwt.NewWithClaims(jwt.SigningMethodHS256, &updated).SignedString([]byte(cp.authClient.jwtSecret))
	if err != nil {
		respondTeamError(c, errTeamUnavailable)
		return
	}
	c.JSON(200, gin.H{"token": token, "tenant_id": tenant})
}
