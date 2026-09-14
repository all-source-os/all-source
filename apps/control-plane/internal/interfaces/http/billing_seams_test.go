package http //nolint:revive // package name intentionally matches directory

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/application/usecases"
	"github.com/allsource/control-plane/internal/domain/entities"
	"github.com/allsource/control-plane/internal/infrastructure/clients"
	"github.com/allsource/control-plane/internal/infrastructure/persistence"
)

// stubLS implements only the LemonSqueezyClient methods the billing handler
// seams touch; anything else panics (nil interface), which keeps the test
// honest about what the path actually calls.
type stubLS struct {
	clients.LemonSqueezyClient
	portalURL string
	variants  map[string]string // "tier:period" -> variant id
	prices    map[string]int    // variant id -> cents
	currency  string            // store currency; empty means USD
}

func (s *stubLS) GetCustomerPortalURL(_ context.Context, _ string) (string, error) {
	return s.portalURL, nil
}
func (s *stubLS) CreateCheckout(_ context.Context, _ clients.CreateCheckoutRequest) (*clients.CheckoutResponse, error) {
	return &clients.CheckoutResponse{ID: "co-1", URL: "https://checkout.example/co-1"}, nil
}
func (s *stubLS) UpdateSubscription(_ context.Context, subID string, variantID int) (*clients.SubscriptionResponse, error) {
	return &clients.SubscriptionResponse{ID: subID, Status: "active", VariantID: variantID, CustomerID: 789}, nil
}
func (s *stubLS) LookupVariantID(tier, period string) (string, error) {
	if period == "" {
		period = "monthly"
	}
	return s.variants[tier+":"+period], nil
}
func (s *stubLS) GetVariant(_ context.Context, variantID string) (*clients.VariantResponse, error) {
	if c, ok := s.prices[variantID]; ok {
		interval := "month"
		for key, id := range s.variants {
			if id == variantID && strings.HasSuffix(key, ":annual") {
				interval = "year"
			}
		}
		return &clients.VariantResponse{ID: variantID, Price: c, Interval: interval}, nil
	}
	return nil, nil
}
func (s *stubLS) GetStoreID() string { return "store-1" }

// GetStoreCurrency backs GetCatalogUseCase's currency lookup. It is implemented
// here rather than left to the embedded nil interface because the catalog path
// genuinely calls it: without this, TestSeam_Catalog_ServesLSPrices panicked
// with a nil-pointer dereference on every run. That is the cost of the
// embedded-interface stub above — adding a method to LemonSqueezyClient turns
// into a runtime panic in whichever test exercises it, not a compile error.
// Defaults to USD so the existing price assertions ("$79") hold; set `currency`
// to exercise the non-USD path (the real LS store is GBP).
func (s *stubLS) GetStoreCurrency(_ context.Context) (string, error) {
	if s.currency == "" {
		return "USD", nil
	}
	return s.currency, nil
}

func withAuthTenant(tid string) gin.HandlerFunc {
	return func(c *gin.Context) {
		c.Set("auth_tenant_id", tid)
		c.Next()
	}
}

func newBillingHandler(t *testing.T, repo *persistence.MemoryTenantRepository, ls clients.LemonSqueezyClient) *BillingHandler {
	t.Helper()
	auditRepo := persistence.NewMemoryAuditRepository()
	updateSubUC := usecases.NewUpdateSubscriptionMetadataUseCase(repo, auditRepo)
	return NewBillingHandler(
		usecases.NewCreateCheckoutUseCase(repo, ls, auditRepo),
		usecases.NewChangePlanUseCase(repo, ls, updateSubUC),
		nil, nil, nil, nil,
		usecases.NewGetCatalogUseCase(ls),
	)
}

func seedActiveSub(t *testing.T, repo *persistence.MemoryTenantRepository, id, subID, tier string) {
	t.Helper()
	sub := &entities.SubscriptionMetadata{
		SubscriptionID:  subID,
		Status:          "active",
		Tier:            tier,
		PaymentProvider: "lemonsqueezy",
	}
	if err := repo.Save(&entities.Tenant{
		ID:        id,
		Name:      id,
		Status:    entities.TenantStatusActive,
		CreatedAt: time.Now(),
		UpdatedAt: time.Now(),
		Metadata: map[string]interface{}{
			"subscription": sub,
			"subscriptions": map[string]entities.SubscriptionRef{
				subID: {Tier: tier, Status: "active", PaymentProvider: "lemonsqueezy"},
			},
		},
	}); err != nil {
		t.Fatalf("seed tenant %s: %v", id, err)
	}
}

func postJSON(router *gin.Engine, path, body string) *httptest.ResponseRecorder {
	req := httptest.NewRequestWithContext(context.Background(), http.MethodPost, path, strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	router.ServeHTTP(w, req)
	return w
}

// Seam: change-plan must operate on the AUTHENTICATED tenant, never the
// client-supplied tenant_id in the body (the tenant-scoping override that makes
// the self-serve PermissionWrite gate safe).
func TestSeam_ChangePlan_TenantScopingOverride(t *testing.T) {
	repo := persistence.NewMemoryTenantRepository()
	seedActiveSub(t, repo, "tenant-Y", "sub-1", "indie") // authed tenant, has a sub
	seedActiveSub(t, repo, "tenant-X", "sub-9", "indie") // attacker-supplied target
	ls := &stubLS{variants: map[string]string{"studio:monthly": "3"}}
	h := newBillingHandler(t, repo, ls)

	router := gin.New()
	router.POST("/change-plan", withAuthTenant("tenant-Y"), h.ChangePlan)

	// Body says tenant-X; the override must force tenant-Y.
	w := postJSON(router, "/change-plan", `{"tenant_id":"tenant-X","tier":"studio"}`)
	if w.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d: %s", w.Code, w.Body.String())
	}
	var res usecases.ChangePlanResult
	if err := json.Unmarshal(w.Body.Bytes(), &res); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if res.Tier != "studio" {
		t.Errorf("response tier = %q, want studio", res.Tier)
	}
	// tenant-Y upgraded...
	ty, _ := repo.FindByID("tenant-Y")                                                             //nolint:errcheck // test assertion reads state seeded above
	if got := ty.Metadata["subscription"].(*entities.SubscriptionMetadata).Tier; got != "studio" { //nolint:forcetypeassert,errcheck // a wrong type must fail this test loudly
		t.Errorf("tenant-Y tier = %q, want studio", got)
	}
	// ...tenant-X untouched.
	tx, _ := repo.FindByID("tenant-X")                                                            //nolint:errcheck // test assertion reads state seeded above
	if got := tx.Metadata["subscription"].(*entities.SubscriptionMetadata).Tier; got != "indie" { //nolint:forcetypeassert,errcheck // a wrong type must fail this test loudly
		t.Errorf("tenant-X tier = %q, want indie (untouched)", got)
	}
}

// Seam: checkout for a tenant with an active subscription returns the customer
// portal URL (is_portal) instead of creating a duplicate subscription.
func TestSeam_Checkout_DedupGuardReturnsPortal(t *testing.T) {
	repo := persistence.NewMemoryTenantRepository()
	seedActiveSub(t, repo, "tenant-Y", "sub-1", "indie")
	ls := &stubLS{portalURL: "https://portal.example/manage"}
	h := newBillingHandler(t, repo, ls)

	router := gin.New()
	router.POST("/checkout", withAuthTenant("tenant-Y"), h.CreateCheckout)

	w := postJSON(router, "/checkout", `{"tenant_id":"tenant-Y","tier":"studio"}`)
	if w.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d: %s", w.Code, w.Body.String())
	}
	var res struct {
		CheckoutURL string `json:"checkout_url"`
		IsPortal    bool   `json:"is_portal"`
	}
	if err := json.Unmarshal(w.Body.Bytes(), &res); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if !res.IsPortal || res.CheckoutURL != "https://portal.example/manage" {
		t.Errorf("expected portal redirect, got %+v", res)
	}
}

// Seam: checkout for a tenant with NO active subscription creates a real
// checkout (is_portal false).
func TestSeam_Checkout_NoActiveSubCreatesCheckout(t *testing.T) {
	repo := persistence.NewMemoryTenantRepository()
	if err := repo.Save(&entities.Tenant{ID: "tenant-Z", Name: "Z", Status: entities.TenantStatusActive, CreatedAt: time.Now(), UpdatedAt: time.Now()}); err != nil {
		t.Fatalf("seed: %v", err)
	}
	ls := &stubLS{variants: map[string]string{"studio:monthly": "3"}}
	h := newBillingHandler(t, repo, ls)

	router := gin.New()
	router.POST("/checkout", withAuthTenant("tenant-Z"), h.CreateCheckout)

	w := postJSON(router, "/checkout", `{"tenant_id":"tenant-Z","tier":"studio"}`)
	if w.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d: %s", w.Code, w.Body.String())
	}
	var res struct {
		CheckoutURL string `json:"checkout_url"`
		IsPortal    bool   `json:"is_portal"`
	}
	_ = json.Unmarshal(w.Body.Bytes(), &res) //nolint:errcheck // test decode
	if res.IsPortal || res.CheckoutURL == "" {
		t.Errorf("expected fresh checkout, got %+v", res)
	}
}

// Seam: the catalog endpoint serves LS-sourced prices in the documented shape.
func TestSeam_Catalog_ServesLSPrices(t *testing.T) {
	repo := persistence.NewMemoryTenantRepository()
	ls := &stubLS{
		variants: map[string]string{"studio:monthly": "3", "studio:annual": "4"},
		prices:   map[string]int{"3": 7900, "4": 75900},
	}
	h := newBillingHandler(t, repo, ls)

	router := gin.New()
	router.GET("/catalog", h.GetCatalog)

	req := httptest.NewRequestWithContext(context.Background(), http.MethodGet, "/catalog", http.NoBody)
	w := httptest.NewRecorder()
	router.ServeHTTP(w, req)
	if w.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d: %s", w.Code, w.Body.String())
	}
	var cat usecases.Catalog
	if err := json.Unmarshal(w.Body.Bytes(), &cat); err != nil {
		t.Fatalf("decode: %v", err)
	}
	var studio *usecases.CatalogTier
	for i := range cat.Tiers {
		if cat.Tiers[i].Tier == "studio" {
			studio = &cat.Tiers[i]
		}
	}
	if studio == nil || studio.Monthly == nil || studio.Monthly.Formatted != "$79" {
		t.Fatalf("expected studio monthly $79, got %+v", cat.Tiers)
	}
}

// Seam: change-plan use case absent → 503 (not a panic / 500).
func TestSeam_ChangePlan_Unavailable(t *testing.T) {
	h := NewBillingHandler(nil, nil, nil, nil, nil, nil, nil)
	router := gin.New()
	router.POST("/change-plan", h.ChangePlan)
	w := postJSON(router, "/change-plan", `{"tier":"studio"}`)
	if w.Code != http.StatusServiceUnavailable {
		t.Errorf("expected 503, got %d", w.Code)
	}
}

// Seam: a validly-signed webhook resolves and PERSISTS the tier end-to-end.
func TestSeam_Webhook_SignatureToTierPersisted(t *testing.T) {
	repo := persistence.NewMemoryTenantRepository()
	if err := repo.Save(&entities.Tenant{ID: "tenant-123", Name: "T", Status: entities.TenantStatusActive, CreatedAt: time.Now(), UpdatedAt: time.Now()}); err != nil {
		t.Fatalf("seed: %v", err)
	}
	auditRepo := persistence.NewMemoryAuditRepository()
	updateSubUC := usecases.NewUpdateSubscriptionMetadataUseCase(repo, auditRepo)
	suspendUC := usecases.NewSuspendTenantUseCase(repo, auditRepo)
	processUC := usecases.NewProcessLemonSqueezyWebhookUseCase(repo, auditRepo, updateSubUC, suspendUC, usecases.VariantTierMap{"555": "studio"})
	handler := NewWebhookHandler(processUC)

	secret := "seam-secret"
	t.Setenv("LEMON_SQUEEZY_WEBHOOK_SECRET", secret)
	router := gin.New()
	router.POST("/api/v1/webhooks/lemonsqueezy", handler.LemonSqueezy)

	event := map[string]interface{}{
		"event_name": "subscription_created",
		"data": map[string]interface{}{
			"id":   "sub-777",
			"type": "subscriptions",
			"attributes": map[string]interface{}{
				"store_id":    1,
				"customer_id": 789,
				"status":      "active",
				"variant_id":  555, // resolves to studio via the variant map
			},
		},
		"meta": map[string]interface{}{"custom_data": map[string]interface{}{"tenant_id": "tenant-123"}},
	}
	body, _ := json.Marshal(event) //nolint:errcheck // test fixture
	req := httptest.NewRequestWithContext(context.Background(), http.MethodPost, "/api/v1/webhooks/lemonsqueezy", strings.NewReader(string(body)))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-Signature", signPayload(body, secret))
	w := httptest.NewRecorder()
	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d: %s", w.Code, w.Body.String())
	}
	tn, _ := repo.FindByID("tenant-123")                                //nolint:errcheck // test assertion reads state seeded above
	got := tn.Metadata["subscription"].(*entities.SubscriptionMetadata) //nolint:forcetypeassert,errcheck // a wrong type must fail this test loudly
	if got.Tier != "studio" || got.SubscriptionID != "sub-777" {
		t.Errorf("persisted sub = %+v, want studio/sub-777", got)
	}
}
