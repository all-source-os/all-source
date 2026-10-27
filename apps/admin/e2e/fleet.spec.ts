import { expect, test } from "@playwright/test";

// ── Fixtures matching the documented §2.2 / §5.1 response shapes ───────

const TENANT_ID = "tenant-acme";

const mockFleetHealth = {
  _links: {
    self: "/api/v1/admin/fleet/health",
    tenant_template: "/api/v1/admin/fleet/health/{id}",
  },
  summary: { total: 10, healthy: 6, degraded: 2, at_risk: 1, critical: 1 },
  worst: [
    {
      tenant_id: TENANT_ID,
      name: "Acme Corp",
      tier: "critical",
      reasons: [
        { signal: "subscription_state", value: "canceled", tier: "critical" },
        { signal: "events_quota_pct", value: "118%", tier: "at_risk" },
      ],
    },
    {
      tenant_id: "tenant-globex",
      name: "Globex",
      tier: "at_risk",
      reasons: [{ signal: "last_sync_age", value: "31h", tier: "at_risk" }],
    },
    {
      tenant_id: "tenant-umbrella",
      name: "Umbrella",
      tier: "degraded",
      reasons: [{ signal: "subscription_state", value: "past_due", tier: "degraded" }],
    },
  ],
};

const mockTenantHealth = {
  tenant_id: TENANT_ID,
  name: "Acme Corp",
  tier: "critical",
  signals: [
    {
      signal: "subscription_state",
      value: "canceled",
      tier: "critical",
      source: "Core tenant metadata",
    },
    {
      signal: "events_quota_pct",
      value: "118%",
      tier: "at_risk",
      source: "Core tenant metadata",
    },
    {
      signal: "last_event_age",
      value: "2h",
      tier: "healthy",
      source: "Core (desc probe)",
    },
  ],
  subscription: {
    tier: "pro",
    status: "canceled",
    renews_at: "2026-05-01T00:00:00Z",
    grandfathered: false,
    events_used: 590_000,
    events_quota: 500_000,
  },
};

function mockAuthRoute(page: import("@playwright/test").Page) {
  return page.route("**/api/auth/session", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        data: { user: { id: "admin-1", email: "admin@allsource.co", role: "admin" } },
      }),
    })
  );
}

/**
 * Build a decode-only admin JWT (the proxy middleware + auth.ts only `decodeJwt`
 * it, never verifying the signature) and seat it in the `admin_token` cookie so
 * the server-side proxy (src/proxy.ts) lets the authenticated routes through.
 */
function adminJwt(): string {
  const b64 = (o: unknown) => Buffer.from(JSON.stringify(o)).toString("base64url");
  const now = Math.floor(Date.now() / 1000);
  const header = b64({ alg: "HS256", typ: "JWT" });
  const payload = b64({
    sub: "admin-1",
    email: "admin@allsource.co",
    name: "Admin",
    role: "admin",
    iat: now,
    exp: now + 3600 * 24,
  });
  return `${header}.${payload}.test-signature`;
}

async function seatAdminCookie(context: import("@playwright/test").BrowserContext) {
  await context.addCookies([
    {
      name: "admin_token",
      value: adminJwt(),
      domain: "localhost",
      path: "/",
      httpOnly: true,
      sameSite: "Lax",
    },
  ]);
}

test.describe("Fleet health overview", () => {
  test.beforeEach(async ({ page, context }) => {
    await seatAdminCookie(context);
    await mockAuthRoute(page);
    await page.route("**/api/v1/admin/fleet/health?*", (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(mockFleetHealth),
      })
    );
  });

  test("renders four tier stat cards + worst-N table with colored chips", async ({ page }) => {
    await page.goto("/fleet");

    await expect(page.getByTestId("fleet-page")).toBeVisible();

    // Four StatCard counts (stat-card-<title>)
    await expect(page.getByTestId("stat-card-healthy")).toContainText("6");
    await expect(page.getByTestId("stat-card-degraded")).toContainText("2");
    await expect(page.getByTestId("stat-card-at-risk")).toContainText("1");
    await expect(page.getByTestId("stat-card-critical")).toContainText("1");

    // Worst-N table with per-tenant rows
    await expect(page.getByTestId("fleet-worst-table")).toBeVisible();
    await expect(page.getByTestId(`fleet-row-${TENANT_ID}`)).toContainText("Acme Corp");

    const criticalChip = page.getByTestId("health-chip-critical").first();
    await expect(criticalChip).toBeVisible();
    await expect(criticalChip.locator("span.bg-red-500")).toHaveCount(1);

    // Degraded row → yellow dot in a reason.
    await expect(page.getByTestId("health-chip-degraded").first()).toBeVisible();

    // Contributing-reason rows are present.
    await expect(page.getByTestId(`fleet-reason-${TENANT_ID}-subscription_state`)).toContainText(
      "canceled"
    );
  });
});

test.describe("Fleet tenant drill-down", () => {
  test.beforeEach(async ({ page, context }) => {
    await seatAdminCookie(context);
    await mockAuthRoute(page);
    await page.route(`**/api/v1/admin/fleet/health/${TENANT_ID}`, (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(mockTenantHealth),
      })
    );
  });

  test("lists every signal with value, tier, and source", async ({ page }) => {
    await page.goto(`/fleet/${TENANT_ID}`);

    await expect(page.getByTestId("fleet-tenant-page")).toBeVisible();

    // Signals table renders one row per signal.
    await expect(page.getByTestId("fleet-signals-table")).toBeVisible();
    const sub = page.getByTestId("fleet-signal-subscription_state");
    await expect(sub).toContainText("subscription_state");
    await expect(sub).toContainText("canceled");
    await expect(sub).toContainText("Core tenant metadata");

    await expect(page.getByTestId("fleet-signal-events_quota_pct")).toContainText("118%");
    await expect(page.getByTestId("fleet-signal-last_event_age")).toContainText("2h");

    // Subscription block + quota bar.
    await expect(page.getByTestId("fleet-subscription-card")).toBeVisible();
    await expect(page.getByTestId("fleet-sub-status")).toContainText("canceled");
    await expect(page.getByTestId("fleet-quota-bar")).toBeVisible();

    // Links back to tenant detail + billing.
    await expect(page.getByTestId("fleet-link-tenant")).toBeVisible();
    await expect(page.getByTestId("fleet-link-billing")).toBeVisible();

    // Recovery console present.
    await expect(page.getByTestId("fleet-recovery-console")).toBeVisible();
  });

  test("GUARD: Destructive dialog keeps Apply disabled until preview shown AND typed id matches", async ({
    page,
  }) => {
    let dryRunCalled = false;
    let applyBody: Record<string, unknown> | null = null;

    // Mock the reprovision endpoint: dry-run returns a `would` preview;
    // a real apply records the body.
    await page.route(`**/api/v1/admin/recovery/${TENANT_ID}/reprovision`, async (route) => {
      const body = route.request().postDataJSON();
      if (body.dry_run) {
        dryRunCalled = true;
        return route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            dry_run: true,
            would: { metadata_diff: { status: "active → reprovisioned" } },
          }),
        });
      }
      applyBody = body;
      return route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ applied: true }),
      });
    });

    await page.goto(`/fleet/${TENANT_ID}`);
    await expect(page.getByTestId("fleet-tenant-page")).toBeVisible();

    // Open the Destructive "Reprovision" action.
    await page.getByTestId("recovery-action-reprovision").click();
    await expect(page.getByTestId("recovery-dialog")).toBeVisible();
    await expect(page.getByTestId("recovery-destructive-warning")).toBeVisible();

    const applyBtn = page.getByTestId("recovery-apply-btn");

    // GUARD A: Apply disabled before any preview.
    await expect(applyBtn).toBeDisabled();

    // The typed-id input is itself locked until a preview is shown — the guard
    // is even stronger than "Apply disabled": you cannot confirm before previewing.
    await expect(page.getByTestId("recovery-confirm-input")).toBeDisabled();

    // Run the dry-run preview.
    await page.getByTestId("recovery-preview-btn").click();
    await expect(page.getByTestId("recovery-would-payload")).toBeVisible();
    expect(dryRunCalled).toBe(true);

    // GUARD B: preview shown but id still empty → Apply disabled.
    await expect(applyBtn).toBeDisabled();

    // Preview shown but id mismatched → still disabled.
    await page.getByTestId("recovery-confirm-input").fill("wrong-id");
    await expect(applyBtn).toBeDisabled();

    // Both guards satisfied (preview shown AND id matches) → Apply enabled.
    await page.getByTestId("recovery-confirm-input").fill(TENANT_ID);
    await expect(applyBtn).toBeEnabled();

    // Apply forwards the typed-name confirmation to the server.
    await applyBtn.click();
    await expect(page.getByTestId("recovery-dialog")).not.toBeVisible();
    expect(applyBody).toMatchObject({
      dry_run: false,
      confirm_tenant_id: TENANT_ID,
    });
  });

  test("Guarded action only needs a preview before Apply", async ({ page }) => {
    await page.route(`**/api/v1/admin/recovery/${TENANT_ID}/resync`, async (route) => {
      const body = route.request().postDataJSON();
      return route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(
          body.dry_run ? { dry_run: true, would: { events_to_pull: 1240 } } : { applied: true }
        ),
      });
    });

    await page.goto(`/fleet/${TENANT_ID}`);
    await expect(page.getByTestId("fleet-tenant-page")).toBeVisible();

    await page.getByTestId("recovery-action-resync").click();
    await expect(page.getByTestId("recovery-dialog")).toBeVisible();

    // No destructive warning, no typed-id input for a Guarded action.
    await expect(page.getByTestId("recovery-destructive-warning")).toHaveCount(0);
    await expect(page.getByTestId("recovery-confirm-input")).toHaveCount(0);

    const applyBtn = page.getByTestId("recovery-apply-btn");
    await expect(applyBtn).toBeDisabled();

    await page.getByTestId("recovery-preview-btn").click();
    await expect(page.getByTestId("recovery-would-payload")).toBeVisible();

    // After preview, Apply enabled (no typed confirmation required).
    await expect(applyBtn).toBeEnabled();
  });
});
