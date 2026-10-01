import { expect, test } from "@playwright/test";

for (const slug of ["cqrs-vs-event-sourcing", "event-store-vs-database"]) {
  test(`${slug} has canonical metadata and readable comparison tables`, async ({ page }) => {
    const response = await page.goto(`/blog/${slug}`);
    expect(response?.status()).toBe(200);
    await expect(page.getByRole("heading", { level: 1 })).toHaveCount(1);
    await expect(page.locator('link[rel="canonical"]')).toHaveAttribute(
      "href",
      `https://www.all-source.xyz/blog/${slug}`
    );
    await expect(page.locator('meta[name="twitter:image"]')).toHaveAttribute(
      "content",
      /\.(png|jpe?g|webp)$/i
    );
    for (const width of [1440, 360]) {
      await page.setViewportSize({ width, height: 900 });
      const tables = page.locator("article table");
      expect(await tables.count()).toBeGreaterThan(0);
      for (const table of await tables.all()) {
        const metrics = await table.evaluate((element) => {
          const rect = element.getBoundingClientRect();
          return {
            left: rect.left,
            right: rect.right,
            scrollWidth: element.scrollWidth,
            clientWidth: element.clientWidth,
          };
        });
        expect(metrics.left).toBeGreaterThanOrEqual(0);
        expect(metrics.right).toBeLessThanOrEqual(width);
        expect(metrics.scrollWidth).toBeLessThanOrEqual(metrics.clientWidth + 1);
      }
    }
    await page.locator('article a[href="/platform/event-sourcing"]').first().click();
    await expect(page).toHaveURL(/\/platform\/event-sourcing$/);
  });
}
