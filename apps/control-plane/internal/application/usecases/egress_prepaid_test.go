package usecases

import (
	"context"
	"fmt"
	"testing"

	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

type prepaidTestCore struct {
	clients.CoreClient
	values   map[string]any
	credited map[string]uint64
	refunded map[string]bool
	tenant   string
}

func (c *prepaidTestCore) SetConfig(_ context.Context, request clients.SetConfigRequest) error {
	c.values[request.Key] = request.Value
	return nil
}
func (c *prepaidTestCore) GetConfig(_ context.Context, key string) (*clients.ConfigEntryResponse, error) {
	v, ok := c.values[key]
	if !ok {
		return nil, fmt.Errorf("missing binding")
	}
	return &clients.ConfigEntryResponse{Value: v}, nil
}
func (c *prepaidTestCore) ChangeEgress(_ context.Context, tenant string, command map[string]any) error {
	c.tenant = tenant
	id, validID := command["payment_id"].(string)
	bytes, validBytes := command["bytes"].(uint64)
	refunded, validRefund := command["refunded"].(bool)
	if !validID || !validBytes || !validRefund {
		return fmt.Errorf("invalid ledger command")
	}
	c.credited[id] = bytes
	c.refunded[id] = c.refunded[id] || refunded
	return nil
}

type prepaidTestProvider struct {
	clients.LemonSqueezyClient
	custom map[string]string
	price  int
	order  clients.PrepaidOrder
}

func (p *prepaidTestProvider) GetStoreCurrency(context.Context) (string, error) { return "GBP", nil }
func (p *prepaidTestProvider) GetStoreID() string                               { return "5" }
func (p *prepaidTestProvider) GetPrepaidVariant(_ context.Context, id string) (*clients.VariantResponse, error) {
	return &clients.VariantResponse{ID: id, Price: p.price}, nil
}
func (p *prepaidTestProvider) CreateCheckout(_ context.Context, r clients.CreateCheckoutRequest) (*clients.CheckoutResponse, error) {
	p.custom = r.CustomData
	return &clients.CheckoutResponse{URL: "https://checkout.example.test/prepaid"}, nil
}
func (p *prepaidTestProvider) GetPrepaidOrder(context.Context, string) (*clients.PrepaidOrder, error) {
	return &p.order, nil
}

func prepaidFixture(t *testing.T) (*EgressPrepaid, *prepaidTestCore, *prepaidTestProvider, LemonSqueezyWebhookEvent) {
	t.Helper()
	core := &prepaidTestCore{values: map[string]any{}, credited: map[string]uint64{}, refunded: map[string]bool{}}
	provider := &prepaidTestProvider{price: 1000}
	provider.order.StoreID = 5
	provider.order.Currency = "GBP"
	provider.order.Status = "paid"
	provider.order.Subtotal = 1000
	provider.order.Item.VariantID = 123
	provider.order.Item.Price = 1000
	provider.order.Item.Quantity = 1
	service, err := NewEgressPrepaid(core, provider, `{"100gb":"123"}`)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := service.Checkout(context.Background(), "owner-tenant", "100gb", ""); err != nil {
		t.Fatal(err)
	}
	event := LemonSqueezyWebhookEvent{EventName: "order_created", Data: LemonSqueezyEventData{ID: "42", Type: "orders"}, Meta: map[string]interface{}{"custom_data": map[string]interface{}{"egress_checkout": provider.custom["egress_checkout"], "tenant_id": "forged-tenant"}}}
	return service, core, provider, event
}

func TestEgressPrepaidUsesBoundTenantAndCanonicalPaidOrder(t *testing.T) {
	service, core, provider, event := prepaidFixture(t)
	if err := service.Process(context.Background(), event); err != nil {
		t.Fatal(err)
	}
	if core.tenant != "owner-tenant" || core.credited["lemonsqueezy:5:42"] != 100_000_000_000 {
		t.Fatalf("wrong prepaid attribution: %#v", core)
	}
	service.variants["100gb"] = "456"
	provider.order.Status = "refunded"
	if err := service.Process(context.Background(), event); err != nil {
		t.Fatal(err)
	}
	if !core.refunded["lemonsqueezy:5:42"] {
		t.Fatal("stale paid webhook ignored canonical refund")
	}
}

func TestEgressPrepaidRejectsUnfundedOrMismatchedOrders(t *testing.T) {
	for _, test := range []struct {
		name   string
		mutate func(*clients.PrepaidOrder)
	}{
		{"pending", func(o *clients.PrepaidOrder) { o.Status = "pending" }},
		{"test-mode", func(o *clients.PrepaidOrder) { o.TestMode = true }},
		{"other-store", func(o *clients.PrepaidOrder) { o.StoreID = 6 }},
		{"other-variant", func(o *clients.PrepaidOrder) { o.Item.VariantID = 456 }},
		{"underpaid", func(o *clients.PrepaidOrder) { o.Subtotal = 1 }},
		{"wrong-currency", func(o *clients.PrepaidOrder) { o.Currency = "USD" }},
	} {
		t.Run(test.name, func(t *testing.T) {
			s, c, p, e := prepaidFixture(t)
			test.mutate(&p.order)
			if err := s.Process(context.Background(), e); err == nil {
				t.Fatal("invalid order credited")
			}
			if len(c.credited) != 0 {
				t.Fatal("rejected order changed credit")
			}
		})
	}
}

func TestEgressPrepaidCheckoutRejectsUnverifiedLivePrice(t *testing.T) {
	s, c, p, _ := prepaidFixture(t)
	p.price = 1
	count := len(c.values)
	if _, err := s.Checkout(context.Background(), "owner-tenant", "100gb", ""); err == nil {
		t.Fatal("unverified price accepted")
	}
	if len(c.values) != count {
		t.Fatal("rejected price created checkout")
	}
}
