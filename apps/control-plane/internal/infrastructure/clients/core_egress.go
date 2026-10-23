package clients

import (
	"context"
	"net/url"
)

func (c *coreClient) ChangeEgress(ctx context.Context, tenant string, command map[string]any) error {
	_, span := c.startSpan(ctx, "ChangeEgress")
	defer span.End()
	response, err := c.request(ctx).SetBody(command).Post("/api/v1/tenants/" + url.PathEscape(tenant) + "/egress")
	return c.handleError(span, response, err)
}
