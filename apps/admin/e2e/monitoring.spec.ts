import { expect, type Page, test } from "@playwright/test";

/**
 * Monitoring dashboard e2e tests. Every API call is mocked; no backend runs.
 */

const minute = 60_000;
const at = (msAgo: number) => new Date(Date.now() - msAgo).toISOString();

const healthyReport = {
  configured: true,
  range: "1h",
  step_seconds: 60,
  generated_at: at(0),
  now: {
    window_seconds: 300,
    served: 5809,
    refused_auth: 0,
    throttled: 3,
    failed: 1,
    total: 5813,
    served_ratio: 5809 / 5813,
  },
  series: Array.from({ length: 12 }, (_, i) => ({
    t: at((11 - i) * minute),
    served: 1150 + i,
    refused_auth: i === 6 ? 40 : 0,
    throttled: 1,
    failed: 0,
  })),
  tenants: [
    {
      tenant: "onboard-control-plane-at-technical-leaders-com",
      served_5m: 5_800,
      refused_5m: 4,
      failed_5m: 1,
      baseline_per_min: 1180,
      state: "served",
    },
    {
      tenant: "anonymous",
      served_5m: 0,
      refused_5m: 0,
      failed_5m: 0,
      baseline_per_min: 0,
      state: "idle",
    },
  ],
  releases: [
    { app: "allsource-control-plane", instance: "849100f2140768", started_at: at(6 * minute) },
    { app: "allsource-control-plane", instance: "9080e6f2a3", started_at: at(6 * minute - 20_000) },
    { app: "allsource-core", instance: "85777d6c1d17d8", started_at: at(6 * minute - 40_000) },
  ],
  machines: [
    {
      app: "allsource-control-plane",
      instance: "849100f2140768",
      up: true,
      started_at: at(6 * minute),
      uptime_seconds: 360,
    },
    {
      app: "allsource-core",
      instance: "85777d6c1d17d8",
      up: true,
      started_at: at(150 * minute),
      uptime_seconds: 9000,
    },
  ],
};

const outageReport = {
  ...healthyReport,
  now: {
    window_seconds: 300,
    served: 0,
    refused_auth: 821,
    throttled: 0,
    failed: 0,
    total: 821,
    served_ratio: 0,
  },
  tenants: [
    {
      tenant: "anonymous",
      served_5m: 0,
      refused_5m: 626,
      failed_5m: 0,
      baseline_per_min: 0,
      state: "refused",
    },
    {
      tenant: "onboard-control-plane-at-technical-leaders-com",
      served_5m: 0,
      refused_5m: 0,
      failed_5m: 0,
      baseline_per_min: 1150,
      state: "quiet",
    },
  ],
};

function adminJwt(): string {
  const b64 = (o: unknown) => Buffer.from(JSON.stringify(o)).toString("base64url");
  const iat = Math.floor(Date.now() / 1000);
  const header = b64({ alg: "HS256", typ: "JWT" });
  const payload = b64({
    sub: "admin-1",
    email: "admin@allsource.co",
    role: "admin",
    iat,
    exp: iat + 3600,
  });
  return `${header}.${payload}.test-signature`;
}

async function mockApis(page: Page, health: { status: number; body: unknown }) {
  await page.context().addCookies([
    {
      name: "admin_token",
      value: adminJwt(),
      domain: "localhost",
      path: "/",
      httpOnly: true,
      sameSite: "Lax",
    },
  ]);
  await page.route("**/api/auth/session", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        data: { user: { id: "admin-1", email: "admin@allsource.co", role: "admin" } },
      }),
    })
  );
  await page.route("**/api/v1/admin/metrics/client-health*", (route) =>
    route.fulfill({
      status: health.status,
      contentType: "application/json",
      body: JSON.stringify(health.body),
    })
  );
  await page.route("**/api/v1/admin/metrics/summary", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        events_total: 1_420_000,
        query_latency_p99_ms: 11.9,
        uptime_seconds: 0,
      }),
    })
  );
}

test.describe("Monitoring dashboard", () => {
  test("healthy traffic shows a served verdict and real numbers", async ({ page }) => {
    await mockApis(page, { status: 200, body: healthyReport });
    await page.goto("/monitoring");

    const verdict = page.getByTestId("client-verdict");
    await expect(verdict).toHaveAttribute("data-tone", "ok");
    await expect(verdict).toContainText(
      "Clients served: 99.9% of 5,813 requests in the last 5 min."
    );

    const statValue = (card: string) => page.getByTestId(card).getByTestId("stat-value");
    await expect(statValue("stat-card-served")).toHaveText("5,809");
    await expect(statValue("stat-card-refused-by-auth")).toHaveText("0");
    await expect(statValue("stat-card-throttled")).toHaveText("3");
    await expect(statValue("stat-card-failed")).toHaveText("1");
    await expect(statValue("stat-card-tenants-served")).toHaveText("1");
    await expect(page.getByTestId("stat-card-tenants-served")).toContainText(
      "0 gone quiet, 0 refused"
    );

    for (const removed of ["uptime", "events-sec", "active-tenants", "error-rate"]) {
      await expect(page.getByTestId(`stat-card-${removed}`)).toHaveCount(0);
    }

    await expect(page.getByTestId("core-events-total")).toContainText("1,420,000");
    await expect(page.getByTestId("core-query-p99")).toContainText("11.9 ms");
  });

  test("chart renders stacked series with one visible marker per rolling deploy", async ({
    page,
  }) => {
    await mockApis(page, { status: 200, body: healthyReport });
    await page.goto("/monitoring");

    const chart = page.getByTestId("chart-client-requests");
    await expect(chart.locator("svg.recharts-surface").first()).toBeVisible();
    await expect(page.getByTestId("release-markers")).toContainText(
      "control-plane 849100f2140768 started"
    );
    await expect(page.getByTestId("release-markers").locator("li")).toHaveCount(3);

    const lines = chart.locator(".recharts-reference-line line");
    await expect(lines).toHaveCount(1);
    const stroke = await lines.first().evaluate((el) => getComputedStyle(el).stroke);
    expect(stroke).not.toBe("none");
    await expect(chart.locator("svg.recharts-surface").first()).toContainText(
      "control-plane ×2, core"
    );

    const curves = chart.locator(".recharts-area-curve");
    await expect(curves).toHaveCount(4);
    const curveStrokes = await curves.evaluateAll((els) =>
      els.map((el) => getComputedStyle(el).stroke)
    );
    expect(curveStrokes).not.toContain("none");
    expect(new Set(curveStrokes).size).toBe(4);
  });

  test("an auth outage reads as refused and puts the quiet tenant first", async ({ page }) => {
    await mockApis(page, { status: 200, body: outageReport });
    await page.goto("/monitoring");

    const verdict = page.getByTestId("client-verdict");
    await expect(verdict).toHaveAttribute("data-tone", "bad");
    await expect(verdict).toContainText("821 of 821 requests refused (401/403)");

    const rows = page.getByTestId("tenant-health").locator("tbody tr");
    await expect(rows.first()).toHaveAttribute("data-state", "quiet");
    await expect(page.getByTestId("tenant-row-anonymous")).toContainText("authentication failed");
    await expect(page.getByTestId("stat-card-tenants-served").getByTestId("stat-value")).toHaveText(
      "0"
    );
    await expect(page.getByTestId("stat-card-tenants-served")).toContainText("1 gone quiet");
  });

  test("not configured explains the env var and shows no numbers", async ({ page }) => {
    await mockApis(page, {
      status: 200,
      body: { configured: false, range: "1h", generated_at: at(0) },
    });
    await page.goto("/monitoring");

    await expect(page.getByTestId("client-health-not-configured")).toContainText(
      "ALLSOURCE_METRICS_PROM_TOKEN"
    );
    await expect(page.getByTestId("client-health-not-configured")).toContainText("FlyV1");
    await expect(page.getByTestId("client-verdict")).toHaveAttribute("data-tone", "unknown");
    await expect(page.getByTestId("stat-card-served")).toContainText("Not measured");
  });

  test("an upstream failure shows the error with a retry, never zeros", async ({ page }) => {
    let calls = 0;
    await mockApis(page, {
      status: 502,
      body: { configured: true, error: "requests now: prometheus: HTTP 500: boom" },
    });
    await page.route("**/api/v1/admin/metrics/client-health*", (route) => {
      calls++;
      return route.fulfill({
        status: 502,
        contentType: "application/json",
        body: JSON.stringify({
          configured: true,
          error: "requests now: prometheus: HTTP 500: boom",
        }),
      });
    });
    await page.goto("/monitoring");

    const error = page.getByTestId("client-health-error");
    await expect(error).toContainText("boom");
    await expect(page.getByTestId("stat-card-served")).toContainText("Error");
    const before = calls;
    await error.getByRole("button", { name: "Retry" }).click();
    await expect.poll(() => calls).toBeGreaterThan(before);
  });

  test("range selector requests the new range", async ({ page }) => {
    const ranges: string[] = [];
    await mockApis(page, { status: 200, body: healthyReport });
    await page.route("**/api/v1/admin/metrics/client-health*", (route) => {
      ranges.push(new URL(route.request().url()).searchParams.get("range") ?? "");
      return route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(healthyReport),
      });
    });
    await page.goto("/monitoring");
    await expect(page.getByTestId("chart-client-requests")).toBeVisible();

    await page.getByTestId("range-selector").getByText("24H").click();
    await expect.poll(() => ranges).toContain("24h");
  });

  test("range selector stays usable after a failed range", async ({ page }) => {
    const ranges: string[] = [];
    await mockApis(page, { status: 200, body: healthyReport });
    await page.route("**/api/v1/admin/metrics/client-health*", (route) => {
      const range = new URL(route.request().url()).searchParams.get("range") ?? "";
      ranges.push(range);
      return route.fulfill(
        range === "7d"
          ? {
              status: 502,
              contentType: "application/json",
              body: JSON.stringify({ configured: true, error: "prometheus: HTTP 504: timeout" }),
            }
          : { status: 200, contentType: "application/json", body: JSON.stringify(healthyReport) }
      );
    });
    await page.goto("/monitoring");
    await expect(page.getByTestId("chart-client-requests")).toBeVisible();

    await page.getByTestId("range-selector").getByText("7D").click();
    await expect(page.getByTestId("client-health-error")).toContainText("HTTP 504");
    await expect(page.getByTestId("chart-client-requests")).toHaveCount(0);

    await page.getByTestId("range-selector").getByText("1H").click();
    await expect(page.getByTestId("chart-client-requests")).toBeVisible();
    expect(ranges.at(-1)).toBe("1h");
  });

  test("phone width stacks tenants so the state badge is visible", async ({ page }) => {
    await page.setViewportSize({ width: 320, height: 900 });
    await mockApis(page, { status: 200, body: outageReport });
    await page.goto("/monitoring");

    const card = page.getByTestId("tenant-card-onboard-control-plane-at-technical-leaders-com");
    await expect(card).toHaveAttribute("data-state", "quiet");
    const badge = card.getByText("Gone quiet");
    await badge.scrollIntoViewIfNeeded();
    await expect(badge).toBeInViewport({ ratio: 1 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth
    );
    expect(overflow).toBeLessThanOrEqual(0);
  });

  test("auto-refresh indicator is visible", async ({ page }) => {
    await mockApis(page, { status: 200, body: healthyReport });
    await page.goto("/monitoring");
    await expect(page.getByTestId("auto-refresh-indicator")).toContainText(/refresh/i);
  });
});
