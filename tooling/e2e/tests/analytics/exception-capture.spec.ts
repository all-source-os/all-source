import { expect, test } from "@playwright/test";

const SECRET = "e2e-secret-message-7f3a";

test("an uncaught browser error reaches PostHog as a type-only exception", async ({ page }) => {
  const exceptionBodies: string[] = [];
  const cspViolations: string[] = [];

  // Fulfilled locally: nothing reaches the real PostHog project.
  await page.route(/^https:\/\/eu(-assets)?\.i\.posthog\.com\//, async (route) => {
    const request = route.request();
    const body = request.postData() ?? "";
    if (request.url().includes("/i/v0/e/") && body.includes('"$exception"')) {
      exceptionBodies.push(body);
    }
    await route.fulfill({ status: 200, contentType: "application/json", body: "{}" });
  });
  page.on("console", (message) => {
    if (message.text().includes("Content Security Policy")) cspViolations.push(message.text());
  });

  await page.goto("/");
  await page.waitForFunction(
    () =>
      (window as { __allsourceAnalytics?: { exceptionTrackingInstalled?: boolean } })
        .__allsourceAnalytics?.exceptionTrackingInstalled === true
  );

  await page.evaluate((secret) => {
    setTimeout(() => {
      throw new Error(secret);
    });
    void Promise.reject(new Error(secret));
  }, SECRET);

  await expect.poll(() => exceptionBodies.length).toBe(2);
  const mechanisms = exceptionBodies.map((body) => {
    expect(body).not.toContain(SECRET);
    expect(body).not.toContain("stack");
    const { properties } = JSON.parse(body);
    expect(properties.$exception_type).toBe("ClientError");
    expect(properties.exception_contract).toBe("type-only");
    expect(properties.analytics_test).toBe(true);
    return properties.$exception_mechanism;
  });
  expect(mechanisms.sort()).toEqual(["window.error", "window.unhandledrejection"]);
  expect(cspViolations).toEqual([]);
});
