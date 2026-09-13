package clients

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestCreateCheckout_Success(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost || r.URL.Path != "/v1/checkouts" {
			t.Errorf("unexpected request: %s %s", r.Method, r.URL.Path)
		}
		if r.Header.Get("Authorization") != "Bearer test-key" {
			t.Errorf("missing or wrong auth header: %s", r.Header.Get("Authorization"))
		}
		if r.Header.Get("Accept") != "application/vnd.api+json" {
			t.Errorf("missing accept header")
		}

		var body jsonAPIEnvelope
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Fatalf("decode request body: %v", err)
		}
		if body.Data.Type != "checkouts" {
			t.Errorf("expected type checkouts, got %s", body.Data.Type)
		}

		// Verify relationships contain store and variant
		var rels map[string]jsonAPIRelationship
		if err := json.Unmarshal(body.Data.Relationships, &rels); err != nil {
			t.Fatalf("decode relationships: %v", err)
		}
		if rels["store"].Data.ID != "store-123" {
			t.Errorf("expected store ID store-123, got %s", rels["store"].Data.ID)
		}
		if rels["variant"].Data.ID != "variant-456" {
			t.Errorf("expected variant ID variant-456, got %s", rels["variant"].Data.ID)
		}

		w.Header().Set("Content-Type", "application/vnd.api+json")
		w.WriteHeader(http.StatusOK)
		resp := jsonAPIEnvelope{
			Data: jsonAPIData{
				Type: "checkouts",
				ID:   "checkout-789",
				Attributes: mustJSON(t, checkoutAttributes{
					URL: "https://store.lemonsqueezy.com/checkout/custom/checkout-789",
				}),
			},
		}
		_ = json.NewEncoder(w).Encode(resp) //nolint:errcheck // test handler
	}))
	defer server.Close()

	client := newLemonSqueezyClient(server.URL, "test-key", "store-123", nil)

	result, err := client.CreateCheckout(context.Background(), CreateCheckoutRequest{
		VariantID:   "variant-456",
		Email:       "user@example.com",
		RedirectURL: "https://app.example.com/success",
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.ID != "checkout-789" {
		t.Errorf("expected checkout ID checkout-789, got %s", result.ID)
	}
	if result.URL != "https://store.lemonsqueezy.com/checkout/custom/checkout-789" {
		t.Errorf("unexpected checkout URL: %s", result.URL)
	}
}

func TestCreateCheckout_APIError(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusUnprocessableEntity)
		_, _ = w.Write([]byte(`{"errors":[{"detail":"Variant not found"}]}`)) //nolint:errcheck // test handler
	}))
	defer server.Close()

	client := newLemonSqueezyClient(server.URL, "test-key", "store-123", nil)

	_, err := client.CreateCheckout(context.Background(), CreateCheckoutRequest{
		VariantID: "bad-variant",
	})
	if err == nil {
		t.Fatal("expected error, got nil")
	}
	if got := err.Error(); got == "" {
		t.Error("expected non-empty error message")
	}
}

func TestGetVariant_UsesCurrentPriceModelNotDeprecatedVariantPrice(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/vnd.api+json")
		switch r.URL.Path {
		case "/v1/variants/42":
			_ = json.NewEncoder(w).Encode(jsonAPIEnvelope{Data: jsonAPIData{
				Type: "variants", ID: "42",
				Attributes: mustJSON(t, map[string]any{"name": "Indie annual", "status": "published", "test_mode": false, "price": 18199, "interval": "year"}),
			}})
		case "/v1/variants/42/price-model":
			_ = json.NewEncoder(w).Encode(jsonAPIEnvelope{Data: jsonAPIData{
				Type: "prices", ID: "new-price",
				Attributes: mustJSON(t, map[string]any{"variant_id": 42, "category": "subscription", "scheme": "standard", "unit_price": 19999, "renewal_interval_unit": "year", "renewal_interval_quantity": 1}),
			}})
		default:
			t.Errorf("unexpected request: %s", r.URL.Path)
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	defer server.Close()

	client := newLemonSqueezyClient(server.URL, "test-key", "store-123", nil)
	variant, err := client.GetVariant(context.Background(), "42")
	if err != nil {
		t.Fatalf("GetVariant: %v", err)
	}
	if variant.Price != 19999 || variant.Interval != "year" {
		t.Fatalf("current price = %+v, want 19999 cents/year", variant)
	}
}

func TestGetVariant_RejectsUnsupportedPriceModel(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/vnd.api+json")
		if r.URL.Path == "/v1/variants/42" {
			_ = json.NewEncoder(w).Encode(jsonAPIEnvelope{Data: jsonAPIData{
				Type: "variants", ID: "42", Attributes: mustJSON(t, map[string]any{"status": "published", "price": 18199}),
			}})
			return
		}
		_ = json.NewEncoder(w).Encode(jsonAPIEnvelope{Data: jsonAPIData{
			Type: "prices", ID: "new-price",
			Attributes: mustJSON(t, map[string]any{"variant_id": 42, "category": "subscription", "scheme": "graduated", "unit_price": 19999, "renewal_interval_unit": "year", "renewal_interval_quantity": 1}),
		}})
	}))
	defer server.Close()

	client := newLemonSqueezyClient(server.URL, "test-key", "store-123", nil)
	if _, err := client.GetVariant(context.Background(), "42"); err == nil {
		t.Fatal("unsupported price model must not show deprecated variant price")
	}
}

func TestGetSubscription_Success(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet || r.URL.Path != "/v1/subscriptions/sub-100" {
			t.Errorf("unexpected request: %s %s", r.Method, r.URL.Path)
		}

		w.Header().Set("Content-Type", "application/vnd.api+json")
		resp := jsonAPIEnvelope{
			Data: jsonAPIData{
				Type: "subscriptions",
				ID:   "sub-100",
				Attributes: mustJSON(t, subscriptionAttributes{
					Status:       "active",
					VariantID:    42,
					ProductID:    10,
					CustomerID:   5,
					CardBrand:    "visa",
					CardLastFour: "4242",
					RenewsAt:     "2026-03-14T00:00:00Z",
					URLs: subscriptionURLs{
						CustomerPortal:      "https://portal.example.com/sub-100",
						UpdatePaymentMethod: "https://pay.example.com/sub-100",
					},
				}),
			},
		}
		_ = json.NewEncoder(w).Encode(resp) //nolint:errcheck // test handler
	}))
	defer server.Close()

	client := newLemonSqueezyClient(server.URL, "test-key", "store-123", nil)

	sub, err := client.GetSubscription(context.Background(), "sub-100")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if sub.ID != "sub-100" {
		t.Errorf("expected ID sub-100, got %s", sub.ID)
	}
	if sub.Status != "active" {
		t.Errorf("expected status active, got %s", sub.Status)
	}
	if sub.VariantID != 42 {
		t.Errorf("expected variant_id 42, got %d", sub.VariantID)
	}
	if sub.CustomerPortalURL != "https://portal.example.com/sub-100" {
		t.Errorf("unexpected portal URL: %s", sub.CustomerPortalURL)
	}
	if sub.UpdatePaymentMethodURL != "https://pay.example.com/sub-100" {
		t.Errorf("unexpected payment method URL: %s", sub.UpdatePaymentMethodURL)
	}
}

func TestGetSubscription_NotFound(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusNotFound)
		_, _ = w.Write([]byte(`{"errors":[{"detail":"Not found"}]}`)) //nolint:errcheck // test handler
	}))
	defer server.Close()

	client := newLemonSqueezyClient(server.URL, "test-key", "store-123", nil)

	_, err := client.GetSubscription(context.Background(), "nonexistent")
	if err == nil {
		t.Fatal("expected error, got nil")
	}
}

func TestGetCustomerPortalURL_Success(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/vnd.api+json")
		resp := jsonAPIEnvelope{
			Data: jsonAPIData{
				Type: "subscriptions",
				ID:   "sub-200",
				Attributes: mustJSON(t, subscriptionAttributes{
					Status: "active",
					URLs: subscriptionURLs{
						CustomerPortal: "https://portal.example.com/sub-200",
					},
				}),
			},
		}
		_ = json.NewEncoder(w).Encode(resp) //nolint:errcheck // test handler
	}))
	defer server.Close()

	client := newLemonSqueezyClient(server.URL, "test-key", "store-123", nil)

	url, err := client.GetCustomerPortalURL(context.Background(), "sub-200")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if url != "https://portal.example.com/sub-200" {
		t.Errorf("unexpected portal URL: %s", url)
	}
}

func TestGetCustomerPortalURL_NoURL(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/vnd.api+json")
		resp := jsonAPIEnvelope{
			Data: jsonAPIData{
				Type: "subscriptions",
				ID:   "sub-300",
				Attributes: mustJSON(t, subscriptionAttributes{
					Status: "canceled",
				}),
			},
		}
		_ = json.NewEncoder(w).Encode(resp) //nolint:errcheck // test handler
	}))
	defer server.Close()

	client := newLemonSqueezyClient(server.URL, "test-key", "store-123", nil)

	_, err := client.GetCustomerPortalURL(context.Background(), "sub-300")
	if err == nil {
		t.Fatal("expected error for missing portal URL")
	}
}

func TestReportUsage_Success(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost || r.URL.Path != "/v1/usage-records" {
			t.Errorf("unexpected request: %s %s", r.Method, r.URL.Path)
		}

		var body jsonAPIEnvelope
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Fatalf("decode request body: %v", err)
		}
		if body.Data.Type != "usage-records" {
			t.Errorf("expected type usage-records, got %s", body.Data.Type)
		}

		// Verify attributes
		var attrs struct {
			Quantity int    `json:"quantity"`
			Action   string `json:"action"`
		}
		if err := json.Unmarshal(body.Data.Attributes, &attrs); err != nil {
			t.Fatalf("decode attributes: %v", err)
		}
		if attrs.Quantity != 100 {
			t.Errorf("expected quantity 100, got %d", attrs.Quantity)
		}
		if attrs.Action != "increment" {
			t.Errorf("expected action increment, got %s", attrs.Action)
		}

		// Verify relationship
		var rels map[string]jsonAPIRelationship
		if err := json.Unmarshal(body.Data.Relationships, &rels); err != nil {
			t.Fatalf("decode relationships: %v", err)
		}
		if rels["subscription-item"].Data.ID != "si-50" {
			t.Errorf("expected subscription-item ID si-50, got %s", rels["subscription-item"].Data.ID)
		}

		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`{"data":{"type":"usage-records","id":"ur-1"}}`)) //nolint:errcheck // test handler
	}))
	defer server.Close()

	client := newLemonSqueezyClient(server.URL, "test-key", "store-123", nil)

	err := client.ReportUsage(context.Background(), ReportUsageRequest{
		SubscriptionItemID: "si-50",
		Quantity:           100,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestReportUsage_APIError(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusBadRequest)
		_, _ = w.Write([]byte(`{"errors":[{"detail":"Invalid quantity"}]}`)) //nolint:errcheck // test handler
	}))
	defer server.Close()

	client := newLemonSqueezyClient(server.URL, "test-key", "store-123", nil)

	err := client.ReportUsage(context.Background(), ReportUsageRequest{
		SubscriptionItemID: "si-50",
		Quantity:           -1,
	})
	if err == nil {
		t.Fatal("expected error, got nil")
	}
}

func TestLookupVariantID(t *testing.T) {
	client := newLemonSqueezyClient("http://unused", "key", "store", VariantMap{
		"indie:monthly":  "var_im",
		"indie:annual":   "var_ia",
		"studio:monthly": "var_sm",
		"scale":          "var_legacy", // bare key = monthly fallback
	})

	tests := []struct {
		tier    string
		period  string
		wantID  string
		wantErr bool
	}{
		{"indie", "monthly", "var_im", false},
		{"indie", "annual", "var_ia", false},
		{"indie", "yearly", "var_ia", false},      // yearly normalizes to annual
		{"indie", "", "var_im", false},            // empty defaults to monthly
		{"studio", "annual", "var_sm", false},     // no annual key → falls back to monthly
		{"scale", "monthly", "var_legacy", false}, // bare key fallback
		{"scale", "annual", "var_legacy", false},  // bare key fallback for any period
		{"unknown", "monthly", "", true},
	}

	for _, tt := range tests {
		id, err := client.LookupVariantID(tt.tier, tt.period)
		if (err != nil) != tt.wantErr {
			t.Errorf("LookupVariantID(%q,%q): err=%v, wantErr=%v", tt.tier, tt.period, err, tt.wantErr)
		}
		if id != tt.wantID {
			t.Errorf("LookupVariantID(%q,%q): got %q, want %q", tt.tier, tt.period, id, tt.wantID)
		}
	}
}

func TestLookupVariantID_NoMap(t *testing.T) {
	client := newLemonSqueezyClient("http://unused", "key", "store", nil)

	_, err := client.LookupVariantID("free", "monthly")
	if err == nil {
		t.Fatal("expected error when variant map is nil")
	}
}

func TestNewLemonSqueezyClient_EnvVars(t *testing.T) {
	// Missing API key
	t.Setenv("LEMON_SQUEEZY_API_KEY", "")
	t.Setenv("LEMON_SQUEEZY_STORE_ID", "store-1")
	_, err := NewLemonSqueezyClient()
	if err == nil {
		t.Fatal("expected error for missing API key")
	}

	// Missing store ID
	t.Setenv("LEMON_SQUEEZY_API_KEY", "key-1")
	t.Setenv("LEMON_SQUEEZY_STORE_ID", "")
	_, err = NewLemonSqueezyClient()
	if err == nil {
		t.Fatal("expected error for missing store ID")
	}

	// Valid config
	t.Setenv("LEMON_SQUEEZY_API_KEY", "key-1")
	t.Setenv("LEMON_SQUEEZY_STORE_ID", "store-1")
	t.Setenv("LEMON_SQUEEZY_VARIANT_MAP", `{"free":"v1","pro":"v2"}`)
	client, err := NewLemonSqueezyClient()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if client == nil {
		t.Fatal("expected non-nil client")
	}

	// Invalid variant map JSON
	t.Setenv("LEMON_SQUEEZY_VARIANT_MAP", `{invalid}`)
	_, err = NewLemonSqueezyClient()
	if err == nil {
		t.Fatal("expected error for invalid variant map JSON")
	}
}

func mustJSON(t *testing.T, v any) json.RawMessage {
	t.Helper()
	b, err := json.Marshal(v)
	if err != nil {
		t.Fatalf("mustJSON: %v", err)
	}
	return b
}
