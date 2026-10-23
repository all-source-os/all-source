package clients

import (
	"context"
	"encoding/json"
	"fmt"
	"net/url"
)

// PrepaidOrder combines canonical order and order-item values used to validate credit.
type PrepaidOrder struct {
	StoreID        int    `json:"store_id"`
	Currency       string `json:"currency"`
	Status         string `json:"status"`
	Subtotal       int    `json:"subtotal"`
	DiscountTotal  int    `json:"discount_total"`
	SetupFee       int    `json:"setup_fee"`
	RefundedAmount int    `json:"refunded_amount"`
	TestMode       bool   `json:"test_mode"`
	Item           struct {
		ID        int `json:"id"`
		OrderID   int `json:"order_id"`
		VariantID int `json:"variant_id"`
		Price     int `json:"price"`
		Quantity  int `json:"quantity"`
	} `json:"first_order_item"`
}

func (c *lemonSqueezyClient) GetPrepaidOrder(ctx context.Context, orderID string) (*PrepaidOrder, error) {
	var envelope jsonAPIEnvelope
	response, err := c.client.R().SetContext(ctx).SetResult(&envelope).Get("/v1/orders/" + url.PathEscape(orderID))
	if err != nil || !response.IsSuccess() {
		return nil, fmt.Errorf("prepaid order lookup failed")
	}
	if envelope.Data.ID != orderID || envelope.Data.Type != "orders" {
		return nil, fmt.Errorf("prepaid order identity mismatch")
	}
	var order PrepaidOrder
	if err := json.Unmarshal(envelope.Data.Attributes, &order); err != nil {
		return nil, err
	}
	if order.Item.ID <= 0 {
		return nil, fmt.Errorf("prepaid order item missing")
	}
	var item jsonAPIEnvelope
	response, err = c.client.R().SetContext(ctx).SetResult(&item).Get(fmt.Sprintf("/v1/order-items/%d", order.Item.ID))
	if err != nil || !response.IsSuccess() {
		return nil, fmt.Errorf("prepaid order item lookup failed")
	}
	if item.Data.ID != fmt.Sprint(order.Item.ID) || item.Data.Type != "order-items" {
		return nil, fmt.Errorf("prepaid order item mismatch")
	}
	var current PrepaidOrder
	if err := json.Unmarshal(item.Data.Attributes, &current.Item); err != nil {
		return nil, err
	}
	order.Item = current.Item
	if fmt.Sprint(order.Item.OrderID) != orderID {
		return nil, fmt.Errorf("prepaid order ownership mismatch")
	}
	return &order, nil
}
