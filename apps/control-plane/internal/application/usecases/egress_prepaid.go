package usecases

import (
	"context"
	"encoding/json"
	"fmt"
	"strconv"
	"time"

	"github.com/google/uuid"

	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// EgressPack defines a server-priced product, never a client-supplied byte grant.
type EgressPack struct {
	ID       string `json:"id"`
	Bytes    uint64 `json:"bytes"`
	Currency string `json:"currency"`
	Cents    int    `json:"cents"`
}

// EgressPacks are proposed products; checkout additionally requires verified live variants.
var EgressPacks = []EgressPack{
	{ID: "100gb", Bytes: 100_000_000_000, Currency: currencyGBP, Cents: 1000},
	{ID: "1tb", Bytes: 1_000_000_000_000, Currency: currencyGBP, Cents: 7500},
}

type prepaidProvider interface {
	clients.LemonSqueezyClient
	GetPrepaidVariant(context.Context, string) (*clients.VariantResponse, error)
	GetPrepaidOrder(context.Context, string) (*clients.PrepaidOrder, error)
}

type prepaidLedger interface {
	ChangeEgress(context.Context, string, map[string]any) error
}

// EgressPrepaid binds verified provider orders to the canonical tenant ledger.
type EgressPrepaid struct {
	core     clients.CoreClient
	provider prepaidProvider
	variants map[string]string
}

type egressCheckoutBinding struct {
	Tenant   string `json:"tenant_id"`
	Pack     string `json:"pack"`
	Variant  string `json:"variant_id"`
	Bytes    uint64 `json:"bytes"`
	Currency string `json:"currency"`
	Cents    int    `json:"cents"`
}

// NewEgressPrepaid rejects unknown or duplicate product mappings before serving requests.
func NewEgressPrepaid(core clients.CoreClient, provider clients.LemonSqueezyClient, variantsJSON string) (*EgressPrepaid, error) {
	service := &EgressPrepaid{core: core, variants: map[string]string{}}
	if typed, ok := provider.(prepaidProvider); ok {
		service.provider = typed
	}
	if variantsJSON != "" {
		if err := json.Unmarshal([]byte(variantsJSON), &service.variants); err != nil {
			return nil, fmt.Errorf("invalid egress variant map")
		}
	}
	seen := map[string]bool{}
	for pack, variant := range service.variants {
		if _, ok := egressPack(pack); !ok {
			return nil, fmt.Errorf("unknown egress pack")
		}
		id, err := strconv.ParseUint(variant, 10, 64)
		if err != nil || id == 0 || seen[variant] {
			return nil, fmt.Errorf("invalid or duplicate egress variant")
		}
		seen[variant] = true
	}
	return service, nil
}

func egressPack(id string) (EgressPack, bool) {
	for _, pack := range EgressPacks {
		if pack.ID == id {
			return pack, true
		}
	}
	return EgressPack{}, false
}

// Checkout durably binds ownership before creating the provider checkout.
func (s *EgressPrepaid) Checkout(ctx context.Context, tenant, packID, email string) (*clients.CheckoutResponse, error) {
	pack, ok := egressPack(packID)
	variant := s.variants[packID]
	if !ok || variant == "" || s.provider == nil {
		return nil, fmt.Errorf("prepaid egress checkout unavailable")
	}
	ctx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	currency, err := s.provider.GetStoreCurrency(ctx)
	if err != nil || currency != pack.Currency {
		return nil, fmt.Errorf("prepaid currency not verified")
	}
	price, err := s.provider.GetPrepaidVariant(ctx, variant)
	if err != nil || price == nil || price.Price != pack.Cents || price.Interval != "" {
		return nil, fmt.Errorf("prepaid price not verified")
	}
	nonce := uuid.NewString()
	binding := map[string]any{"tenant_id": tenant, "pack": packID, "variant_id": variant, "bytes": pack.Bytes, "currency": pack.Currency, "cents": pack.Cents}
	if err := s.core.SetConfig(ctx, clients.SetConfigRequest{Key: "billing:egress-checkout:" + nonce, Value: binding, ChangedBy: "billing"}); err != nil {
		return nil, fmt.Errorf("prepaid checkout binding unavailable")
	}
	return s.provider.CreateCheckout(ctx, clients.CreateCheckoutRequest{
		VariantID: variant, Email: email,
		CustomData: map[string]string{"tenant_id": tenant, "egress_checkout": nonce},
	})
}

// Process requires an HMAC-verified webhook and rechecks the current provider order.
func (s *EgressPrepaid) Process(ctx context.Context, event LemonSqueezyWebhookEvent) error {
	custom, ok := event.Meta["custom_data"].(map[string]interface{})
	if !ok {
		return fmt.Errorf("invalid prepaid checkout metadata")
	}
	nonce, ok := custom["egress_checkout"].(string)
	if !ok {
		return fmt.Errorf("invalid prepaid checkout nonce")
	}
	if _, err := uuid.Parse(nonce); err != nil {
		return fmt.Errorf("invalid prepaid checkout binding")
	}
	if s.provider == nil || event.Data.Type != "orders" || (event.EventName != "order_created" && event.EventName != "order_refunded") {
		return fmt.Errorf("prepaid payment processing unavailable")
	}
	ctx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	entry, err := s.core.GetConfig(ctx, "billing:egress-checkout:"+nonce)
	if err != nil || entry == nil {
		return fmt.Errorf("prepaid checkout binding unavailable")
	}
	encoded, err := json.Marshal(entry.Value)
	if err != nil {
		return fmt.Errorf("invalid prepaid checkout binding")
	}
	var binding egressCheckoutBinding
	if err := json.Unmarshal(encoded, &binding); err != nil {
		return fmt.Errorf("invalid prepaid binding fields")
	}
	if binding.Tenant == "" || binding.Variant == "" || binding.Pack == "" || binding.Bytes == 0 || binding.Bytes > 1_000_000_000_000 || binding.Currency != currencyGBP || binding.Cents <= 0 {
		return fmt.Errorf("prepaid checkout product mismatch")
	}
	order, err := s.provider.GetPrepaidOrder(ctx, event.Data.ID)
	if err != nil || order == nil {
		return fmt.Errorf("prepaid order verification unavailable")
	}
	if order.TestMode || fmt.Sprint(order.StoreID) != s.provider.GetStoreID() || order.Currency != binding.Currency ||
		fmt.Sprint(order.Item.VariantID) != binding.Variant || order.Item.Price != binding.Cents || order.Item.Quantity < 1 || order.Item.Quantity > 100 ||
		order.Subtotal != binding.Cents*order.Item.Quantity || order.DiscountTotal != 0 || order.SetupFee != 0 {
		return fmt.Errorf("prepaid order does not match funded product")
	}
	refunded := order.Status == "refunded" || order.Status == "partial_refund" || order.Status == "fraudulent" || order.RefundedAmount > 0
	if order.Status != "paid" && !refunded {
		return fmt.Errorf("prepaid order is not settled")
	}
	sink, ok := s.core.(prepaidLedger)
	if !ok {
		return fmt.Errorf("durable prepaid ledger unavailable")
	}
	return sink.ChangeEgress(ctx, binding.Tenant, map[string]any{
		"operation": "payment", "payment_id": "lemonsqueezy:" + s.provider.GetStoreID() + ":" + event.Data.ID,
		"bytes": binding.Bytes * uint64(order.Item.Quantity), "refunded": refunded,
	})
}
