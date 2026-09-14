package clients

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/go-resty/resty/v2"
)

const (
	lemonSqueezyBaseURL = "https://api.lemonsqueezy.com"
	lemonSqueezyTimeout = 30 * time.Second
	lemonSqueezyTracer  = "lemonsqueezy-client"
)

// LemonSqueezyClient defines methods for interacting with the LemonSqueezy API.
type LemonSqueezyClient interface {
	CreateCheckout(ctx context.Context, req CreateCheckoutRequest) (*CheckoutResponse, error)
	GetCustomerPortalURL(ctx context.Context, subscriptionID string) (string, error)
	ReportUsage(ctx context.Context, req ReportUsageRequest) error
	GetSubscription(ctx context.Context, subscriptionID string) (*SubscriptionResponse, error)
	// UpdateSubscription changes the plan of an existing subscription by swapping
	// its variant (LS prorates). Used for in-app up/downgrade across tiers.
	UpdateSubscription(ctx context.Context, subscriptionID string, variantID int) (*SubscriptionResponse, error)
	ListSubscriptions(ctx context.Context, storeID string, page int) (*SubscriptionListResponse, error)
	ListInvoices(ctx context.Context, storeID string, page int, status string) (*InvoiceListResponse, error)
	RefundInvoice(ctx context.Context, orderID string, amount int) error
	// LookupVariantID resolves a tier + billing period to a LemonSqueezy variant
	// ID. period is "monthly" or "annual"; empty defaults to monthly.
	LookupVariantID(tier, period string) (string, error)
	// GetVariant fetches a variant and its current Price object from LS.
	GetVariant(ctx context.Context, variantID string) (*VariantResponse, error)
	// VariantMap returns the configured tier:period → variant-ID map (read-only
	// copy) so callers can enumerate the catalog.
	VariantMap() VariantMap
	GetStoreID() string
	// GetStoreCurrency fetches the store's billing currency (ISO code, e.g. "USD",
	// "GBP") so the catalog formats prices in what customers are actually charged
	// rather than a hardcoded "$".
	GetStoreCurrency(ctx context.Context) (string, error)
}

// VariantResponse is a LemonSqueezy variant's pricing detail.
type VariantResponse struct {
	ID       string `json:"id"`
	Name     string `json:"name"`
	Price    int    `json:"price"`    // in cents
	Interval string `json:"interval"` // "month" | "year" | "" (one-time)
	Status   string `json:"status"`
}

type variantAttributes struct {
	Name     string `json:"name"`
	Status   string `json:"status"`
	TestMode bool   `json:"test_mode"`
}

type priceAttributes struct {
	VariantID               int     `json:"variant_id"`
	Category                string  `json:"category"`
	Scheme                  string  `json:"scheme"`
	UsageAggregation        *string `json:"usage_aggregation"`
	UnitPrice               *int    `json:"unit_price"`
	SetupFeeEnabled         bool    `json:"setup_fee_enabled"`
	RenewalIntervalUnit     string  `json:"renewal_interval_unit"`
	RenewalIntervalQuantity int     `json:"renewal_interval_quantity"`
}

// VariantMap maps plan tier names to LemonSqueezy variant IDs.
type VariantMap map[string]string

// --- Request types ---

// CreateCheckoutRequest contains parameters for creating a LemonSqueezy checkout.
type CreateCheckoutRequest struct {
	VariantID   string            `json:"variant_id"`
	Email       string            `json:"email,omitempty"`
	Name        string            `json:"name,omitempty"`
	CustomData  map[string]string `json:"custom_data,omitempty"`
	RedirectURL string            `json:"redirect_url,omitempty"`
}

// ReportUsageRequest contains parameters for reporting usage to LemonSqueezy.
type ReportUsageRequest struct {
	SubscriptionItemID string `json:"subscription_item_id"`
	Quantity           int    `json:"quantity"`
	Action             string `json:"action,omitempty"` // "increment" (default) or "set"
}

// --- Response types ---

// CheckoutResponse contains the result of creating a checkout.
type CheckoutResponse struct {
	ID  string `json:"id"`
	URL string `json:"url"`
}

// SubscriptionResponse contains subscription details from LemonSqueezy.
type SubscriptionResponse struct {
	ID                     string `json:"id"`
	Status                 string `json:"status"`
	VariantID              int    `json:"variant_id"`
	ProductID              int    `json:"product_id"`
	CustomerID             int    `json:"customer_id"`
	CardBrand              string `json:"card_brand,omitempty"`
	CardLastFour           string `json:"card_last_four,omitempty"`
	RenewsAt               string `json:"renews_at,omitempty"`
	EndsAt                 string `json:"ends_at,omitempty"`
	CustomerPortalURL      string `json:"customer_portal_url,omitempty"`
	UpdatePaymentMethodURL string `json:"update_payment_method_url,omitempty"`
}

// --- JSON:API envelope types (internal) ---

type jsonAPIEnvelope struct {
	Data jsonAPIData `json:"data"`
}

type jsonAPIData struct {
	Type          string          `json:"type"`
	ID            string          `json:"id,omitempty"`
	Attributes    json.RawMessage `json:"attributes"`
	Relationships json.RawMessage `json:"relationships,omitempty"`
}

type checkoutAttributes struct {
	StoreID        int                  `json:"store_id,omitempty"`
	URL            string               `json:"url,omitempty"`
	CheckoutData   *checkoutDataAttrs   `json:"checkout_data,omitempty"`
	ProductOptions *productOptionsAttrs `json:"product_options,omitempty"`
}

type checkoutDataAttrs struct {
	Email  string            `json:"email,omitempty"`
	Name   string            `json:"name,omitempty"`
	Custom map[string]string `json:"custom,omitempty"`
}

type productOptionsAttrs struct {
	RedirectURL string `json:"redirect_url,omitempty"`
}

type subscriptionAttributes struct {
	Status       string           `json:"status"`
	StoreID      int              `json:"store_id"`
	CustomerID   int              `json:"customer_id"`
	OrderID      int              `json:"order_id"`
	ProductID    int              `json:"product_id"`
	VariantID    int              `json:"variant_id"`
	CardBrand    string           `json:"card_brand"`
	CardLastFour string           `json:"card_last_four"`
	RenewsAt     string           `json:"renews_at"`
	EndsAt       string           `json:"ends_at"`
	URLs         subscriptionURLs `json:"urls"`
}

type subscriptionURLs struct {
	UpdatePaymentMethod string `json:"update_payment_method"`
	CustomerPortal      string `json:"customer_portal"`
}

type jsonAPIRelationship struct {
	Data struct {
		Type string `json:"type"`
		ID   string `json:"id"`
	} `json:"data"`
}

// --- Implementation ---

type lemonSqueezyClient struct {
	client     *resty.Client
	storeID    string
	variantMap VariantMap
}

// NewLemonSqueezyClient creates a LemonSqueezyClient from environment variables:
//   - LEMON_SQUEEZY_API_KEY (required)
//   - LEMON_SQUEEZY_STORE_ID (required)
//   - LEMON_SQUEEZY_VARIANT_MAP (optional JSON). Keys are "<tier>:<period>"
//     where period is "monthly" or "annual"; a bare "<tier>" key is treated as
//     the monthly variant. 011 tiers: indie, studio, scale. Example:
//     {"indie:monthly":"var_a","indie:annual":"var_b",
//     "studio:monthly":"var_c","studio:annual":"var_d",
//     "scale:monthly":"var_e","scale:annual":"var_f"}
//     See docs/runbooks/PRICING_BILLING_CUTOVER.md.
func NewLemonSqueezyClient() (LemonSqueezyClient, error) {
	apiKey := os.Getenv("LEMON_SQUEEZY_API_KEY")
	if apiKey == "" {
		return nil, fmt.Errorf("LEMON_SQUEEZY_API_KEY is required")
	}

	storeID := os.Getenv("LEMON_SQUEEZY_STORE_ID")
	if storeID == "" {
		return nil, fmt.Errorf("LEMON_SQUEEZY_STORE_ID is required")
	}

	var variantMap VariantMap
	if raw := os.Getenv("LEMON_SQUEEZY_VARIANT_MAP"); raw != "" {
		if err := json.Unmarshal([]byte(raw), &variantMap); err != nil {
			return nil, fmt.Errorf("invalid LEMON_SQUEEZY_VARIANT_MAP: %w", err)
		}
	}

	return newLemonSqueezyClient(lemonSqueezyBaseURL, apiKey, storeID, variantMap), nil
}

func newLemonSqueezyClient(baseURL, apiKey, storeID string, variantMap VariantMap) *lemonSqueezyClient {
	transport := &http.Transport{
		MaxIdleConns:        20,
		MaxIdleConnsPerHost: 20,
		IdleConnTimeout:     90 * time.Second,
	}

	client := resty.NewWithClient(&http.Client{Transport: transport}).
		SetBaseURL(baseURL).
		SetTimeout(lemonSqueezyTimeout).
		SetHeader("Accept", "application/vnd.api+json").
		SetHeader("Content-Type", "application/vnd.api+json").
		SetHeader("Authorization", "Bearer "+apiKey).
		SetRetryCount(2).
		SetRetryWaitTime(500 * time.Millisecond).
		SetRetryMaxWaitTime(3 * time.Second).
		AddRetryCondition(func(r *resty.Response, err error) bool {
			if err != nil {
				return true
			}
			return r.StatusCode() == 429 || r.StatusCode() >= 500
		})

	return &lemonSqueezyClient{
		client:     client,
		storeID:    storeID,
		variantMap: variantMap,
	}
}

func (c *lemonSqueezyClient) CreateCheckout(ctx context.Context, req CreateCheckoutRequest) (*CheckoutResponse, error) {
	attrs := checkoutAttributes{}
	if req.Email != "" || req.Name != "" || len(req.CustomData) > 0 {
		attrs.CheckoutData = &checkoutDataAttrs{
			Email:  req.Email,
			Name:   req.Name,
			Custom: req.CustomData,
		}
	}
	if req.RedirectURL != "" {
		attrs.ProductOptions = &productOptionsAttrs{
			RedirectURL: req.RedirectURL,
		}
	}

	attrsJSON, err := json.Marshal(attrs)
	if err != nil {
		return nil, fmt.Errorf("marshal checkout attributes: %w", err)
	}

	rels := map[string]jsonAPIRelationship{
		"store": {Data: struct {
			Type string `json:"type"`
			ID   string `json:"id"`
		}{Type: "stores", ID: c.storeID}},
		"variant": {Data: struct {
			Type string `json:"type"`
			ID   string `json:"id"`
		}{Type: "variants", ID: req.VariantID}},
	}

	relsJSON, err := json.Marshal(rels)
	if err != nil {
		return nil, fmt.Errorf("marshal checkout relationships: %w", err)
	}

	body := jsonAPIEnvelope{
		Data: jsonAPIData{
			Type:          "checkouts",
			Attributes:    attrsJSON,
			Relationships: relsJSON,
		},
	}

	var respEnvelope jsonAPIEnvelope
	resp, err := c.client.R().
		SetContext(ctx).
		SetBody(body).
		SetResult(&respEnvelope).
		Post("/v1/checkouts")

	if err != nil {
		return nil, fmt.Errorf("create checkout request failed: %w", err)
	}
	if resp.StatusCode() >= 400 {
		return nil, fmt.Errorf("create checkout returned status %d: %s", resp.StatusCode(), resp.String())
	}

	var respAttrs checkoutAttributes
	if err := json.Unmarshal(respEnvelope.Data.Attributes, &respAttrs); err != nil {
		return nil, fmt.Errorf("unmarshal checkout response: %w", err)
	}

	return &CheckoutResponse{
		ID:  respEnvelope.Data.ID,
		URL: respAttrs.URL,
	}, nil
}

func (c *lemonSqueezyClient) GetCustomerPortalURL(ctx context.Context, subscriptionID string) (string, error) {
	sub, err := c.GetSubscription(ctx, subscriptionID)
	if err != nil {
		return "", err
	}
	if sub.CustomerPortalURL == "" {
		return "", fmt.Errorf("no customer portal URL for subscription %s", subscriptionID)
	}
	return sub.CustomerPortalURL, nil
}

func (c *lemonSqueezyClient) ReportUsage(ctx context.Context, req ReportUsageRequest) error {
	action := req.Action
	if action == "" {
		action = "increment"
	}

	type usageAttrs struct {
		Quantity int    `json:"quantity"`
		Action   string `json:"action"`
	}

	attrs, err := json.Marshal(usageAttrs{Quantity: req.Quantity, Action: action})
	if err != nil {
		return fmt.Errorf("marshal usage attributes: %w", err)
	}

	rels := map[string]jsonAPIRelationship{
		"subscription-item": {Data: struct {
			Type string `json:"type"`
			ID   string `json:"id"`
		}{Type: "subscription-items", ID: req.SubscriptionItemID}},
	}

	relsJSON, err := json.Marshal(rels)
	if err != nil {
		return fmt.Errorf("marshal usage relationships: %w", err)
	}

	body := jsonAPIEnvelope{
		Data: jsonAPIData{
			Type:          "usage-records",
			Attributes:    attrs,
			Relationships: relsJSON,
		},
	}

	resp, err := c.client.R().
		SetContext(ctx).
		SetBody(body).
		Post("/v1/usage-records")

	if err != nil {
		return fmt.Errorf("report usage request failed: %w", err)
	}
	if resp.StatusCode() >= 400 {
		return fmt.Errorf("report usage returned status %d: %s", resp.StatusCode(), resp.String())
	}

	return nil
}

func (c *lemonSqueezyClient) GetSubscription(ctx context.Context, subscriptionID string) (*SubscriptionResponse, error) {
	var respEnvelope jsonAPIEnvelope
	resp, err := c.client.R().
		SetContext(ctx).
		SetResult(&respEnvelope).
		Get("/v1/subscriptions/" + subscriptionID)

	if err != nil {
		return nil, fmt.Errorf("get subscription request failed: %w", err)
	}
	if resp.StatusCode() >= 400 {
		return nil, fmt.Errorf("get subscription returned status %d: %s", resp.StatusCode(), resp.String())
	}

	var attrs subscriptionAttributes
	if err := json.Unmarshal(respEnvelope.Data.Attributes, &attrs); err != nil {
		return nil, fmt.Errorf("unmarshal subscription response: %w", err)
	}

	return &SubscriptionResponse{
		ID:                     respEnvelope.Data.ID,
		Status:                 attrs.Status,
		VariantID:              attrs.VariantID,
		ProductID:              attrs.ProductID,
		CustomerID:             attrs.CustomerID,
		CardBrand:              attrs.CardBrand,
		CardLastFour:           attrs.CardLastFour,
		RenewsAt:               attrs.RenewsAt,
		EndsAt:                 attrs.EndsAt,
		CustomerPortalURL:      attrs.URLs.CustomerPortal,
		UpdatePaymentMethodURL: attrs.URLs.UpdatePaymentMethod,
	}, nil
}

// UpdateSubscription swaps a subscription's variant (plan change), letting LS
// prorate. Works across products, so it powers in-app tier up/downgrade.
func (c *lemonSqueezyClient) UpdateSubscription(ctx context.Context, subscriptionID string, variantID int) (*SubscriptionResponse, error) {
	body := map[string]any{
		"data": map[string]any{
			"type": "subscriptions",
			"id":   subscriptionID,
			"attributes": map[string]any{
				"variant_id": variantID,
			},
		},
	}
	var respEnvelope jsonAPIEnvelope
	resp, err := c.client.R().
		SetContext(ctx).
		SetBody(body).
		SetResult(&respEnvelope).
		Patch("/v1/subscriptions/" + subscriptionID)

	if err != nil {
		return nil, fmt.Errorf("update subscription request failed: %w", err)
	}
	if resp.StatusCode() >= 400 {
		return nil, fmt.Errorf("update subscription returned status %d: %s", resp.StatusCode(), resp.String())
	}

	var attrs subscriptionAttributes
	if err := json.Unmarshal(respEnvelope.Data.Attributes, &attrs); err != nil {
		return nil, fmt.Errorf("unmarshal subscription response: %w", err)
	}
	return &SubscriptionResponse{
		ID:         respEnvelope.Data.ID,
		Status:     attrs.Status,
		VariantID:  attrs.VariantID,
		ProductID:  attrs.ProductID,
		CustomerID: attrs.CustomerID,
		RenewsAt:   attrs.RenewsAt,
		EndsAt:     attrs.EndsAt,
	}, nil
}

// LookupVariantID resolves a plan tier + billing period to a LemonSqueezy
// variant ID using the configured variant map. Resolution order (first hit
// wins): "<tier>:<period>", "<tier>:monthly", "<tier>" (legacy bare key,
// treated as monthly). This keeps old single-variant-per-tier maps working
// while supporting the 011 monthly/annual split.
func (c *lemonSqueezyClient) LookupVariantID(tier, period string) (string, error) {
	if c.variantMap == nil {
		return "", fmt.Errorf("variant map not configured")
	}
	t := strings.ToLower(tier)
	p := normalizeBillingPeriod(period)
	if id, ok := c.variantMap[t+":"+p]; ok {
		return id, nil
	}
	if id, ok := c.variantMap[t+":monthly"]; ok {
		return id, nil
	}
	if id, ok := c.variantMap[t]; ok {
		return id, nil
	}
	return "", fmt.Errorf("no LemonSqueezy variant configured for tier %q period %q", tier, period)
}

// GetStoreCurrency fetches the store's billing currency ISO code (e.g. "USD",
// "GBP") from LemonSqueezy. Variant prices are denominated in this currency, so
// the catalog must format with it rather than a hardcoded "$".
func (c *lemonSqueezyClient) GetStoreCurrency(ctx context.Context) (string, error) {
	var env struct {
		Data struct {
			Attributes struct {
				Currency string `json:"currency"`
			} `json:"attributes"`
		} `json:"data"`
	}
	resp, err := c.client.R().SetContext(ctx).SetResult(&env).Get("/v1/stores/" + c.storeID)
	if err != nil {
		return "", fmt.Errorf("get store currency request failed: %w", err)
	}
	if resp.StatusCode() >= 400 {
		return "", fmt.Errorf("get store returned status %d: %s", resp.StatusCode(), resp.String())
	}
	return env.Data.Attributes.Currency, nil
}

// GetStoreID returns the configured LemonSqueezy store ID.
func (c *lemonSqueezyClient) GetStoreID() string {
	return c.storeID
}

// GetVariant fetches the variant and its current price-model relationship.
// Variant.price is deprecated and can diverge from the checkout price after a
// price edit; historical Price objects must not be used either.
func (c *lemonSqueezyClient) GetVariant(ctx context.Context, variantID string) (*VariantResponse, error) {
	var respEnvelope jsonAPIEnvelope
	resp, err := c.client.R().
		SetContext(ctx).
		SetResult(&respEnvelope).
		Get("/v1/variants/" + variantID)

	if err != nil {
		return nil, fmt.Errorf("get variant request failed: %w", err)
	}
	if resp.StatusCode() >= 400 {
		return nil, fmt.Errorf("get variant returned status %d: %s", resp.StatusCode(), resp.String())
	}

	var attrs variantAttributes
	if err := json.Unmarshal(respEnvelope.Data.Attributes, &attrs); err != nil {
		return nil, fmt.Errorf("unmarshal variant response: %w", err)
	}
	if respEnvelope.Data.ID != variantID || attrs.Status != "published" || attrs.TestMode {
		return nil, fmt.Errorf("variant %s is not a published live variant", variantID)
	}

	var priceEnvelope jsonAPIEnvelope
	priceResp, err := c.client.R().
		SetContext(ctx).
		SetResult(&priceEnvelope).
		Get("/v1/variants/" + variantID + "/price-model")
	if err != nil {
		return nil, fmt.Errorf("get current price request failed: %w", err)
	}
	if priceResp.StatusCode() >= 400 {
		return nil, fmt.Errorf("get current price returned status %d", priceResp.StatusCode())
	}
	if priceEnvelope.Data.Type != "prices" {
		return nil, fmt.Errorf("variant %s has no current Price object", variantID)
	}
	var price priceAttributes
	if err := json.Unmarshal(priceEnvelope.Data.Attributes, &price); err != nil {
		return nil, fmt.Errorf("unmarshal current price response: %w", err)
	}
	if fmt.Sprint(price.VariantID) != variantID || price.Category != "subscription" || price.Scheme != "standard" ||
		price.UsageAggregation != nil || price.UnitPrice == nil || *price.UnitPrice <= 0 ||
		price.SetupFeeEnabled || price.RenewalIntervalQuantity != 1 ||
		(price.RenewalIntervalUnit != "month" && price.RenewalIntervalUnit != "year") {
		return nil, fmt.Errorf("variant %s has an unsupported current Price model", variantID)
	}

	return &VariantResponse{
		ID:       respEnvelope.Data.ID,
		Name:     attrs.Name,
		Price:    *price.UnitPrice,
		Interval: price.RenewalIntervalUnit,
		Status:   attrs.Status,
	}, nil
}

// VariantMap returns a read-only copy of the configured tier:period → variant-ID map.
func (c *lemonSqueezyClient) VariantMap() VariantMap {
	out := make(VariantMap, len(c.variantMap))
	for k, v := range c.variantMap {
		out[k] = v
	}
	return out
}

// --- Subscription list types ---

// SubscriptionListItem is a single subscription in a list response.
type SubscriptionListItem struct {
	ID         string `json:"id"`
	Status     string `json:"status"`
	CustomerID int    `json:"customer_id"`
	ProductID  int    `json:"product_id"`
	VariantID  int    `json:"variant_id"`
	RenewsAt   string `json:"renews_at,omitempty"`
	EndsAt     string `json:"ends_at,omitempty"`
	CreatedAt  string `json:"created_at,omitempty"`
	Price      int    `json:"price,omitempty"` // price in cents
}

// SubscriptionListResponse wraps a paginated list of subscriptions.
type SubscriptionListResponse struct {
	Subscriptions []SubscriptionListItem `json:"subscriptions"`
	Total         int                    `json:"total"`
	Page          int                    `json:"page"`
	TotalPages    int                    `json:"total_pages"`
}

// ListSubscriptions retrieves a paginated list of subscriptions from LemonSqueezy.
func (c *lemonSqueezyClient) ListSubscriptions(ctx context.Context, storeID string, page int) (*SubscriptionListResponse, error) {
	if storeID == "" {
		storeID = c.storeID
	}

	var respEnvelope struct {
		Data []struct {
			ID         string          `json:"id"`
			Attributes json.RawMessage `json:"attributes"`
		} `json:"data"`
		Meta struct {
			Page struct {
				CurrentPage int `json:"currentPage"`
				LastPage    int `json:"lastPage"`
				Total       int `json:"total"`
			} `json:"page"`
		} `json:"meta"`
	}

	resp, err := c.client.R().
		SetContext(ctx).
		SetQueryParam("filter[store_id]", storeID).
		SetQueryParam("page[number]", fmt.Sprintf("%d", page)).
		SetQueryParam("page[size]", "20").
		SetResult(&respEnvelope).
		Get("/v1/subscriptions")

	if err != nil {
		return nil, fmt.Errorf("list subscriptions request failed: %w", err)
	}
	if resp.StatusCode() >= 400 {
		return nil, fmt.Errorf("list subscriptions returned status %d: %s", resp.StatusCode(), resp.String())
	}

	var items []SubscriptionListItem
	for _, d := range respEnvelope.Data {
		var attrs struct {
			Status     string `json:"status"`
			CustomerID int    `json:"customer_id"`
			ProductID  int    `json:"product_id"`
			VariantID  int    `json:"variant_id"`
			RenewsAt   string `json:"renews_at"`
			EndsAt     string `json:"ends_at"`
			CreatedAt  string `json:"created_at"`
		}
		if err := json.Unmarshal(d.Attributes, &attrs); err != nil {
			continue
		}
		items = append(items, SubscriptionListItem{
			ID:         d.ID,
			Status:     attrs.Status,
			CustomerID: attrs.CustomerID,
			ProductID:  attrs.ProductID,
			VariantID:  attrs.VariantID,
			RenewsAt:   attrs.RenewsAt,
			EndsAt:     attrs.EndsAt,
			CreatedAt:  attrs.CreatedAt,
		})
	}

	return &SubscriptionListResponse{
		Subscriptions: items,
		Total:         respEnvelope.Meta.Page.Total,
		Page:          respEnvelope.Meta.Page.CurrentPage,
		TotalPages:    respEnvelope.Meta.Page.LastPage,
	}, nil
}

// --- Invoice list types ---

// InvoiceItem represents a single invoice/order from LemonSqueezy.
type InvoiceItem struct {
	ID             string `json:"id"`
	CustomerID     int    `json:"customer_id"`
	SubscriptionID int    `json:"subscription_id,omitempty"`
	Status         string `json:"status"` // paid, refunded, pending, failed
	TotalFormatted string `json:"total_formatted"`
	Total          int    `json:"total"` // in cents
	Currency       string `json:"currency"`
	CreatedAt      string `json:"created_at"`
	RefundedAt     string `json:"refunded_at,omitempty"`
}

// InvoiceListResponse wraps a paginated list of invoices.
type InvoiceListResponse struct {
	Invoices   []InvoiceItem `json:"invoices"`
	Total      int           `json:"total"`
	Page       int           `json:"page"`
	TotalPages int           `json:"total_pages"`
}

// ListInvoices retrieves a paginated list of orders (invoices) from LemonSqueezy.
func (c *lemonSqueezyClient) ListInvoices(ctx context.Context, storeID string, page int, status string) (*InvoiceListResponse, error) {
	if storeID == "" {
		storeID = c.storeID
	}

	var respEnvelope struct {
		Data []struct {
			ID         string          `json:"id"`
			Attributes json.RawMessage `json:"attributes"`
		} `json:"data"`
		Meta struct {
			Page struct {
				CurrentPage int `json:"currentPage"`
				LastPage    int `json:"lastPage"`
				Total       int `json:"total"`
			} `json:"page"`
		} `json:"meta"`
	}

	req := c.client.R().
		SetContext(ctx).
		SetQueryParam("filter[store_id]", storeID).
		SetQueryParam("page[number]", fmt.Sprintf("%d", page)).
		SetQueryParam("page[size]", "20").
		SetResult(&respEnvelope)

	if status != "" {
		req.SetQueryParam("filter[status]", status)
	}

	resp, err := req.Get("/v1/orders")
	if err != nil {
		return nil, fmt.Errorf("list invoices request failed: %w", err)
	}
	if resp.StatusCode() >= 400 {
		return nil, fmt.Errorf("list invoices returned status %d: %s", resp.StatusCode(), resp.String())
	}

	var invoices []InvoiceItem
	for _, d := range respEnvelope.Data {
		var attrs struct {
			CustomerID     int    `json:"customer_id"`
			SubscriptionID int    `json:"first_subscription_id"`
			Status         string `json:"status"`
			TotalFormatted string `json:"total_formatted"`
			Total          int    `json:"total"`
			Currency       string `json:"currency"`
			CreatedAt      string `json:"created_at"`
			RefundedAt     string `json:"refunded_at"`
		}
		if err := json.Unmarshal(d.Attributes, &attrs); err != nil {
			continue
		}
		invoices = append(invoices, InvoiceItem{
			ID:             d.ID,
			CustomerID:     attrs.CustomerID,
			SubscriptionID: attrs.SubscriptionID,
			Status:         attrs.Status,
			TotalFormatted: attrs.TotalFormatted,
			Total:          attrs.Total,
			Currency:       attrs.Currency,
			CreatedAt:      attrs.CreatedAt,
			RefundedAt:     attrs.RefundedAt,
		})
	}

	return &InvoiceListResponse{
		Invoices:   invoices,
		Total:      respEnvelope.Meta.Page.Total,
		Page:       respEnvelope.Meta.Page.CurrentPage,
		TotalPages: respEnvelope.Meta.Page.LastPage,
	}, nil
}

// RefundInvoice issues a refund for a given order via the LemonSqueezy API.
func (c *lemonSqueezyClient) RefundInvoice(ctx context.Context, orderID string, amount int) error {
	// LemonSqueezy refunds are issued via POST /v1/refunds
	type refundAttrs struct {
		Amount int    `json:"amount"`
		Reason string `json:"reason,omitempty"`
	}
	attrs, err := json.Marshal(refundAttrs{Amount: amount, Reason: "admin_refund"})
	if err != nil {
		return fmt.Errorf("marshal refund attributes: %w", err)
	}

	rels := map[string]jsonAPIRelationship{
		"order": {Data: struct {
			Type string `json:"type"`
			ID   string `json:"id"`
		}{Type: "orders", ID: orderID}},
	}
	relsJSON, err := json.Marshal(rels)
	if err != nil {
		return fmt.Errorf("marshal refund relationships: %w", err)
	}

	body := jsonAPIEnvelope{
		Data: jsonAPIData{
			Type:          "refunds",
			Attributes:    attrs,
			Relationships: relsJSON,
		},
	}

	resp, err := c.client.R().
		SetContext(ctx).
		SetBody(body).
		Post("/v1/refunds")

	if err != nil {
		return fmt.Errorf("refund request failed: %w", err)
	}
	if resp.StatusCode() >= 400 {
		return fmt.Errorf("refund returned status %d: %s", resp.StatusCode(), resp.String())
	}

	return nil
}
