package main

import (
	"crypto/rand"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"log"

	"net/http"
	"net/url"
	"os"
	"strings"
	"time"

	"github.com/allsource/control-plane/internal/logsafe"

	"github.com/gin-gonic/gin"
	"github.com/go-resty/resty/v2"
)

// OAuth provider names.
const (
	providerGitHub = "github"
	providerGoogle = "google"
)

// OAuth provider configuration URLs.
const (
	githubAuthorizeURL = "https://github.com/login/oauth/authorize"
	githubTokenURL     = "https://github.com/login/oauth/access_token" //nolint:gosec // URL, not a credential
	githubUserURL      = "https://api.github.com/user"
	githubEmailsURL    = "https://api.github.com/user/emails"

	googleAuthorizeURL = "https://accounts.google.com/o/oauth2/v2/auth"
	googleTokenURL     = "https://oauth2.googleapis.com/token" //nolint:gosec // URL, not a credential
	googleUserInfoURL  = "https://www.googleapis.com/oauth2/v2/userinfo"

	oauthStateCookieName    = "oauth_state"
	oauthRedirectCookieName = "oauth_redirect_to"
	oauthInviteTokenCookie  = "oauth_invite_token" //nolint:gosec // this is a cookie name, not a credential
)

// oauthProviderConfig holds client credentials for an OAuth provider.
type oauthProviderConfig struct {
	ClientID     string
	ClientSecret string
}

// getOAuthConfig returns the OAuth config for a provider, or nil if unconfigured.
func getOAuthConfig(provider string) *oauthProviderConfig {
	switch provider {
	case providerGitHub:
		id := os.Getenv("GITHUB_CLIENT_ID")
		secret := os.Getenv("GITHUB_CLIENT_SECRET")
		if id == "" || secret == "" {
			return nil
		}
		return &oauthProviderConfig{ClientID: id, ClientSecret: secret}
	case providerGoogle:
		id := os.Getenv("GOOGLE_CLIENT_ID")
		secret := os.Getenv("GOOGLE_CLIENT_SECRET")
		if id == "" || secret == "" {
			return nil
		}
		return &oauthProviderConfig{ClientID: id, ClientSecret: secret}
	default:
		return nil
	}
}

func getFrontendURL() string {
	if u := os.Getenv("FRONTEND_URL"); u != "" {
		return u
	}
	return "http://localhost:3000"
}

// getAllowedFrontendURLs returns the list of frontend URLs allowed as OAuth redirect targets.
// Includes FRONTEND_URL (always allowed) plus any additional URLs from ALLOWED_FRONTEND_URLS.
func getAllowedFrontendURLs() []string {
	allowed := []string{getFrontendURL()}
	if extra := os.Getenv("ALLOWED_FRONTEND_URLS"); extra != "" {
		for _, u := range strings.Split(extra, ",") {
			u = strings.TrimSpace(u)
			if u != "" {
				allowed = append(allowed, strings.TrimRight(u, "/"))
			}
		}
	}
	return allowed
}

// allowedCORSOrigins returns the set of browser origins permitted to make
// CREDENTIALED cross-origin requests. It is derived from the same frontend
// allowlist used for OAuth redirect validation (getAllowedFrontendURLs), so the
// admin panel and the web app are added in exactly one place: the FRONTEND_URL /
// ALLOWED_FRONTEND_URLS env. Each entry is reduced to scheme://host for an exact
// origin match. Credentialed CORS must never reflect an arbitrary Origin — doing
// so would let any website make authenticated requests on a logged-in user's
// behalf — which is why this allowlist exists instead of echoing every Origin.
func allowedCORSOrigins() map[string]struct{} {
	set := make(map[string]struct{})
	for _, raw := range getAllowedFrontendURLs() {
		if p, err := url.Parse(raw); err == nil && p.Scheme != "" && p.Host != "" {
			set[p.Scheme+"://"+p.Host] = struct{}{}
		}
	}
	return set
}

// isAllowedRedirectURL checks if a redirect URL is in the allowlist.
// Prevents open redirect attacks by only allowing known frontend origins.
func isAllowedRedirectURL(redirectTo string) bool {
	parsed, err := url.Parse(redirectTo)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" {
		return false
	}
	origin := parsed.Scheme + "://" + parsed.Host
	for _, allowed := range getAllowedFrontendURLs() {
		allowedParsed, err := url.Parse(allowed)
		if err != nil {
			continue
		}
		allowedOrigin := allowedParsed.Scheme + "://" + allowedParsed.Host
		if origin == allowedOrigin {
			return true
		}
	}
	return false
}

// isSecureContext returns true if the deployment uses HTTPS.
// Derived from FRONTEND_URL scheme, which is more reliable than GIN_MODE
// when running behind a TLS-terminating reverse proxy.
func isSecureContext() bool {
	return strings.HasPrefix(getFrontendURL(), "https://")
}

// oauthCookieDomain returns the Domain attribute for the short-lived OAuth
// transient cookies (state, redirect_to, invite). The OAuth flow can be started
// from one subdomain (e.g. the admin app proxies the start from
// admin.all-source.xyz) while the provider callback lands on another
// (getOAuthCallbackBaseURL = FRONTEND_URL, e.g. www.all-source.xyz). Host-only
// cookies are then lost across that hop, so CSRF-state validation fails and the
// per-app redirect_to is dropped (the user lands on FRONTEND_URL instead of the
// app that started the flow). Setting the registrable parent domain via
// OAUTH_COOKIE_DOMAIN (e.g. ".all-source.xyz") shares them across every
// *.all-source.xyz subdomain so both survive. Empty (the default) keeps host-only
// cookies — correct for localhost and single-domain deployments.
func oauthCookieDomain() string {
	return os.Getenv("OAUTH_COOKIE_DOMAIN")
}

// getOAuthCallbackBaseURL returns the public base URL for OAuth callbacks.
// OAuth requests are proxied through the frontend (Next.js), so the callback
// URL must use FRONTEND_URL — that's the domain registered with Google/GitHub.
// Flow: Google → browser → frontend/api/v1/auth/oauth/:provider/callback → proxy → CP.
func getOAuthCallbackBaseURL() string {
	if u := os.Getenv("FRONTEND_URL"); u != "" {
		return strings.TrimRight(u, "/")
	}
	port := os.Getenv("PORT")
	if port == "" {
		port = DefaultPort
	}
	return fmt.Sprintf("http://localhost:%s", port)
}

// generateOAuthState creates a cryptographically random state parameter for CSRF protection.
func generateOAuthState() (string, error) {
	b := make([]byte, 32)
	if _, err := rand.Read(b); err != nil {
		return "", fmt.Errorf("failed to generate random state: %w", err)
	}
	return base64.URLEncoding.EncodeToString(b), nil
}

// OAuthAuthorize redirects the browser to the OAuth provider's authorization page.
// GET /api/v1/auth/oauth/:provider
func (cp *ControlPlane) OAuthAuthorize(c *gin.Context) {
	provider := c.Param("provider")
	frontendURL := getFrontendURL()

	// Only accept known providers
	if provider != providerGitHub && provider != providerGoogle {
		c.Redirect(http.StatusFound, frontendURL+"/login?error=auth_failed")
		return
	}

	cfg := getOAuthConfig(provider)
	if cfg == nil {
		log.Printf("[OAuth] Provider %s not configured", logsafe.String(provider))
		c.Redirect(http.StatusFound, frontendURL+"/login?error=auth_failed")
		return
	}

	// Generate CSRF state token
	state, err := generateOAuthState()
	if err != nil {
		log.Printf("[OAuth] Failed to generate state: %v", err)
		c.Redirect(http.StatusFound, frontendURL+"/login?error=auth_failed")
		return
	}

	// Store state in a short-lived, httpOnly, SameSite=Lax cookie
	secure := isSecureContext()
	c.SetSameSite(http.SameSiteLaxMode)
	c.SetCookie(oauthStateCookieName, state, 600, "/api/v1/auth/oauth/", oauthCookieDomain(), secure, true)

	// Store redirect_to in a cookie so the callback knows which app to redirect to.
	// If not provided or not in the allowlist, the callback falls back to FRONTEND_URL.
	if redirectTo := c.Query("redirect_to"); redirectTo != "" && isAllowedRedirectURL(redirectTo) {
		c.SetCookie(oauthRedirectCookieName, redirectTo, 600, "/api/v1/auth/oauth/", oauthCookieDomain(), secure, true)
	}

	// Store invite_token so the callback can assign the user to the invited tenant.
	if inviteToken := c.Query("invite_token"); inviteToken != "" {
		c.SetCookie(oauthInviteTokenCookie, inviteToken, 600, "/api/v1/auth/oauth/", oauthCookieDomain(), secure, true)
	}

	callbackURL := fmt.Sprintf("%s/api/v1/auth/oauth/%s/callback", getOAuthCallbackBaseURL(), provider)

	var authURL string
	switch provider {
	case providerGitHub:
		params := url.Values{
			"client_id":    {cfg.ClientID},
			"redirect_uri": {callbackURL},
			"scope":        {"user:email"},
			"state":        {state},
		}
		authURL = githubAuthorizeURL + "?" + params.Encode()

	case providerGoogle:
		params := url.Values{
			"client_id":     {cfg.ClientID},
			"redirect_uri":  {callbackURL},
			"response_type": {"code"},
			"scope":         {"openid email profile"},
			"access_type":   {"offline"},
			"prompt":        {"consent"},
			"state":         {state},
		}
		authURL = googleAuthorizeURL + "?" + params.Encode()
	}

	c.Redirect(http.StatusFound, authURL)
}

// OAuthCallback handles the OAuth provider callback, exchanges the code for a token,
// fetches user info, creates/finds the user, signs a JWT, and redirects to the frontend.
// GET /api/v1/auth/oauth/:provider/callback
func (cp *ControlPlane) OAuthCallback(c *gin.Context) {
	provider := c.Param("provider")
	frontendURL := getFrontendURL()

	// Only accept known providers
	if provider != providerGitHub && provider != providerGoogle {
		c.Redirect(http.StatusFound, frontendURL+"/login?error=auth_failed")
		return
	}

	// Handle provider errors
	if errParam := c.Query("error"); errParam != "" {
		log.Printf("[OAuth] Provider %s returned error: %s", logsafe.String(provider), logsafe.String(errParam))
		errorCode := "auth_failed"
		if errParam == "access_denied" {
			errorCode = "access_denied"
		}
		c.Redirect(http.StatusFound, fmt.Sprintf("%s/login?error=%s", frontendURL, errorCode))
		return
	}

	code := c.Query("code")
	if code == "" {
		c.Redirect(http.StatusFound, frontendURL+"/login?error=auth_failed")
		return
	}

	// Verify CSRF state parameter
	returnedState := c.Query("state")
	cookieState, err := c.Cookie(oauthStateCookieName)
	if err != nil || cookieState == "" || returnedState == "" || returnedState != cookieState {
		log.Printf("[OAuth] State mismatch for %s (cookie present: %t, param present: %t)",
			logsafe.String(provider), cookieState != "", returnedState != "")
		c.Redirect(http.StatusFound, frontendURL+"/login?error=auth_failed")
		return
	}

	// Clear the state cookie
	c.SetSameSite(http.SameSiteLaxMode)
	c.SetCookie(oauthStateCookieName, "", -1, "/api/v1/auth/oauth/", oauthCookieDomain(), isSecureContext(), true)

	// Read and clear the redirect_to cookie — determines which frontend app to redirect to
	redirectTarget := frontendURL
	if redirectTo, err := c.Cookie(oauthRedirectCookieName); err == nil && redirectTo != "" {
		if isAllowedRedirectURL(redirectTo) {
			redirectTarget = strings.TrimRight(redirectTo, "/")
		}
	}
	c.SetCookie(oauthRedirectCookieName, "", -1, "/api/v1/auth/oauth/", oauthCookieDomain(), isSecureContext(), true)

	// Read and clear the invite_token cookie (empty string means no invite).
	inviteToken, err := c.Cookie(oauthInviteTokenCookie)
	if err != nil {
		inviteToken = ""
	}
	c.SetCookie(oauthInviteTokenCookie, "", -1, "/api/v1/auth/oauth/", oauthCookieDomain(), isSecureContext(), true)

	cfg := getOAuthConfig(provider)
	if cfg == nil {
		c.Redirect(http.StatusFound, redirectTarget+"/login?error=auth_failed")
		return
	}

	callbackURL := fmt.Sprintf("%s/api/v1/auth/oauth/%s/callback", getOAuthCallbackBaseURL(), provider)

	// Exchange code for provider access token
	// Use a plain HTTP client for external OAuth calls — cp.client has a service
	// JWT as its default Bearer token which would override the provider's token.
	oauthClient := resty.New().SetTimeout(10 * time.Second)

	providerToken, err := exchangeCode(oauthClient, provider, code, cfg, callbackURL)
	if err != nil {
		log.Printf("[OAuth] Code exchange failed for %s: %v", provider, err)
		c.Redirect(http.StatusFound, redirectTarget+"/login?error=auth_failed")
		return
	}

	// Fetch user info from provider
	userInfo, err := fetchUserInfo(oauthClient, provider, providerToken)
	if err != nil {
		log.Printf("[OAuth] User info fetch failed for %s: %v", provider, err)
		c.Redirect(http.StatusFound, redirectTarget+"/login?error=auth_failed")
		return
	}

	// Create or find user + sign JWT (pass invite token so the user joins the right tenant).
	result, err := cp.completeOAuthSignIn(provider, userInfo, inviteToken)
	if err != nil {
		log.Printf("[OAuth] User creation/JWT signing failed: %v", err)
		c.Redirect(http.StatusFound, redirectTarget+"/login?error=auth_failed")
		return
	}

	// TODO: The JWT in the query string is exposed in browser history, server logs, and Referer headers.
	// Replace with a short-lived authorization code that the callback exchanges server-side for the JWT.
	// This requires CP to maintain a temporary code store (e.g., in-memory with TTL or Redis).
	redirectURL := fmt.Sprintf("%s/api/auth/callback?token=%s&new_user=%t",
		redirectTarget, url.QueryEscape(result.Token), result.IsNewUser)
	c.Redirect(http.StatusFound, redirectURL)
}

// providerUserInfo holds user information fetched from an OAuth provider.
type providerUserInfo struct {
	ProviderID    string
	Email         string
	Name          string
	EmailVerified bool
}

// stringFromMap safely extracts a string value from a map, returning "" if missing or wrong type.
func stringFromMap(m map[string]interface{}, key string) string {
	v, _ := m[key].(string) //nolint:errcheck // type assertion ok-pattern; zero value is fine
	return v
}

// boolFromMap safely extracts a bool value from a map, returning false if missing or wrong type.
func boolFromMap(m map[string]interface{}, key string) bool {
	v, _ := m[key].(bool) //nolint:errcheck // type assertion ok-pattern; zero value is fine
	return v
}

// exchangeCode exchanges an authorization code for an access token.
func exchangeCode(client *resty.Client, provider, code string, cfg *oauthProviderConfig, redirectURI string) (string, error) {
	switch provider {
	case providerGitHub:
		resp, err := client.R().
			SetHeader("Content-Type", "application/json").
			SetHeader("Accept", "application/json").
			SetBody(map[string]string{
				"client_id":     cfg.ClientID,
				"client_secret": cfg.ClientSecret,
				"code":          code,
			}).
			Post(githubTokenURL)
		if err != nil {
			return "", fmt.Errorf("github token request failed: %w", err)
		}
		if resp.StatusCode() != http.StatusOK {
			return "", fmt.Errorf("github token endpoint returned status %d", resp.StatusCode())
		}

		var result map[string]interface{}
		if err := json.Unmarshal(resp.Body(), &result); err != nil {
			return "", fmt.Errorf("failed to parse github token response: %w", err)
		}
		if errMsg, ok := result["error"].(string); ok {
			desc := stringFromMap(result, "error_description")
			return "", fmt.Errorf("github token error: %s: %s", errMsg, desc)
		}
		token, ok := result["access_token"].(string)
		if !ok || token == "" {
			return "", fmt.Errorf("no access_token in github token response")
		}
		return token, nil

	case providerGoogle:
		resp, err := client.R().
			SetHeader("Content-Type", "application/x-www-form-urlencoded").
			SetFormData(map[string]string{
				"code":          code,
				"client_id":     cfg.ClientID,
				"client_secret": cfg.ClientSecret,
				"redirect_uri":  redirectURI,
				"grant_type":    "authorization_code",
			}).
			Post(googleTokenURL)
		if err != nil {
			return "", fmt.Errorf("google token request failed: %w", err)
		}
		if resp.StatusCode() != http.StatusOK {
			// Parse error description if available, but don't dump the full body
			var errResp map[string]interface{}
			if json.Unmarshal(resp.Body(), &errResp) == nil {
				if errMsg, ok := errResp["error"].(string); ok {
					desc := stringFromMap(errResp, "error_description")
					return "", fmt.Errorf("google token error (HTTP %d): %s: %s", resp.StatusCode(), errMsg, desc)
				}
			}
			return "", fmt.Errorf("google token endpoint returned status %d", resp.StatusCode())
		}

		var result map[string]interface{}
		if err := json.Unmarshal(resp.Body(), &result); err != nil {
			return "", fmt.Errorf("failed to parse google token response: %w", err)
		}
		token, ok := result["access_token"].(string)
		if !ok || token == "" {
			return "", fmt.Errorf("no access_token in google token response")
		}
		return token, nil

	default:
		return "", fmt.Errorf("unknown provider: %s", provider)
	}
}

// fetchUserInfo fetches user info from the OAuth provider using the access token.
func fetchUserInfo(client *resty.Client, provider, accessToken string) (*providerUserInfo, error) {
	switch provider {
	case providerGitHub:
		return fetchGitHubUserInfo(client, accessToken)
	case providerGoogle:
		return fetchGoogleUserInfo(client, accessToken)
	default:
		return nil, fmt.Errorf("unknown provider: %s", provider)
	}
}

func fetchGitHubUserInfo(client *resty.Client, token string) (*providerUserInfo, error) {
	resp, err := client.R().
		SetHeader("Authorization", "Bearer "+token).
		SetHeader("Accept", "application/json").
		Get(githubUserURL)
	if err != nil {
		return nil, fmt.Errorf("github user request failed: %w", err)
	}
	if resp.StatusCode() != http.StatusOK {
		return nil, fmt.Errorf("github user endpoint returned status %d", resp.StatusCode())
	}

	var user map[string]interface{}
	if err := json.Unmarshal(resp.Body(), &user); err != nil {
		return nil, fmt.Errorf("failed to parse github user response: %w", err)
	}

	providerID := fmt.Sprintf("%v", user["id"])
	name := stringFromMap(user, "name")
	if name == "" {
		name = stringFromMap(user, "login")
	}

	// Public profile text does not prove control of an invitation's email.
	email, err := fetchGitHubPrimaryEmail(client, token)
	if err != nil {
		return nil, fmt.Errorf("failed to get github email: %w", err)
	}

	return &providerUserInfo{
		ProviderID:    providerID,
		Email:         email,
		Name:          name,
		EmailVerified: true,
	}, nil
}

func fetchGitHubPrimaryEmail(client *resty.Client, token string) (string, error) {
	resp, err := client.R().
		SetHeader("Authorization", "Bearer "+token).
		SetHeader("Accept", "application/json").
		Get(githubEmailsURL)
	if err != nil {
		return "", fmt.Errorf("github emails request failed: %w", err)
	}
	if resp.StatusCode() != http.StatusOK {
		return "", fmt.Errorf("github emails endpoint returned status %d", resp.StatusCode())
	}

	var emails []map[string]interface{}
	if err := json.Unmarshal(resp.Body(), &emails); err != nil {
		return "", fmt.Errorf("failed to parse github emails response: %w", err)
	}

	// Find primary verified email
	for _, e := range emails {
		if boolFromMap(e, "primary") && boolFromMap(e, "verified") {
			if addr, ok := e["email"].(string); ok {
				return addr, nil
			}
		}
	}
	// Fall back to any verified email
	for _, e := range emails {
		if boolFromMap(e, "verified") {
			if addr, ok := e["email"].(string); ok {
				return addr, nil
			}
		}
	}
	return "", fmt.Errorf("no verified email found in github account")
}

func fetchGoogleUserInfo(client *resty.Client, token string) (*providerUserInfo, error) {
	resp, err := client.R().
		SetHeader("Authorization", "Bearer "+token).
		Get(googleUserInfoURL)
	if err != nil {
		return nil, fmt.Errorf("google userinfo request failed: %w", err)
	}
	if resp.StatusCode() != http.StatusOK {
		return nil, fmt.Errorf("google userinfo endpoint returned status %d", resp.StatusCode())
	}

	var user map[string]interface{}
	if err := json.Unmarshal(resp.Body(), &user); err != nil {
		return nil, fmt.Errorf("failed to parse google userinfo response: %w", err)
	}

	providerID := stringFromMap(user, "id")
	email := stringFromMap(user, "email")
	name := stringFromMap(user, "name")
	if name == "" {
		name = email
	}

	if providerID == "" || email == "" {
		return nil, fmt.Errorf("missing id or email in google response")
	}

	return &providerUserInfo{
		ProviderID:    providerID,
		Email:         email,
		Name:          name,
		EmailVerified: boolFromMap(user, "verified_email"),
	}, nil
}
