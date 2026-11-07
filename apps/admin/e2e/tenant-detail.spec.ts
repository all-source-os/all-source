import { expect, test } from "@playwright/test";

const TENANT_ID = "tenant-1";

const mockTenantDetail = {
  id: TENANT_ID,
  name: "Acme Corp",
  description: "Primary enterprise customer",
  plan: "pro",
  status: "active",
  events_count: 142_000,
  members_count: 3,
  created_at: "2025-06-15T10:30:00Z",
  quotas: {
    event_limit: 500_000,
    query_limit: 100_000,
    storage_limit_mb: 5_000,
  },
  members: [
    { id: "m-1", email: "alice@acme.co", role: "owner", joined_at: "2025-06-15T10:30:00Z" },
    { id: "m-2", email: "bob@acme.co", role: "admin", joined_at: "2025-07-01T09:00:00Z" },
    { id: "m-3", email: "carol@acme.co", role: "member", joined_at: "2025-08-12T14:22:00Z" },
  ],
  subscription: {
    plan: "pro",
    started_at: "2025-06-15T00:00:00Z",
    current_period_end: "2026-06-15T00:00:00Z",
  },
  audit_log: [
    {
      id: "a-1",
      action: "quotas.updated",
      actor: "admin@allsource.co",
      timestamp: "2026-03-01T12:00:00Z",
      details: "event_limit: 500000",
    },
    {
      id: "a-2",
      action: "member.added",
      actor: "alice@acme.co",
      timestamp: "2026-02-15T09:30:00Z",
      details: "carol@acme.co",
    },
  ],
};

const mockUsage = {
  event_count: 142_000,
  query_count: 45_000,
  storage_bytes: 1_200 * 1024 * 1024,
  event_limit: 500_000,
  query_limit: 100_000,
  storage_limit_mb: 5_000,
  daily_usage: Array.from({ length: 14 }, (_, i) => ({
    date: `2026-02-${String(22 + i).padStart(2, "0")}`,
    events_ingested: 8_000 + Math.floor(Math.random() * 4_000),
  })),
};

async function mockAuthRoute(page: import("@playwright/test").Page) {
  const b64 = (value: unknown) => Buffer.from(JSON.stringify(value)).toString("base64url");
  const token = `${b64({ alg: "HS256", typ: "JWT" })}.${b64({ sub: "admin-1", role: "admin", exp: Math.floor(Date.now() / 1000) + 3600 })}.test-only`;
  // proxy.ts redirects to /login unless an admin_token cookie decodes to role "admin".
  await page.context().addCookies([
    {
      name: "admin_token",
      value: token,
      domain: "localhost",
      path: "/",
      httpOnly: true,
      sameSite: "Lax",
    },
  ]);
  return page.route("**/api/auth/session", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        data: {
          user: { id: "admin-1", email: "admin@allsource.co", role: "admin" },
        },
      }),
    })
  );
}

test.describe("Tenant detail page", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuthRoute(page);
  });

  test("loads and displays tenant detail sections", async ({ page }) => {
    await page.route(`**/api/v1/admin/tenants/${TENANT_ID}`, (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(mockTenantDetail),
      })
    );
    await page.route(`**/api/v1/admin/tenants/${TENANT_ID}/usage`, (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(mockUsage),
      })
    );

    await page.goto(`/tenants/${TENANT_ID}`);

    // Page loaded
    await expect(page.getByTestId("tenant-detail-page")).toBeVisible();

    // Breadcrumb
    const breadcrumb = page.getByTestId("tenant-breadcrumb");
    await expect(breadcrumb).toBeVisible();
    await expect(breadcrumb).toContainText("Tenants");
    await expect(breadcrumb).toContainText("Acme Corp");

    // Info card
    await expect(page.getByTestId("tenant-info-card")).toBeVisible();
    await expect(page.getByTestId("tenant-status-badge")).toContainText("active");

    // Subscription card
    await expect(page.getByTestId("tenant-subscription-card")).toBeVisible();

    // Usage card with stats
    await expect(page.getByTestId("tenant-usage-card")).toBeVisible();
    await expect(page.getByTestId("stat-events-ingested")).toContainText("142,000");
    await expect(page.getByTestId("stat-queries-run")).toContainText("45,000");
    await expect(page.getByTestId("stat-storage-used")).toContainText("1,200");

    // Quota bars
    await expect(page.getByTestId("usage-bar-events")).toBeVisible();
    await expect(page.getByTestId("usage-bar-queries")).toBeVisible();
    await expect(page.getByTestId("usage-bar-storage")).toBeVisible();

    // Daily chart
    await expect(page.getByTestId("daily-events-chart")).toBeVisible();

    // Members
    await expect(page.getByTestId("tenant-members-card")).toBeVisible();
    await expect(page.getByTestId("member-row-m-1")).toContainText("alice@acme.co");
    await expect(page.getByTestId("member-row-m-2")).toContainText("bob@acme.co");
    await expect(page.getByTestId("member-row-m-3")).toContainText("carol@acme.co");

    // Audit log
    await expect(page.getByTestId("tenant-audit-card")).toBeVisible();
    await expect(page.getByTestId("audit-row-a-1")).toContainText("quotas.updated");
    await expect(page.getByTestId("audit-row-a-2")).toContainText("member.added");
  });

  test("edit quotas modal submits successfully", async ({ page }) => {
    let quotasPutBody: Record<string, unknown> | null = null;

    await page.route(`**/api/v1/admin/tenants/${TENANT_ID}`, (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(mockTenantDetail),
      })
    );
    await page.route(`**/api/v1/admin/tenants/${TENANT_ID}/usage`, (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(mockUsage),
      })
    );
    await page.route(`**/api/v1/admin/tenants/${TENANT_ID}/quotas`, async (route) => {
      quotasPutBody = route.request().postDataJSON();
      return route.fulfill({ status: 200, contentType: "application/json", body: "{}" });
    });

    await page.goto(`/tenants/${TENANT_ID}`);
    await expect(page.getByTestId("tenant-detail-page")).toBeVisible();

    // Open dialog
    await page.getByTestId("ops-edit-quotas-btn").click();
    await expect(page.getByTestId("edit-quotas-dialog")).toBeVisible();

    // Update values
    const eventInput = page.getByTestId("quota-event-limit");
    await eventInput.clear();
    await eventInput.fill("750000");

    const queryInput = page.getByTestId("quota-query-limit");
    await queryInput.clear();
    await queryInput.fill("200000");

    const storageInput = page.getByTestId("quota-storage-limit");
    await storageInput.clear();
    await storageInput.fill("10000");

    // Save
    await page.getByTestId("quota-save-btn").click();

    // Dialog closes
    await expect(page.getByTestId("edit-quotas-dialog")).not.toBeVisible();

    // Verify PUT body
    expect(quotasPutBody).toEqual({
      event_limit: 750000,
      query_limit: 200000,
      storage_limit_mb: 10000,
    });
  });

  test("suspend toggles tenant status", async ({ page }) => {
    let suspendCalled = false;

    await page.route(`**/api/v1/admin/tenants/${TENANT_ID}`, (route) => {
      // After suspend, return suspended status
      const detail = suspendCalled
        ? { ...mockTenantDetail, status: "suspended" }
        : mockTenantDetail;
      return route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(detail),
      });
    });
    await page.route(`**/api/v1/admin/tenants/${TENANT_ID}/usage`, (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(mockUsage),
      })
    );
    await page.route(`**/api/v1/admin/tenants/${TENANT_ID}/suspend`, (route) => {
      suspendCalled = true;
      return route.fulfill({ status: 200, contentType: "application/json", body: "{}" });
    });

    await page.goto(`/tenants/${TENANT_ID}`);
    await expect(page.getByTestId("tenant-detail-page")).toBeVisible();

    // Status should be active
    await expect(page.getByTestId("tenant-status-badge")).toContainText("active");

    // Open suspend dialog
    await page.getByTestId("ops-suspend-btn").click();
    await expect(page.getByTestId("suspend-dialog")).toBeVisible();
    await expect(page.getByTestId("suspend-dialog")).toContainText("Acme Corp");

    // Confirm
    await page.getByTestId("suspend-confirm-btn").click();

    // Dialog closes and status updates
    await expect(page.getByTestId("suspend-dialog")).not.toBeVisible();
    await expect(page.getByTestId("tenant-status-badge")).toContainText("suspended");

    // Button should now say "Unsuspend"
    await expect(page.getByTestId("ops-suspend-btn")).toContainText("Unsuspend");
  });

  test("client keys: list, register a public JWK, revoke with confirmation", async ({ page }) => {
    const activeKid = "kid-active-1";
    const newKid = "kid-new-2";
    const jwk = {
      kty: "EC",
      crv: "P-256",
      x: "f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU",
      y: "x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0",
    };
    const keys: Record<string, unknown>[] = [
      {
        kid: activeKid,
        client_id: "billing-worker",
        role: "serviceaccount",
        scopes: ["events:read"],
        revoked: false,
        invalid: false,
        active: true,
        created_at: "2026-11-01T10:00:00Z",
        created_by: "admin-1",
        jwk,
      },
    ];
    let registerBody: Record<string, unknown> | null = null;
    let revokedKid: string | null = null;

    await page.route(`**/api/v1/admin/tenants/${TENANT_ID}`, (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(mockTenantDetail),
      })
    );
    await page.route(`**/api/v1/admin/tenants/${TENANT_ID}/usage`, (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(mockUsage),
      })
    );
    await page.route(`**/api/v1/admin/tenants/${TENANT_ID}/client-keys`, async (route) => {
      if (route.request().method() === "POST") {
        registerBody = route.request().postDataJSON();
        keys.push({
          kid: newKid,
          client_id: "sync-agent",
          role: "serviceaccount",
          scopes: ["events:read", "events:write"],
          revoked: false,
          invalid: false,
          active: true,
          created_at: "2026-11-07T10:00:00Z",
          created_by: "admin-1",
          jwk,
        });
        return route.fulfill({
          status: 201,
          contentType: "application/json",
          body: JSON.stringify({
            kid: newKid,
            tenant_id: TENANT_ID,
            client_id: "sync-agent",
            alg: "ES256",
            role: "serviceaccount",
            scopes: ["events:read", "events:write"],
          }),
        });
      }
      return route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ tenant_id: TENANT_ID, client_keys: keys }),
      });
    });
    await page.route(`**/api/v1/admin/tenants/${TENANT_ID}/client-keys/*`, (route) => {
      revokedKid = decodeURIComponent(route.request().url().split("/").pop() ?? "");
      const key = keys.find((k) => k.kid === revokedKid);
      if (key) {
        key.revoked = true;
        key.active = false;
        key.revoked_at = "2026-11-07T11:00:00Z";
        key.revoked_by = "admin-1";
      }
      return route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          kid: revokedKid,
          tenant_id: TENANT_ID,
          revoked_at: "2026-11-07T11:00:00Z",
        }),
      });
    });

    await page.goto(`/tenants/${TENANT_ID}`);
    await expect(page.getByTestId("tenant-client-keys-card")).toBeVisible();

    const activeRow = page.getByTestId(`client-key-row-${activeKid}`);
    await expect(activeRow).toContainText("billing-worker");
    await expect(activeRow).toContainText("serviceaccount");
    await expect(activeRow).toContainText("events:read");
    await expect(page.getByTestId(`client-key-state-${activeKid}`)).toContainText("active");

    // A private JWK is refused in the browser and never posted.
    await page.getByTestId("client-key-client-id").fill("sync-agent");
    await page.getByTestId("client-key-jwk").fill(JSON.stringify({ ...jwk, d: "private" }));
    await page.getByTestId("client-key-register-btn").click();
    await expect(page.getByTestId("client-key-register-error")).toContainText("private JWK");
    expect(registerBody).toBeNull();

    await page.getByTestId("client-key-jwk").fill(JSON.stringify(jwk));
    await page.getByTestId("client-key-scope-events:write").click();
    await page.getByTestId("client-key-register-btn").click();
    await expect(page.getByTestId("client-key-register-success")).toContainText(newKid);
    expect(registerBody).toEqual({
      client_id: "sync-agent",
      jwk,
      scopes: ["events:read", "events:write"],
    });
    await expect(page.getByTestId(`client-key-row-${newKid}`)).toContainText("sync-agent");

    await page.getByTestId(`client-key-revoke-btn-${activeKid}`).click();
    await expect(page.getByTestId("client-key-revoke-dialog")).toContainText(activeKid);
    await page.getByTestId("client-key-revoke-cancel-btn").click();
    await expect(page.getByTestId("client-key-revoke-dialog")).not.toBeVisible();
    expect(revokedKid).toBeNull();

    await page.getByTestId(`client-key-revoke-btn-${activeKid}`).click();
    await page.getByTestId("client-key-revoke-confirm-btn").click();
    await expect(page.getByTestId("client-key-revoke-dialog")).not.toBeVisible();
    expect(revokedKid).toBe(activeKid);
    await expect(page.getByTestId(`client-key-state-${activeKid}`)).toContainText("revoked");
    await expect(page.getByTestId(`client-key-revoke-btn-${activeKid}`)).toHaveCount(0);
  });

  test("client keys: an unavailable authority shows the error with a retry", async ({ page }) => {
    await page.route(`**/api/v1/admin/tenants/${TENANT_ID}`, (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(mockTenantDetail),
      })
    );
    await page.route(`**/api/v1/admin/tenants/${TENANT_ID}/usage`, (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(mockUsage),
      })
    );
    await page.route(`**/api/v1/admin/tenants/${TENANT_ID}/client-keys`, (route) =>
      route.fulfill({
        status: 503,
        contentType: "application/json",
        body: JSON.stringify({
          error: "authority_unavailable",
          message: "client key authority is unavailable; retry shortly",
        }),
      })
    );

    await page.goto(`/tenants/${TENANT_ID}`);
    await expect(page.getByTestId("client-keys-error")).toContainText(
      "client key authority is unavailable; retry shortly"
    );
    await expect(page.getByTestId("client-keys-retry-btn")).toBeVisible();
  });

  test("unsuspend toggles suspended tenant back to active", async ({ page }) => {
    let unsuspendCalled = false;
    const suspendedTenant = { ...mockTenantDetail, status: "suspended" };

    await page.route(`**/api/v1/admin/tenants/${TENANT_ID}`, (route) => {
      const detail = unsuspendCalled ? { ...mockTenantDetail, status: "active" } : suspendedTenant;
      return route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(detail),
      });
    });
    await page.route(`**/api/v1/admin/tenants/${TENANT_ID}/usage`, (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(mockUsage),
      })
    );
    await page.route(`**/api/v1/admin/tenants/${TENANT_ID}/unsuspend`, (route) => {
      unsuspendCalled = true;
      return route.fulfill({ status: 200, contentType: "application/json", body: "{}" });
    });

    await page.goto(`/tenants/${TENANT_ID}`);
    await expect(page.getByTestId("tenant-detail-page")).toBeVisible();

    // Should show "Unsuspend" button for suspended tenant
    await expect(page.getByTestId("ops-suspend-btn")).toContainText("Unsuspend");
    await expect(page.getByTestId("tenant-status-badge")).toContainText("suspended");

    // Click unsuspend
    await page.getByTestId("ops-suspend-btn").click();
    await expect(page.getByTestId("suspend-dialog")).toBeVisible();
    await page.getByTestId("suspend-confirm-btn").click();

    // Should revert to active
    await expect(page.getByTestId("suspend-dialog")).not.toBeVisible();
    await expect(page.getByTestId("tenant-status-badge")).toContainText("active");
  });
});
