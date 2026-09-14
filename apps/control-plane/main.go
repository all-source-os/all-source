package main

import (
	"context"
	_ "embed"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"os"
	"os/signal"
	"strings"
	"syscall"
	"time"

	"github.com/dgrijalva/jwt-go"
	"github.com/gin-gonic/gin"
	"github.com/go-resty/resty/v2"
	"github.com/joho/godotenv"
	"github.com/prometheus/client_golang/prometheus/promhttp"
	"gopkg.in/yaml.v3"

	"github.com/allsource/control-plane/internal"
	"github.com/allsource/control-plane/internal/domain/entities"
	"github.com/allsource/control-plane/internal/infrastructure/clients"
	"github.com/allsource/control-plane/internal/infrastructure/x402"
	httphandlers "github.com/allsource/control-plane/internal/interfaces/http"
)

//go:embed docs/openapi.yaml
var openAPIYAML []byte

// Control plane configuration constants.
const (
	// Version is the current version of the control plane.
	Version = "0.24.0"
	// DefaultPort is the default port the control plane listens on.
	DefaultPort = "3901"
	// CoreServiceURL is the URL of the core event store service.
	CoreServiceURL = "http://localhost:3900"
)

// Connection pooling configuration for HTTP connections to Rust Core.
// These settings optimize connection reuse and reduce connection overhead.
const (
	// MaxIdleConns is the maximum number of idle connections across all hosts.
	MaxIdleConns = 100
	// MaxIdleConnsPerHost is the maximum number of idle connections per host.
	MaxIdleConnsPerHost = 100
	// IdleConnTimeout is the duration an idle connection remains open.
	IdleConnTimeout = 90 * time.Second
)

// NewPooledHTTPClient creates an http.Client with connection pooling configured.
// Preserves the Authorization header on redirects (Go's default client strips it).
func NewPooledHTTPClient() *http.Client {
	transport := &http.Transport{
		MaxIdleConns:        MaxIdleConns,
		MaxIdleConnsPerHost: MaxIdleConnsPerHost,
		IdleConnTimeout:     IdleConnTimeout,
	}
	return &http.Client{
		Transport: transport,
		CheckRedirect: func(req *http.Request, via []*http.Request) error {
			if len(via) >= 10 {
				return fmt.Errorf("stopped after 10 redirects")
			}
			// Preserve Authorization header across redirects (Go strips it by default).
			// Internal service-to-service: redirect targets are constrained to
			// configured backends so header forwarding is safe here.
			if auth := via[0].Header.Get("Authorization"); auth != "" {
				req.Header.Set("Authorization", auth) //nolint:gosec // G119
			}
			return nil
		},
	}
}

// ControlPlane is the main control plane service that manages the event store cluster.
type ControlPlane struct {
	client          *resty.Client
	router          *gin.Engine
	metrics         *ControlPlaneMetrics
	cacheMetrics    *CacheMetrics
	cache           *ResponseCache
	container       *internal.Container
	authClient      *AuthClient
	auditLogger     *AuditLogger
	policyEngine    *PolicyEngine
	coreClient      clients.CoreClient
	x402Handler     *x402.Handler
	x402Pricing     *x402.PricingConfig
	x402Facilitator x402.PaymentFacilitator

	// Data-plane delegation: Control Plane is the single public entry point.
	// After auth, forward writes to Core and reads to Query Service using the
	// service JWT (backends trust CP, not the caller). Tenant is injected from
	// the authenticated caller's identity before forwarding.
	delegation *delegationClient
	turnstile  *TurnstileVerifier

	// extractionGate hard-gates the hosted Hound doc-extraction proxy
	// (ProxyExtraction) on the tenant's per-tier extraction-token allowance.
	// Reads the meter + entitlement from Core tenant metadata; fails open.
	extractionGate *x402.CoreQuotaChecker

	// In-memory probe cache; the status endpoint reads from this so a Core
	// outage doesn't take the status page down with it.
	heartbeats *heartbeatEmitter
}

// NewControlPlane creates a new control plane instance with full middleware stack.
func NewControlPlane(ctx context.Context) (*ControlPlane, error) {
	// Determine core service URL
	coreURL := os.Getenv("CORE_SERVICE_URL")
	if coreURL == "" {
		coreURL = CoreServiceURL
	}

	// Create resty client with pooled HTTP transport for connection reuse
	httpClient := NewPooledHTTPClient()
	client := resty.NewWithClient(httpClient).
		SetTimeout(10 * time.Second).
		SetBaseURL(coreURL)

	// Set up Gin router
	if os.Getenv("GIN_MODE") == "release" {
		gin.SetMode(gin.ReleaseMode)
	}
	router := gin.New()
	router.Use(gin.Recovery())

	// Initialize metrics
	metrics := NewMetrics()
	cacheMetrics := NewCacheMetrics()

	// Initialize response cache
	cache := NewResponseCache(cacheMetrics)

	// Initialize auth client
	jwtSecret := os.Getenv("JWT_SECRET")
	if jwtSecret == "" {
		log.Println("JWT_SECRET not set, using default (INSECURE for production)")
		jwtSecret = "default-secret-change-in-production"
	}
	authClient := NewAuthClient(jwtSecret, coreURL)

	// Sign a service-to-service JWT so the resty client can authenticate with Core.
	// Uses a long-lived token with admin role — Core validates with the same shared secret.
	serviceClaims := &Claims{
		UserID:   "control-plane",
		Username: "control-plane",
		TenantID: "system",
		Role:     entities.RoleAdmin,
		StandardClaims: jwt.StandardClaims{
			ExpiresAt: time.Now().Add(365 * 24 * time.Hour).Unix(),
			IssuedAt:  time.Now().Unix(),
			Issuer:    "allsource",
			Subject:   "control-plane",
		},
	}
	serviceToken, err := jwt.NewWithClaims(jwt.SigningMethodHS256, serviceClaims).SignedString([]byte(jwtSecret))
	if err != nil {
		return nil, fmt.Errorf("failed to sign service JWT: %w", err)
	}
	client.SetAuthToken(serviceToken)
	// coreURL comes from operator-controlled env config, not user input.
	log.Printf("Service JWT set for Core auth (token length: %d, core URL: %s)", len(serviceToken), coreURL) //nolint:gosec // G706

	// Initialize audit logger
	auditLogPath := os.Getenv("AUDIT_LOG_PATH")
	if auditLogPath == "" {
		auditLogPath = "audit.log"
	}
	auditLogger, err := NewAsyncAuditLogger(auditLogPath)
	if err != nil {
		return nil, fmt.Errorf("failed to initialize audit logger: %w", err)
	}

	// Initialize policy engine
	policyEngine := NewPolicyEngine()

	// Initialize typed CoreClient for Clean Architecture use cases.
	// Uses the same admin service JWT as cp.client so it can call Core's
	// Admin-protected endpoints (config store, API key creation, etc.).
	coreClient := clients.NewCoreClientWithJWT(coreURL, serviceToken)

	// Initialize LemonSqueezy client (optional — billing features require it)
	var lsClient clients.LemonSqueezyClient
	if os.Getenv("LEMON_SQUEEZY_API_KEY") != "" {
		var lsErr error
		lsClient, lsErr = clients.NewLemonSqueezyClient()
		if lsErr != nil {
			log.Printf("LemonSqueezy client initialization failed: %v (billing endpoints will be unavailable)", lsErr)
		}
	}

	// Initialize email client (optional — usage warning emails require it)
	var emailClient clients.EmailClient
	if os.Getenv("SMTP_HOST") != "" {
		var emailErr error
		emailClient, emailErr = clients.NewEmailClientFromEnv()
		if emailErr != nil {
			log.Printf("Email client initialization failed: %v (usage warning emails will be unavailable)", emailErr)
		}
	}

	// Initialize Coinbase CDP client (optional — enables x402 auto-pay for wallet-less agents)
	var cdpClient clients.CDPClient
	if os.Getenv("COINBASE_CDP_KEY_NAME") != "" {
		var cdpErr error
		cdpClient, cdpErr = clients.NewCoinbaseCDPClient()
		if cdpErr != nil {
			log.Printf("Coinbase CDP client init failed: %v (x402 auto-pay disabled)", cdpErr)
		}
	}

	// Query Service base URL (no trailing slash). Used by (a) the metrics
	// passthrough (re-fetches /api/admin/metrics/* + /api/cluster/members for the
	// admin /monitoring page) and (b) deriving the /health URL below. Falls back
	// to the Fly internal default. This is the SAME base the delegation client
	// resolves at queryURL further down — kept in sync via QUERY_SERVICE_URL.
	queryServiceBase := os.Getenv("QUERY_SERVICE_URL")
	if queryServiceBase == "" {
		queryServiceBase = "http://allsource-query.internal:3902"
	}
	queryServiceBase = strings.TrimRight(queryServiceBase, "/")

	// Query Service /health URL for the fleet-recovery edition-trap detector.
	// The edition lives on QS, not CP (edition.ex); we HTTP-probe it. Honor the
	// same QUERY_HEALTH_URL override the heartbeat probes use, else derive from
	// the QS base above.
	queryHealthURL := os.Getenv("QUERY_HEALTH_URL")
	if queryHealthURL == "" {
		queryHealthURL = queryServiceBase + "/health"
	}

	// Initialize Clean Architecture container (Core-backed repos, no PostgreSQL)
	containerCfg := internal.ContainerConfig{
		CoreClient:      coreClient,
		LSClient:        lsClient,
		EmailClient:     emailClient,
		KeySigner:       authClient.SignAPIKey,
		CDPClient:       cdpClient,
		JWTSecret:       jwtSecret,
		QueryHealthURL:  queryHealthURL,
		QueryServiceURL: queryServiceBase,
		// The admin metrics passthrough authenticates to the QS with the CP's
		// own service JWT (the same long-lived admin/system token cp.client uses
		// for Core) — the admin's cookie never leaves the BFF→CP hop. A pooled
		// client reuses connections to the QS.
		ServiceToken:      serviceToken,
		MetricsHTTPClient: NewPooledHTTPClient(),
		// Read-only view-as impersonation (ADMIN_TENANT_POWER_TOOL §5 / Phase 7).
		// The signer mints a DISTINCT readonly+view_as ~15m token off AuthClient —
		// the signing key never leaves AuthClient (we pass the method value, not
		// the secret).
		ViewAsSigner: authClient.SignViewAsJWT,
	}
	container := internal.NewContainerWithConfig(containerCfg)

	// Loud boot-time billing config self-check: turn silent misconfig (variant
	// map gaps, bad webhook secret) into visible logs (t-3ca5d7). Never fatal —
	// logs only, so a config warn can't take prod down.
	logBillingConfigCheck(container)

	// The x402 included-allowance counter is per-instance and exact only on a
	// single CP machine. If the operator scales out, make the residual-overshoot
	// bound LOUD instead of silent (t-d7aabc).
	warnX402MultiInstance()

	// Initialize tracing
	otelEndpoint := os.Getenv("OTEL_ENDPOINT")
	tracingEnabled := otelEndpoint != ""
	tracingShutdown, err := InitTracing(TracingConfig{
		Enabled:      tracingEnabled,
		OTLPEndpoint: otelEndpoint,
		SamplingRate: 1.0,
	})
	if err != nil {
		log.Printf("Failed to initialize tracing: %v", err)
	}
	// tracingShutdown will be called during graceful shutdown
	_ = tracingShutdown

	// Initialize x402 payment facilitator (optional — requires X402_ENABLED env var)
	x402Pricing, err := x402.LoadPricingConfig()
	if err != nil {
		log.Printf("x402 pricing config load failed: %v (x402 payments will be disabled)", err)
		x402Pricing = &x402.PricingConfig{}
	}
	// Default to Coinbase's hosted facilitator — no private keys or blockchain node required.
	// Override X402_FACILITATOR_URL to point at a self-hosted facilitator.
	facilitatorURL := os.Getenv("X402_FACILITATOR_URL")
	if facilitatorURL == "" {
		facilitatorURL = x402.CoinbaseX402FacilitatorURL
	}
	var x402Facilitator x402.PaymentFacilitator = x402.NewRemoteFacilitator(facilitatorURL)
	x402Handler := x402.NewHandler(x402Facilitator, x402Pricing)

	// Data-plane delegation targets. All default to the Fly internal names
	// when unset (production wiring). Core handles event writes + query,
	// Query Service is reserved for future QS-specific reads, Prime handles
	// graph/vector/recall via /api/v1/prime/*.
	queryURL := os.Getenv("QUERY_SERVICE_URL")
	if queryURL == "" {
		queryURL = "http://allsource-query.internal:3902"
	}
	primeURL := os.Getenv("PRIME_SERVICE_URL")
	if primeURL == "" {
		primeURL = "http://allsource-prime.internal:3905"
	}
	delegation, err := newDelegationClient(coreURL, queryURL, primeURL, authClient.SignDelegationJWT, NewPooledHTTPClient())
	if err != nil {
		return nil, fmt.Errorf("init delegation client: %w", err)
	}

	// Regional Core registration (Step 2 of REGIONAL_INDEPENDENCE.md).
	// Reads CORE_SERVICE_URL_<REGION> for each region in the entity
	// allowlist. Unset → skip; the delegation client's coreForTenant
	// fast path returns the default Core for every tenant when the
	// regional map is empty. Once any regional URL is set, writes
	// route by tenant home_region.
	regionalEnabled := false
	for _, region := range []string{"iad", "lhr", "ord", "fra", "syd"} {
		envKey := "CORE_SERVICE_URL_" + strings.ToUpper(region)
		if u := os.Getenv(envKey); u != "" {
			if err := delegation.addRegionalCore(region, u); err != nil {
				return nil, fmt.Errorf("register regional core %s: %w", region, err)
			}
			regionalEnabled = true
			log.Printf("Regional Core registered: %s → %s", region, u) //nolint:gosec // G706: region from hardcoded allowlist, u from operator-controlled env var — no taint source
		}
	}
	if regionalEnabled {
		// Wire the home-region resolver to the tenant repository.
		// Cold lookup per write request — caching is a follow-up if
		// the latency cost shows up in practice.
		delegation.setHomeRegionResolver(func(tenantID string) string {
			t, err := container.TenantRepo.FindByID(tenantID)
			if err != nil || t == nil {
				return ""
			}
			return t.EffectiveHomeRegion()
		})
	}

	cp := &ControlPlane{
		client:          client,
		router:          router,
		metrics:         metrics,
		cacheMetrics:    cacheMetrics,
		cache:           cache,
		container:       container,
		authClient:      authClient,
		auditLogger:     auditLogger,
		policyEngine:    policyEngine,
		coreClient:      coreClient,
		x402Handler:     x402Handler,
		x402Pricing:     x402Pricing,
		x402Facilitator: x402Facilitator,
		delegation:      delegation,
		turnstile:       NewTurnstileVerifier(),
		extractionGate:  x402.NewCoreQuotaChecker(coreClient),
	}

	cp.setupMiddleware()
	cp.setupRoutes()

	// Start background operation scheduler
	cp.container.Scheduler.Start(ctx)

	// Start the event-sourced heartbeat emitter. Probes each backend every
	// heartbeatInterval, records the result in the emitter's in-memory cache
	// (read directly by /api/v1/status/services), and best-effort writes a
	// service.heartbeat event to Core for history.
	cp.heartbeats = newHeartbeatEmitter(coreClient, nil, probesFromEnv(os.Getenv))
	go cp.heartbeats.run(ctx, heartbeatInterval)

	return cp, nil
}

// corsMiddleware applies CORS headers. For an Origin in the allowlist it echoes
// that Origin and grants credentials (so cookies / Authorization work cross-site
// for the web app and the admin panel); every other request gets a
// non-credentialed wildcard grant, so truly public endpoints still work from any
// site but NO site can make a credentialed request unless it is explicitly
// allowlisted. This is the security boundary: a credentialed CORS grant must
// never reflect an arbitrary Origin. Vary: Origin keeps shared caches correct.
func corsMiddleware(allowed map[string]struct{}) gin.HandlerFunc {
	return func(c *gin.Context) {
		origin := c.Request.Header.Get("Origin")
		if _, ok := allowed[origin]; ok && origin != "" {
			c.Writer.Header().Set("Access-Control-Allow-Origin", origin)
			c.Writer.Header().Set("Access-Control-Allow-Credentials", "true")
			c.Writer.Header().Add("Vary", "Origin")
		} else {
			// Public, non-credentialed access only. A browser cannot send cookies
			// or read a credentialed response under an "*" grant.
			c.Writer.Header().Set("Access-Control-Allow-Origin", "*")
		}
		c.Writer.Header().Set("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
		c.Writer.Header().Set("Access-Control-Allow-Headers", "Content-Type, Authorization")

		if c.Request.Method == "OPTIONS" {
			c.AbortWithStatus(204)
			return
		}

		c.Next()
	}
}

func (cp *ControlPlane) setupMiddleware() {
	// CORS — credentialed only for allowlisted frontend origins (web + admin),
	// derived from the FRONTEND_URL / ALLOWED_FRONTEND_URLS allowlist. See
	// corsMiddleware and docs/runbooks/CONTROL_PLANE_CORS.md.
	cp.router.Use(corsMiddleware(allowedCORSOrigins()))

	// Prometheus metrics middleware
	cp.router.Use(PrometheusMiddleware(cp.metrics))

	// Tracing middleware (all routes)
	cp.router.Use(TracingMiddleware(serviceName))

	// Audit logging middleware (before auth so we log all attempts)
	cp.router.Use(AuditMiddleware(cp.auditLogger))

	// Auth middleware (applied globally, skips /health and /metrics)
	cp.router.Use(AuthMiddleware(cp.authClient))

	// View-as write-refusal (ADMIN_TENANT_POWER_TOOL §5.2 layer 2). Runs DIRECTLY
	// after AuthMiddleware (which stashes the validated claims under "auth_claims")
	// so it covers the entire data-plane surface a read-only view_as token could be
	// presented to: any view_as token on a mutating method (POST/PUT/PATCH/DELETE)
	// is hard-refused with 403 AND alarmed as a durable Core event
	// (admin.viewas.write_refused), independent of the role already blocking writes.
	// The alarm is the auditor's RecordWriteRefused method value, so this stays a
	// no-op when Core is absent.
	var viewAsAlarm viewAsAlarmFunc
	if cp.container != nil && cp.container.ViewAsAuditor != nil {
		viewAsAlarm = cp.container.ViewAsAuditor.RecordWriteRefused
	}
	cp.router.Use(ViewAsWriteRefusal(viewAsAlarm))

	// Policy middleware (after auth, uses auth context)
	cp.router.Use(PolicyMiddleware(cp.policyEngine, cp.auditLogger))
}

func (cp *ControlPlane) setupRoutes() {
	// Public endpoints (no auth required — skipped by AuthMiddleware)
	cp.router.GET("/livez", cp.livezHandler) // liveness only — Fly machine check uses this
	cp.router.GET("/health", cp.healthHandler)
	cp.router.GET("/metrics", gin.WrapH(promhttp.Handler()))
	cp.router.GET("/docs", cp.docsHandler)
	cp.router.GET("/openapi", cp.openAPIHandler)

	// JSON status feed for the marketing site's /status page. That React
	// page (www.all-source.xyz/status) is the canonical user-facing status
	// UI; CP just exposes the data.
	cp.router.GET("/api/v1/status/services", cp.statusServicesHandler)

	// Public pricing catalog — list prices read live from LemonSqueezy (source
	// of truth for charged prices). Consumed by the marketing /pricing page and
	// the dashboard billing page so displayed prices never drift from what the
	// customer is actually charged. Auth skipped in AuthMiddleware (isPublicCatalog).
	cp.router.GET("/api/v1/billing/catalog", cp.container.BillingHandler.GetCatalog)

	// Authentication endpoints
	auth := cp.router.Group("/api/v1/auth")
	auth.POST("/login", cp.LoginHandler)
	auth.POST("/register", cp.RegisterHandler)
	auth.GET("/oauth/:provider", cp.OAuthAuthorize)
	auth.GET("/oauth/:provider/callback", cp.OAuthCallback)
	auth.GET("/session", cp.SessionHandler)

	// Onboarding endpoint (public, no auth required)
	// Rate-limited: 5 tenant creations per IP per hour to prevent bulk abuse.
	onboardLimiter := NewIPRateLimiter(5, time.Hour)
	onboard := cp.router.Group("/api/v1/onboard")
	onboard.Use(IPRateLimitMiddleware(onboardLimiter))
	onboard.POST("/start", cp.OnboardHandler)

	// Design-partner applications are public, spam-checked, body-limited, and
	// rate-limited before applicant PII reaches the private Core stream.
	designPartnerLimiter := NewIPRateLimiter(5, time.Hour)
	designPartners := cp.router.Group("/api/v1/design-partners")
	designPartners.Use(IPRateLimitMiddleware(designPartnerLimiter))
	designPartners.POST("/applications", MaxBodySize(16*1024), cp.DesignPartnerApplyHandler)

	// Agent registration (public, no auth required)
	agents := cp.router.Group("/api/v1/agents")
	agents.POST("/register", cp.AgentRegisterHandler)

	// Anonymous trial — stricter rate limit than the named-agent path
	// (3/hour/IP) because every successful call creates a persistent
	// tenant + API key with no caller identity to fall back on. Bead
	// t-072c — Gap 1 of docs/proposals/AGENT_DRIVEN_PRIME_ONBOARDING.md.
	trialLimiter := NewIPRateLimiter(3, time.Hour)
	agentsTrial := cp.router.Group("/api/v1/agents")
	agentsTrial.Use(IPRateLimitMiddleware(trialLimiter))
	agentsTrial.POST("/anonymous-trial", cp.AgentAnonymousTrialHandler)

	// Agent self-service endpoints (auth required)
	agentsSelf := cp.router.Group("/api/v1/agents/me")
	agentsSelf.GET("/payments", RequirePermission(entities.PermissionRead), cp.container.AgentHandler.GetPaymentHistory)

	// Trial-claim endpoint (auth required). Lets a signed-in user attach a
	// previously-minted anonymous trial tenant to their account via the
	// claim_token returned by /api/v1/agents/anonymous-trial. PermissionRead
	// is the gate — claim is housekeeping on the caller's own account, not
	// a write into someone else's data, and we want read-only API keys to
	// be able to perform it too. Bead t-e8b8.
	agentsAuthed := cp.router.Group("/api/v1/agents")
	agentsAuthed.POST("/claim", RequirePermission(entities.PermissionRead), cp.AgentClaimHandler)

	// x402 facilitator endpoints (public — called by payers to verify/settle payments)
	x402Routes := cp.router.Group("/x402")
	x402Routes.POST("/verify", cp.x402Handler.Verify)
	x402Routes.POST("/settle", cp.x402Handler.Settle)
	x402Routes.GET("/routes", cp.x402Handler.Routes)

	// Demo endpoint (public, no auth required)
	demo := cp.router.Group("/api/v1/demo")
	demo.POST("/start", cp.DemoStartHandler)

	// Protected API endpoints
	api := cp.router.Group("/api/v1")

	// x402 auto-pay middleware: intercepts quota-exceeded requests, auto-settles via CDP wallet
	// when the agent has one; falls back to a standard 402 when CDP is unavailable.
	if cp.x402Pricing != nil && cp.x402Pricing.Enabled {
		x402Logger := x402.NewEventLogger(cp.coreClient)
		api.Use(x402.AgentAutoPayMiddleware(
			cp.x402Facilitator,
			cp.x402Pricing,
			x402Logger,
			x402.NewCoreQuotaChecker(cp.coreClient),
			cp.container.WalletLookup,
			cp.container.CDPClient,
		))
	}

	// Reference x402 endpoint — minimal echo handler used to verify the
	// tier gate, 402 payment-required, and auto-pay flows end-to-end.
	// Sits on the `api` group so it inherits the AgentAutoPayMiddleware
	// above. Pricing lives in config/x402-pricing.json under the key
	// "POST /api/v1/agent-echo"; without that entry the route behaves
	// like any other authenticated API call.
	api.POST("/agent-echo", RequirePermission(entities.PermissionRead), cp.AgentEchoHandler)

	// Data-plane delegation. Writes delegate to Core, reads to Query Service.
	// Tenant is injected from the authenticated caller; backends see CP's
	// service JWT, never the caller's ask_ key. These sit under `api` so they
	// inherit AuthMiddleware + PolicyMiddleware + x402 auto-pay gating.
	// Event ingestion — max 256KB per single event, 1MB per batch.
	// Prevents storage abuse via oversized payloads.
	api.POST("/events", MaxBodySize(256*1024), RequirePermission(entities.PermissionWrite), cp.ProxyIngestSingle)
	api.POST("/events/batch", MaxBodySize(1024*1024), RequirePermission(entities.PermissionWrite), cp.ProxyIngestBatch)
	api.GET("/events/query", RequirePermission(entities.PermissionRead), cp.ProxyEventsQuery)

	// Tenant-scoped analytics surface (#230). Each of these forces an
	// auth-derived tenant_id before forwarding; Core's equivalents are global
	// without it, so they must never be proxied bare. Adding them here is what
	// lets the MCP connector's stats/state tools return exact numbers instead of
	// deriving approximations from a full event scan (#229).
	api.GET("/stats", RequirePermission(entities.PermissionRead), cp.ProxyStats)
	api.GET("/entities/:entity_id/state", RequirePermission(entities.PermissionRead), cp.ProxyEntityState)
	api.GET("/entities/:entity_id/snapshot", RequirePermission(entities.PermissionRead), cp.ProxyEntitySnapshot)

	// Prime catch-all: all /api/v1/prime/* methods (GET graph queries, POST
	// nodes/edges/vectors, recall, etc.) forward to the Prime service. Tenant
	// is passed as a query param; Prime's own namespace logic consumes it.
	api.Any("/prime/*path", cp.ProxyPrime)

	// Hosted Hound doc-extraction LLM proxy — the enforcement point for the
	// extraction-token HARD GATE (t-57fca0). Authenticates the tenant from its
	// ask_ key (PRIME_LLM_API_KEY), blocks with 402 when the per-tier extraction
	// allowance is spent, else forwards to AllSource's server-side LLM. prime-mcp
	// points PRIME_LLM_ENDPOINT here for hosted extraction; BYO endpoints never
	// touch this route. 2MB cap guards against oversized chat-completion bodies.
	api.POST("/extraction/chat/completions", MaxBodySize(2*1024*1024), RequirePermission(entities.PermissionWrite), cp.ProxyExtraction)

	// Cluster management
	api.GET("/cluster/status", RequirePermission(entities.PermissionRead), cp.container.OperationsHandler.GetClusterStatus)
	api.GET("/cluster/health", cp.container.OperationsHandler.ClusterHealth) // public — auth skipped by AuthMiddleware
	api.GET("/metrics/json", cp.metricsHandler)

	// Core service health check
	api.GET("/health/core", cp.coreHealthHandler)

	// Operations (Clean Architecture handlers with per-route RBAC)
	operations := api.Group("/operations")
	operations.POST("/snapshots", RequirePermission(entities.PermissionAdmin), cp.container.OperationsHandler.CreateSnapshot)
	operations.GET("/snapshots", RequirePermission(entities.PermissionRead), cp.container.OperationsHandler.ListSnapshots)
	operations.POST("/compaction", RequirePermission(entities.PermissionAdmin), cp.container.OperationsHandler.TriggerCompaction)
	operations.GET("/compaction/stats", RequirePermission(entities.PermissionRead), cp.container.OperationsHandler.GetCompactionStats)
	operations.POST("/replay", RequirePermission(entities.PermissionAdmin), cp.container.OperationsHandler.StartReplay)
	operations.GET("/replay/:id", RequirePermission(entities.PermissionRead), cp.container.OperationsHandler.GetReplayProgress)
	operations.POST("/replay/:id/cancel", RequirePermission(entities.PermissionAdmin), cp.container.OperationsHandler.CancelReplay)
	operations.GET("/history", RequirePermission(entities.PermissionRead), cp.container.OperationsHandler.ListOperations)
	operations.POST("/backup", RequirePermission(entities.PermissionAdmin), cp.backupHandler)

	// Tenant management (Clean Architecture handlers with per-route RBAC)
	tenants := api.Group("/tenants")
	tenants.GET("", RequirePermission(entities.PermissionRead), cp.container.TenantHandler.List)
	tenants.GET("/:id", RequirePermission(entities.PermissionRead), cp.container.TenantHandler.Get)
	tenants.POST("", RequirePermission(entities.PermissionManageTenants), cp.container.TenantHandler.Create)
	tenants.PUT("/:id", RequirePermission(entities.PermissionManageTenants), cp.container.TenantHandler.Update)
	tenants.POST("/:id/suspend", RequireAdmin(), cp.container.TenantHandler.Suspend)
	tenants.POST("/:id/activate", RequireAdmin(), cp.container.TenantHandler.Activate)
	tenants.DELETE("/:id", RequireAdmin(), cp.container.TenantHandler.Delete)
	tenants.GET("/:id/stats", RequirePermission(entities.PermissionRead), cp.container.TenantHandler.Stats)

	// Team management (invite flow + member management)
	// GET /api/v1/teams/invite/:token is public (no auth — used before OAuth to preview the invite).
	cp.router.GET("/api/v1/teams/invite/:token", cp.GetInviteHandler)
	teams := api.Group("/teams")
	teams.POST("/invite", cp.InviteHandler)
	teams.GET("/members", RequirePermission(entities.PermissionRead), cp.ListMembersHandler)
	teams.PUT("/members/:id", cp.UpdateMemberRoleHandler)
	teams.DELETE("/members/:id", cp.DeleteMemberHandler)
	// Agent key management — provision and revoke Core API keys for agents
	teams.POST("/agent-keys", RequirePermission(entities.PermissionWrite), cp.CreateAgentKeyHandler)
	teams.GET("/agent-keys", RequirePermission(entities.PermissionRead), cp.ListAgentKeysHandler)
	teams.DELETE("/agent-keys/:name", RequirePermission(entities.PermissionWrite), cp.RevokeAgentKeyHandler)

	// User management (admin only)
	users := api.Group("/users")
	users.Use(RequireAdmin())
	users.GET("", cp.listUsersHandler)
	users.DELETE("/:id", cp.deleteUserHandler)

	// Policy management (Clean Architecture handlers with per-route RBAC)
	policies := api.Group("/policies")
	policies.POST("/evaluate", RequirePermission(entities.PermissionRead), cp.container.PolicyHandler.Evaluate)
	policies.GET("", RequirePermission(entities.PermissionRead), cp.container.PolicyHandler.List)
	policies.GET("/:id", RequirePermission(entities.PermissionRead), cp.container.PolicyHandler.Get)
	policies.POST("", RequireAdmin(), cp.container.PolicyHandler.Create)
	policies.PUT("/:id", RequireAdmin(), cp.container.PolicyHandler.Update)
	policies.DELETE("/:id", RequireAdmin(), cp.container.PolicyHandler.Delete)
	policies.POST("/:id/enable", RequireAdmin(), cp.container.PolicyHandler.Enable)
	policies.POST("/:id/disable", RequireAdmin(), cp.container.PolicyHandler.Disable)

	// Schema governance (Clean Architecture handlers with per-route RBAC)
	schemas := api.Group("/schemas")
	schemas.POST("", RequirePermission(entities.PermissionManageSchemas), cp.container.SchemaHandler.Register)
	schemas.GET("", RequirePermission(entities.PermissionRead), cp.container.SchemaHandler.List)
	schemas.POST("/validate", RequirePermission(entities.PermissionRead), cp.container.SchemaHandler.Validate)

	// Billing management (Clean Architecture handlers with per-route RBAC).
	// Self-serve checkout + portal are scoped to the caller's OWN tenant (the
	// handler overrides tenant_id from the auth context), so they only need
	// PermissionWrite — a tenant owner (RoleDeveloper) manages their own
	// subscription. ManageTenants is admin-only and would 403 every real user.
	billing := api.Group("/billing")
	billing.POST("/checkout", RequirePermission(entities.PermissionWrite), cp.container.BillingHandler.CreateCheckout)
	billing.POST("/change-plan", RequirePermission(entities.PermissionWrite), cp.container.BillingHandler.ChangePlan)
	billing.GET("/portal", RequirePermission(entities.PermissionWrite), cp.container.BillingHandler.GetPortal)
	billing.GET("/overage", RequirePermission(entities.PermissionRead), cp.container.BillingHandler.GetOverage)
	billing.POST("/overage/enable", RequirePermission(entities.PermissionManageTenants), cp.container.BillingHandler.EnableOverage)
	billing.POST("/overage/disable", RequirePermission(entities.PermissionManageTenants), cp.container.BillingHandler.DisableOverage)
	billing.GET("/projected-charges", RequirePermission(entities.PermissionRead), cp.container.BillingHandler.GetProjectedCharges)

	// Tenant-facing notices (ADMIN_TENANT_POWER_TOOL §4 Pillar C). These are
	// CALLER-scoped — the authenticated tenant's OWN in-app notices, read for the
	// web dashboard banner — and are intentionally NOT admin-gated. The tenant id
	// comes from the caller's JWT (auth_tenant_id), so a tenant only ever sees and
	// dismisses its own notices. PermissionRead is the self-serve gate (any tenant
	// member), exactly like the billing reads above.
	api.GET("/notices", RequirePermission(entities.PermissionRead), cp.container.CommsHandler.TenantNotices)
	api.POST("/notices/:id/dismiss", RequirePermission(entities.PermissionRead), cp.container.CommsHandler.DismissNotice)

	// Webhooks (public — no JWT auth, signature verification instead)
	api.POST("/webhooks/lemonsqueezy", cp.container.WebhookHandler.LemonSqueezy)
	// Resend inbound webhook (Svix-verified). Registered only when Resend is configured.
	if cp.container.ResendWebhookHandler != nil {
		api.POST("/webhooks/resend/inbound", cp.container.ResendWebhookHandler.Inbound)
		// Resend ENGAGEMENT webhook (delivered/opened/clicked/bounced/complained) →
		// engagement Core events for the comms-efficiency engine (prompt 050). Public
		// + Svix-verified like the inbound path; idempotent (replay = one event).
		api.POST("/webhooks/resend/events", cp.container.ResendWebhookHandler.Events)
	}
	// NOTE: the admin inbox routes (connections/addresses/messages/triage/draft/send)
	// live on the /api/v1/admin group below — they need AdminAuthMiddleware (the
	// scheme the admin app's session uses), like /tenants and /fleet, not the
	// api-group AuthMiddleware+RequirePermission (which 401s the admin session).

	// Audit trail (Clean Architecture handlers)
	api.GET("/audit", RequirePermission(entities.PermissionRead), cp.container.AuditHandler.Query)

	// Dynamic configuration (Clean Architecture handlers with per-route RBAC)
	config := api.Group("/config")
	config.GET("", RequirePermission(entities.PermissionRead), cp.container.ConfigHandler.List)
	config.GET("/:key", RequirePermission(entities.PermissionRead), cp.container.ConfigHandler.Get)
	config.POST("", RequireAdmin(), cp.container.ConfigHandler.Create)
	config.PUT("/:key", RequireAdmin(), cp.container.ConfigHandler.Update)
	config.DELETE("/:key", RequireAdmin(), cp.container.ConfigHandler.Delete)

	// Admin-only route group (self-contained JWT + admin role middleware)
	admin := cp.router.Group("/api/v1/admin")
	admin.Use(httphandlers.AdminAuthMiddleware(cp.authClient.jwtSecret))
	admin.GET("/health", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{
			"status":  "ok",
			"service": "allsource-control-plane-admin",
			"version": Version,
		})
	})
	admin.GET("/tenants", cp.container.AdminTenantHandler.ListTenants)
	// Read-only tenant-data analysis (prompt 046). Scans every tenant (or one via
	// ?tenant_id=, or one category via ?category=) and returns anomaly findings,
	// each deep-linking to an EXISTING guarded action (reap-demo, backfill-usage
	// reconcile, recovery/*, billing config-check). NEVER mutates — analysis only.
	// Static path registered before /tenants/:id so it isn't captured by the param.
	admin.GET("/tenants/analyze", cp.container.AdminTenantHandler.AnalyzeTenants)
	admin.POST("/tenants/bulk", cp.container.AdminTenantHandler.BulkAction)
	admin.GET("/tenants/:id", cp.container.AdminTenantHandler.GetDetail)
	admin.GET("/tenants/:id/usage", cp.container.AdminTenantHandler.GetUsage)
	admin.PUT("/tenants/:id/quotas", cp.container.AdminTenantHandler.UpdateQuotas)
	admin.POST("/tenants/:id/suspend", cp.container.AdminTenantHandler.SuspendTenant)
	admin.POST("/tenants/:id/unsuspend", cp.container.AdminTenantHandler.UnsuspendTenant)
	// Reap demo-litter: delete the tenants Core flagged is_demo (the side-effect
	// of the old status probe). Cohort, token-gated — ?dry_run=true previews the
	// matched list + count + a confirm_token; apply requires the echoed token.
	// Reuses the recovery guard + audit machinery (admin.recovery.reap_demo). The
	// prevention half is the DEMO_ENABLED gate on DemoStartHandler.
	admin.POST("/tenants/reap-demo", cp.container.RecoveryHandler.ReapDemo)

	// Fleet health (read) — the cross-tenant rollup + per-tenant assessment.
	// The health model lives once here; the admin UI (P2) and Elixir MCP (P3)
	// are thin consumers (CONTROL_PLANE_TENANT_HEALTH_RECOVERY.md §5).
	admin.GET("/fleet/health", cp.container.FleetHealthHandler.GetFleetHealth)
	admin.GET("/fleet/health/:id", cp.container.FleetHealthHandler.GetTenantHealth)

	// AI inbox management (043). AdminAuthMiddleware-gated like /tenants and /fleet
	// so the admin app's session authenticates.
	admin.GET("/inbox/connections", cp.container.InboxAdminHandler.ListConnections)
	// Register a receiving address (Resend: a connection is a verified address).
	admin.POST("/inbox/addresses", cp.container.InboxAdminHandler.AddAddress)
	admin.DELETE("/inbox/connections/:grant_id", cp.container.InboxAdminHandler.Disconnect)
	admin.GET("/inbox/messages", cp.container.InboxAdminHandler.Messages)
	admin.POST("/inbox/triage", cp.container.InboxAdminHandler.Triage)
	admin.POST("/inbox/draft", cp.container.InboxAdminHandler.Draft)
	// AI-grounded draft generation (045): thread + contact recall → Claude → body.
	admin.POST("/inbox/draft/generate", cp.container.InboxAdminHandler.GenerateDraft)
	admin.POST("/inbox/send", cp.container.InboxAdminHandler.Send)

	// Private design-partner application review. Status updates append events;
	// applicant fields never enter public or tenant-facing routes.
	admin.GET("/design-partners/applications", cp.DesignPartnerApplicationsHandler)
	admin.PUT("/design-partners/applications/:id/status", cp.DesignPartnerStatusHandler)

	// Recovery — diagnose (Safe, read-only) + guarded/destructive actions.
	// reactivate/suspend/edit_quotas are NOT re-implemented — they keep using the
	// existing /tenants/:id/{suspend,unsuspend,quotas} routes above. Only the new
	// capabilities get new endpoints. Every mutating route supports ?dry_run=true
	// and enforces its guards in the use-case layer.
	recovery := admin.Group("/recovery")
	recovery.GET("/diagnose/edition", cp.container.FleetHealthHandler.DiagnoseEdition)
	recovery.GET("/:id/diagnose-identity", cp.container.FleetHealthHandler.DiagnoseIdentity)
	recovery.POST("/:id/resync", cp.container.RecoveryHandler.Resync)
	recovery.POST("/:id/reconcile-subscription", cp.container.RecoveryHandler.ReconcileSubscription)
	recovery.POST("/:id/resolve-dunning", cp.container.RecoveryHandler.ResolveDunning)
	recovery.POST("/:id/rotate-keys", cp.container.RecoveryHandler.RotateKeys)
	recovery.POST("/:id/reprovision", cp.container.RecoveryHandler.Reprovision)
	recovery.POST("/:id/restore", cp.container.RecoveryHandler.Restore)
	recovery.POST("/batch", cp.container.RecoveryHandler.Batch)

	// Proactive comms — in-app notices, operator→tenant email, per-tenant support
	// notes (ADMIN_TENANT_POWER_TOOL §4 Pillar C / Phase 6 CP half). All inside the
	// /api/v1/admin group → inherit AdminAuthMiddleware (no new auth). Cohort
	// notice sends go through the recovery blast-radius guard (dry-run preview +
	// echoed confirm_token); email reuses the existing SMTP client; every action is
	// a durable Core event (admin.notice.*, admin.message.sent, admin.note.created)
	// under the admin-comms system tenant. Notes live under /tenants/:id/notes.
	admin.POST("/notices", cp.container.CommsHandler.CreateNotice)
	admin.GET("/notices", cp.container.CommsHandler.ListNotices)
	admin.POST("/messages", cp.container.CommsHandler.SendMessage)
	admin.POST("/tenants/:id/notes", cp.container.CommsHandler.AddNote)
	admin.GET("/tenants/:id/notes", cp.container.CommsHandler.ListNotes)

	// Proactive-comms EFFICIENCY (prompt 050) — read-only operator analytic: the
	// funnel (delivered→click→goal-in-window), causal lift vs holdout, time-to-goal,
	// and unsub/complaint cost per campaign/stage/variant/tier, with trial→paid as
	// the hero metric. Every figure is joined from Core events by a CP reconciler
	// (no parallel analytics stack). ?refresh=true forces a live recompute.
	admin.GET("/comms/efficiency", cp.container.CommsEfficiencyHandler.GetEfficiency)

	// Read-only "view as tenant" impersonation (ADMIN_TENANT_POWER_TOOL §5 / Phase 7
	// CP half). Inside the /api/v1/admin group → inherit AdminAuthMiddleware (admin
	// role, no new auth). The mint returns a token DISTINCT from the tenant's real
	// session (role:readonly, view_as:true, ~15m TTL) and writes admin.viewas.started
	// BEFORE returning it; /view-as/stop writes the paired admin.viewas.stopped on the
	// operator's Exit (the admin frame, prompt 041, calls both). Read-only is enforced
	// THREE ways: the readonly role, the global ViewAsWriteRefusal middleware (refuses
	// any view_as token on a mutating method + alarms admin.viewas.write_refused), and
	// the surface. NEVER touches the tenant's real session.
	admin.POST("/tenants/:id/view-as", cp.container.ViewAsHandler.Start)
	admin.POST("/tenants/:id/view-as/stop", cp.container.ViewAsHandler.Stop)

	// Platform metrics + cluster status passthrough (ADMIN_TENANT_POWER_TOOL §3
	// Gap 2). The admin /monitoring page hits these SAME-ORIGIN via the BFF
	// (Bearer attached); the CP re-fetches the Query Service metrics/cluster
	// endpoints with its OWN service credential and returns the QS shapes the
	// admin client (metrics-api.ts) consumes. QS-unreachable ⇒ a zeroed/empty
	// payload (HTTP 200), so the admin renders a zero state, never a 500. These
	// inherit AdminAuthMiddleware (admin role) like every other /api/v1/admin
	// route — no new auth.
	adminMetrics := admin.Group("/metrics")
	adminMetrics.GET("/summary", cp.container.MetricsHandler.Summary)
	adminMetrics.GET("/timeseries", cp.container.MetricsHandler.Timeseries)
	admin.GET("/cluster/members", cp.container.MetricsHandler.ClusterMembers)

	// Alert rules management
	admin.POST("/alerts", cp.container.AlertHandler.Create)
	admin.GET("/alerts", cp.container.AlertHandler.List)
	admin.PUT("/alerts/:id", cp.container.AlertHandler.Update)
	admin.DELETE("/alerts/:id", cp.container.AlertHandler.Delete)

	// SLO management
	admin.POST("/slos", cp.container.SLOHandler.Create)
	admin.GET("/slos", cp.container.SLOHandler.List)
	admin.DELETE("/slos/:id", cp.container.SLOHandler.Delete)

	// Security — IP allowlist/blocklist management
	admin.GET("/security/ip-rules", cp.container.IPRuleHandler.List)
	admin.POST("/security/ip-rules", cp.container.IPRuleHandler.Create)
	admin.DELETE("/security/ip-rules/:id", cp.container.IPRuleHandler.Delete)

	// Security — Token audit
	admin.GET("/security/token-audit", cp.container.TokenAuditHandler.Query)
	admin.GET("/security/token-audit/summary", cp.container.TokenAuditHandler.Summary)

	// Security — Suspicious activity detection
	admin.GET("/security/suspicious-activity", cp.container.SuspiciousActivityHandler.Detect)

	// Admin billing management
	adminBilling := admin.Group("/billing")
	adminBilling.GET("/invoices", cp.container.AdminBillingHandler.ListInvoices)
	adminBilling.GET("/revenue", cp.container.AdminBillingHandler.GetRevenue)
	adminBilling.POST("/refund", cp.container.AdminBillingHandler.ProcessRefund)
	adminBilling.GET("/dunning", cp.container.AdminBillingHandler.GetDunning)
	adminBilling.GET("/config-check", cp.container.BillingConfigHandler.ConfigCheck)
	adminBilling.POST("/migrate-early-adopters", cp.container.EarlyAdopterMigrationHandler.Run)
	// Reconcile a tenant's metered events_used from the real event count in Core
	// (for data ingested outside the metered QS write path). Body:
	// {"tenant_id":"...","dry_run":true|false}. Empty tenant_id = all active tenants.
	adminBilling.POST("/backfill-usage", cp.container.BackfillUsageHandler.Run)
}

// livezHandler is the liveness probe: did the CP process come up and is the
// router serving requests? It deliberately does NOT touch Core or any
// downstream — Fly's machine health check uses this so a Core outage doesn't
// cause Fly to kill+restart a perfectly healthy CP machine.
func (cp *ControlPlane) livezHandler(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"status":  "ok",
		"service": "allsource-control-plane",
		"version": Version,
	})
}

// healthHandler is the readiness probe: can CP actually serve user traffic
// end-to-end? Returns 503 when Core is unreachable so external load balancers
// and consumer health checks (issue #160) correctly observe the API as down
// rather than being misled by a "healthy" CP that can't reach its database.
func (cp *ControlPlane) healthHandler(c *gin.Context) {
	coreStatus := resourceUnknown
	if cp.coreClient != nil {
		_, err := cp.coreClient.HealthCheck(c.Request.Context())
		if err != nil {
			coreStatus = "unreachable"
		} else {
			coreStatus = statusHealthy
		}
	}

	overall := statusHealthy
	httpStatus := http.StatusOK
	if coreStatus != statusHealthy {
		overall = statusUnhealthy
		httpStatus = http.StatusServiceUnavailable
	}

	c.JSON(httpStatus, gin.H{
		"status":      overall,
		"service":     "allsource-control-plane",
		"version":     Version,
		"persistence": "core",
		"timestamp":   time.Now().UTC(),
		"core_status": coreStatus,
		"features": gin.H{
			"authentication": true,
			"audit_logging":  cp.auditLogger.enabled,
			"rbac":           true,
			"tracing":        os.Getenv("OTEL_ENDPOINT") != "",
		},
	})
}

func (cp *ControlPlane) coreHealthHandler(c *gin.Context) {
	start := time.Now()

	token, _ := ExtractToken(c) //nolint:errcheck

	resp, err := cp.client.R().
		SetHeader("Authorization", "Bearer "+token).
		Get("/health")

	duration := time.Since(start).Seconds()
	cp.metrics.CoreHealthCheckDuration.Observe(duration)

	if err != nil {
		cp.metrics.CoreHealthCheckTotal.WithLabelValues("error").Inc()
		c.JSON(http.StatusServiceUnavailable, gin.H{
			"status": statusUnhealthy,
			"error":  err.Error(),
		})
		return
	}

	var result map[string]any
	if err := json.Unmarshal(resp.Body(), &result); err != nil {
		cp.metrics.CoreHealthCheckTotal.WithLabelValues("error").Inc()
		c.JSON(http.StatusInternalServerError, gin.H{
			"status": "error",
			"error":  "failed to parse core response",
		})
		return
	}

	cp.metrics.CoreHealthCheckTotal.WithLabelValues("success").Inc()
	c.JSON(http.StatusOK, result)
}

func (cp *ControlPlane) metricsHandler(c *gin.Context) {
	// Check cache first
	if cached := cp.cache.Get(CacheKeyMetrics); cached != nil {
		c.JSON(http.StatusOK, cached.Data)
		return
	}

	token, _ := ExtractToken(c) //nolint:errcheck

	// Aggregate metrics from core
	resp, err := cp.client.R().
		SetHeader("Authorization", "Bearer "+token).
		Get("/api/v1/stats")

	if err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{
			"error": "failed to fetch metrics from core",
		})
		return
	}

	var stats map[string]any
	_ = json.Unmarshal(resp.Body(), &stats) //nolint:errcheck // best effort parsing

	response := gin.H{
		"metrics": gin.H{
			"event_store": stats,
			"control_plane": gin.H{
				"uptime_seconds": time.Since(startTime).Seconds(),
				"version":        Version,
			},
		},
		"timestamp": time.Now().UTC(),
	}

	// Cache the response
	cp.cache.Set(CacheKeyMetrics, response, MetricsCacheTTL)

	c.JSON(http.StatusOK, response)
}

func (cp *ControlPlane) backupHandler(c *gin.Context) {
	auth, _ := GetAuthContext(c) //nolint:errcheck
	token, _ := ExtractToken(c)  //nolint:errcheck

	// Proxy to core backup endpoint
	resp, err := cp.client.R().
		SetHeader("Authorization", "Bearer "+token).
		Post("/api/v1/backup")

	if err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{
			"error": "failed to initiate backup on core service",
		})
		return
	}

	backupID := fmt.Sprintf("backup-%d", time.Now().Unix())
	if auth != nil {
		cp.auditLogger.LogOperationEvent("backup_create", backupID, auth.UserID, "initiated")
	}

	var result map[string]any
	_ = json.Unmarshal(resp.Body(), &result) //nolint:errcheck // best effort parsing
	c.JSON(http.StatusOK, result)
}

// openAPIHandler serves the OpenAPI specification as JSON.
func (cp *ControlPlane) openAPIHandler(c *gin.Context) {
	var spec interface{}
	if err := yaml.Unmarshal(openAPIYAML, &spec); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to parse OpenAPI spec"})
		return
	}
	c.JSON(http.StatusOK, spec)
}

// docsHandler serves Swagger UI for interactive API documentation.
func (cp *ControlPlane) docsHandler(c *gin.Context) {
	html := `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Control Plane API Documentation</title>
  <link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css">
  <style>
    html { box-sizing: border-box; overflow-y: scroll; }
    *, *:before, *:after { box-sizing: inherit; }
    body { margin: 0; background: #fafafa; }
    .swagger-ui .topbar { display: none; }
    .swagger-ui .info { margin-top: 20px; }
    .swagger-ui .info .title { font-size: 36px; }
  </style>
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-standalone-preset.js"></script>
  <script>
    window.onload = function() {
      window.ui = SwaggerUIBundle({
        url: "/openapi",
        dom_id: '#swagger-ui',
        deepLinking: true,
        presets: [
          SwaggerUIBundle.presets.apis,
          SwaggerUIStandalonePreset
        ],
        plugins: [
          SwaggerUIBundle.plugins.DownloadUrl
        ],
        layout: "StandaloneLayout",
        persistAuthorization: true,
        displayRequestDuration: true,
        filter: true,
        tryItOutEnabled: true
      });
    };
  </script>
</body>
</html>`

	c.Data(http.StatusOK, "text/html; charset=utf-8", []byte(html))
}

// User handlers (proxied to core)
func (cp *ControlPlane) listUsersHandler(c *gin.Context) {
	cp.proxyToCoreAuth(c, "GET", "/api/v1/auth/users")
}

func (cp *ControlPlane) deleteUserHandler(c *gin.Context) {
	userID := c.Param("id")
	auth, _ := GetAuthContext(c) //nolint:errcheck

	token, _ := ExtractToken(c) //nolint:errcheck
	resp, err := cp.client.R().
		SetHeader("Authorization", "Bearer "+token).
		Delete("/api/v1/auth/users/" + userID)

	if err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{
			"error": "failed to delete user from core service",
		})
		return
	}

	if auth != nil {
		cp.auditLogger.LogAuthEvent("user_delete", userID, "", "", "user deleted by "+auth.Username)
	}
	c.JSON(resp.StatusCode(), gin.H{"message": "user deleted"})
}

// Helper: proxy request to core with auth
func (cp *ControlPlane) proxyToCoreAuth(c *gin.Context, method, path string) {
	token, _ := ExtractToken(c) //nolint:errcheck

	resp, err := cp.client.R().
		SetHeader("Authorization", "Bearer "+token).
		Execute(method, path)

	if err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{
			"error": "failed to communicate with core service",
		})
		return
	}

	var result map[string]any
	_ = json.Unmarshal(resp.Body(), &result) //nolint:errcheck // best effort parsing
	c.JSON(resp.StatusCode(), result)
}

// Shutdown gracefully shuts down the control plane and closes resources.
func (cp *ControlPlane) Shutdown() {
	// Stop scheduler first
	if cp.container != nil && cp.container.Scheduler != nil {
		cp.container.Scheduler.Stop()
	}
	if cp.auditLogger != nil {
		if err := cp.auditLogger.Close(); err != nil {
			log.Printf("Error closing audit logger: %v", err)
		}
	}
}

var startTime time.Time

func main() {
	// Load .env from repo root (two levels up from apps/control-plane/).
	// Silently ignored in production where real env vars are set.
	godotenv.Load("../../.env") //nolint:errcheck // best-effort .env load

	startTime = time.Now()

	log.Println("AllSource Control Plane starting...")

	port := os.Getenv("PORT")
	if port == "" {
		port = DefaultPort
	}

	ctx := context.Background()

	cp, err := NewControlPlane(ctx)
	if err != nil {
		log.Fatalf("Failed to initialize control plane: %v", err)
	}

	// Graceful shutdown
	srv := &http.Server{
		Addr:              ":" + port,
		Handler:           cp.router,
		ReadHeaderTimeout: 10 * time.Second,
	}

	go func() {
		// Version is a compile-time constant, port is operator config — not user input.
		log.Printf("Control Plane v%s listening on port %s", Version, port) //nolint:gosec // G706
		log.Println("Persistence: core (no PostgreSQL)")
		log.Println("Authentication enabled")
		log.Println("RBAC enabled")
		log.Println("Audit logging enabled")
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			log.Fatalf("Server failed: %v", err)
		}
	}()

	// Wait for interrupt signal
	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)
	<-quit

	log.Println("Shutting down gracefully...")
	shutdownCtx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	if err := srv.Shutdown(shutdownCtx); err != nil {
		log.Printf("Server forced to shutdown: %v", err)
	}

	// Cleanup resources
	cp.Shutdown()

	log.Println("Control Plane stopped")
}

// warnX402MultiInstance logs a loud warning at boot when the operator has
// signaled that the Control Plane runs more than one instance. The x402
// included-allowance counter is in-process (CoreQuotaChecker), so it is exact
// only on a single machine; across N instances the boundary overshoot is
// bounded by (N-1) × calls-since-the-last-1-minute reconciler tick. Set
// CONTROL_PLANE_MULTI_INSTANCE=true when scaling out so this trade-off is
// visible — and move the counter to a shared Core-side atomic counter to make
// it exact again (see CoreQuotaChecker doc + docs/runbooks/PRICING_BILLING_CUTOVER.md).
func warnX402MultiInstance() {
	v := strings.ToLower(strings.TrimSpace(os.Getenv("CONTROL_PLANE_MULTI_INSTANCE")))
	if v == envValueTrue || v == "1" || v == "yes" {
		log.Printf("WARNING: CONTROL_PLANE_MULTI_INSTANCE is set — the x402 included-allowance " +
			"counter is per-instance and NOT shared. Residual boundary overshoot is bounded by " +
			"(instances-1) × calls-since-last-reconciler-tick (~1 min). Move to a shared Core-side " +
			"atomic counter for exactness across instances (t-d7aabc).")
	}
}

// logBillingConfigCheck runs the billing config self-check at boot and logs any
// issues loudly. Never fatal — a config warning must not block startup.
func logBillingConfigCheck(container *internal.Container) {
	if container.VerifyBillingConfigUC == nil {
		return
	}
	report := container.VerifyBillingConfigUC.Execute()
	if report.Skipped {
		log.Printf("BillingConfigCheck: skipped — LemonSqueezy billing not configured")
		return
	}
	if report.OK && len(report.Issues) == 0 {
		log.Printf("BillingConfigCheck: OK — %v", report.Facts)
		return
	}
	for _, issue := range report.Issues {
		log.Printf("BillingConfigCheck: [%s] %s: %s", issue.Severity, issue.Code, issue.Message)
	}
	if !report.OK {
		log.Printf("BillingConfigCheck: FAILED — billing is misconfigured; checkout/webhooks may silently misbehave. See GET /api/v1/admin/billing/config-check")
	}
}
