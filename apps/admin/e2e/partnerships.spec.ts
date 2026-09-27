import { expect, type Page, test } from "@playwright/test";
import { emptyPartnership, type PartnershipRevision } from "../src/lib/partnerships-api";

const example = (): PartnershipRevision => ({
  record: {
    ...emptyPartnership(),
    id: "example.com",
    organization: "Example Ventures",
    website: "https://example.com",
    geography: "London, UK",
    angle: "An explicitly hypothetical replay workshop for interested engineering teams.",
    limitations: "No paid demand or external supplier eligibility verified.",
    next_action: "Check for a reply before proposing anything further.",
    score: {
      model: "human-rubric-v1",
      run_at: "2026-09-27T00:00:00Z",
      fit: 2,
      leverage: 2,
      access: 1,
      paid_demand: 0,
      rationale: "Synthetic evidence for testing, not a prospect.",
    },
    sources: [
      {
        url: "https://example.com/platform",
        title: "Synthetic platform source",
        evidence: "Fixture only. No claim about a real firm.",
        checked_at: "2026-09-27T00:00:00Z",
      },
    ],
  },
  revision: 1,
  saved_at: "2026-09-27T00:00:00Z",
  actor: "test-admin",
});

async function setup(page: Page, initial: PartnershipRevision[] = [example()]) {
  const b64 = (value: unknown) => Buffer.from(JSON.stringify(value)).toString("base64url");
  const token = `${b64({ alg: "HS256", typ: "JWT" })}.${b64({ sub: "test-admin", role: "admin", exp: Math.floor(Date.now() / 1000) + 3600 })}.test-only`;
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
  await page.route("**/api/auth/session", (route) =>
    route.fulfill({
      json: { data: { user: { id: "test-admin", email: "admin@example.com", role: "admin" } } },
    })
  );
  const state = { records: structuredClone(initial), failRead: false, failSave: false, writes: 0 };
  await page.route("**/api/v1/admin/partnerships**", async (route) => {
    if (route.request().method() === "GET") {
      if (state.failRead)
        return route.fulfill({
          status: 503,
          json: { message: "Storage unavailable; no records changed." },
        });
      if (route.request().url().endsWith("/history"))
        return route.fulfill({ json: { history: state.records } });
      return route.fulfill({ json: { records: state.records } });
    }
    state.writes++;
    if (state.failSave)
      return route.fulfill({
        status: 409,
        json: { message: "Record changed; reload before saving." },
      });
    const { record, expected_revision } = route.request().postDataJSON();
    record.id = new URL(record.website).hostname.replace(/^www\./, "");
    const old = state.records.find((r) => r.record.id === record.id);
    if ((old?.revision ?? 0) !== expected_revision)
      return route.fulfill({
        status: 409,
        json: { message: "Record changed; reload before saving." },
      });
    const saved = {
      record,
      revision: expected_revision + 1,
      saved_at: "2026-09-27T12:00:00Z",
      actor: "test-admin",
    };
    state.records = [...state.records.filter((r) => r.record.id !== record.id), saved];
    return route.fulfill({ json: saved });
  });
  return state;
}

test("unauthenticated workspace requires login", async ({ page }) => {
  await page.goto("/partnerships");
  await expect(page).toHaveURL(/\/login\?redirect=/);
});

test("create, edit and record an interaction without sending", async ({ page }) => {
  const state = await setup(page, []);
  await page.goto("/partnerships");
  await page.getByRole("button", { name: "Add organisation" }).click();
  await page.getByLabel("Organisation", { exact: true }).fill("Synthetic Partner");
  await page.getByLabel("Website (identity)").fill("https://example.org");
  await page.getByLabel("Next action", { exact: true }).fill("Verify an appropriate route.");
  await page.getByRole("button", { name: "Save record", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Synthetic Partner" })).toBeVisible();
  await page.getByRole("button", { name: "Edit record", exact: true }).click();
  await page.getByLabel("Commercial angle").fill("A bounded event-store proof.");
  await page.getByRole("button", { name: "Save record", exact: true }).click();
  await expect(page.getByText("A bounded event-store proof.")).toBeVisible();
  await page.getByRole("button", { name: "Messages (0)", exact: true }).click();
  await page.getByRole("button", { name: "Record draft or interaction" }).click();
  await page.getByLabel("Recipient / sender address or profile").fill("team@example.org");
  await page.getByLabel("Exact message").fill("Synthetic draft. Never send this test message.");
  await page.getByRole("button", { name: "Save interaction", exact: true }).click();
  await expect(page.getByText("outbound · email · draft", { exact: true })).toBeVisible();
  expect(state.records[0]?.record.messages[0]?.outcome).toBe("draft");
  expect(state.writes).toBe(3);
  await page.reload();
  await page.getByRole("button", { name: /Synthetic Partner/ }).click();
  await expect(page.getByText("A bounded event-store proof.")).toBeVisible();
});

test("save conflicts preserve the operator's draft; read failure is not empty data", async ({
  page,
}) => {
  const state = await setup(page);
  await page.goto("/partnerships");
  await page.getByRole("button", { name: /Example Ventures/ }).click();
  await page.getByRole("button", { name: "Edit record", exact: true }).click();
  await page.getByLabel("Next action", { exact: true }).fill("Keep this unsaved change.");
  state.failSave = true;
  await page.getByRole("button", { name: "Save record", exact: true }).click();
  await expect(page.getByTestId("partnerships-page").getByRole("alert")).toContainText(
    "Record changed"
  );
  await expect(page.getByLabel("Next action", { exact: true })).toHaveValue(
    "Keep this unsaved change."
  );
  state.failRead = true;
  await page.reload();
  await expect(page.getByTestId("partnerships-page").getByRole("alert")).toContainText(
    "Storage unavailable"
  );
  await expect(page.getByRole("button", { name: "Add organisation" })).toBeDisabled();
});

test("private import reports duplicate conflicts instead of overwriting", async ({ page }) => {
  const state = await setup(page);
  await page.goto("/partnerships");
  await page.getByRole("button", { name: "Research & send workflow" }).click();
  await page.getByText("Import private research and send history", { exact: true }).click();
  await page.getByLabel("Partnership import file").setInputFiles({
    name: "synthetic.json",
    mimeType: "application/json",
    buffer: Buffer.from(
      JSON.stringify({
        records: [
          example().record,
          {
            ...emptyPartnership(),
            organization: "Second Synthetic",
            website: "https://example.org",
          },
        ],
      })
    ),
  });
  await page.getByRole("button", { name: "Import 2 records (no sends)" }).click();
  await expect(page.getByRole("status")).toContainText("1 created, 1 not imported");
  expect(state.records).toHaveLength(2);
  expect(state.records.find((r) => r.record.id === "example.com")?.revision).toBe(1);
  await expect(page.getByTestId("partnerships-page").getByRole("alert")).toContainText(
    "Example Ventures"
  );
});

test("mobile and desktop stay inside viewport with readable evidence and skill handoff", async ({
  page,
}) => {
  await setup(page);
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto("/partnerships");
  await page.getByRole("button", { name: /Example Ventures/ }).click();
  await expect(page.getByRole("heading", { name: "Source evidence" })).toBeVisible();
  await page.screenshot({ path: "proofshot-artifacts/partnerships-desktop.png", fullPage: true });
  await page.getByRole("button", { name: "Research & send workflow" }).click();
  await expect(page.getByText(/Show exact recipient, channel and message/)).toBeVisible();
  await page.setViewportSize({ width: 320, height: 900 });
  await expect
    .poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth))
    .toBe(true);
  await page.screenshot({ path: "proofshot-artifacts/partnerships-mobile.png", fullPage: true });
});
