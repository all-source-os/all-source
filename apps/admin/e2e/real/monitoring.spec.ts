import { expect, test } from "@playwright/test";

test.describe("Monitoring — real stack", () => {
  test("monitoring page loads", async ({ page }) => {
    await page.goto("/monitoring");
    await expect(page.getByTestId("monitoring-page")).toBeVisible({
      timeout: 15_000,
    });
  });

  test("verdict and stat cards render a state, never a blank", async ({ page }) => {
    await page.goto("/monitoring");
    await expect(page.getByTestId("monitoring-page")).toBeVisible({
      timeout: 15_000,
    });

    const verdict = page.getByTestId("client-verdict");
    await expect(verdict).toBeVisible();
    await expect(verdict).not.toContainText("Checking client traffic", { timeout: 15_000 });
    await expect(page.getByTestId("stat-cards")).toBeVisible();
    await expect(page.getByTestId("stat-card-served")).toBeVisible();
  });

  test("machines and Core internals panels render", async ({ page }) => {
    await page.goto("/monitoring");
    await expect(page.getByTestId("monitoring-page")).toBeVisible({
      timeout: 15_000,
    });

    await expect(page.getByTestId("machines-panel")).toBeVisible();
    await expect(page.getByTestId("core-internals")).toBeVisible();
  });

  test("auto-refresh indicator is visible", async ({ page }) => {
    await page.goto("/monitoring");
    await expect(page.getByTestId("monitoring-page")).toBeVisible({
      timeout: 15_000,
    });

    await expect(page.getByTestId("auto-refresh-indicator")).toBeVisible();
  });
});
