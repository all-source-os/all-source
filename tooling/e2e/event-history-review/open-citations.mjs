import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { chromium } from "playwright";
import { lookupCheckout } from "./protocol-client.mjs";

const directory = process.argv[2];
if (!directory || !path.isAbsolute(directory))
  throw new Error("Supply the active fixture directory.");
const read = async (name) => JSON.parse(await readFile(path.join(directory, name), "utf8"));
const { ALLSOURCE_EVENT_HISTORY_WEB_ORIGIN: origin } = await read("services.json");
const account = await read("login.json");
assert.ok(account.email === "reviewer@example.test", "Expected the generated reviewer account.");
const events = (await read("events.json")).filter(
  (row) => row.entity === "checkout" && row.workspace === "review-engineering"
);
assert.equal(events.length, 2);
const timestamp = (raw) => {
  const match = raw.match(/^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)(?:\.(\d{1,9}))?Z$/);
  assert.ok(match, "Expected a recorded UTC timestamp.");
  return match[1] + "." + (match[2] || "").padEnd(9, "0") + "Z";
};
const protocolClient = process.argv[3] === "--protocol";
async function signIn(page) {
  if (await page.getByRole("button", { name: "Visit Site" }).isVisible()) {
    await page.getByRole("button", { name: "Visit Site" }).click();
  }
  await page.waitForURL((url) => url.origin === origin && url.pathname === "/login");
  await page.getByRole("button", { name: "Continue with Email" }).click();
  await page.getByLabel("Email address").fill(account.email);
  await page.getByLabel("Password", { exact: true }).fill(account.password);
  await page.locator('form button[type="submit"]').click();
}
const browser = await chromium.launch({
  args: ["--host-resolver-rules=MAP history-verifier.example.test ~NOTFOUND"],
});
let currentPhase = "browser startup";
const requests = [];
function track(page) {
  page.on("console", (message) => {
    if (message.type() === "error" && /content security policy|form-action/i.test(message.text())) {
      requests.push(["blocked by browser content security policy", null, null]);
    }
  });
  page.on("response", (response) => {
    const pathname = new URL(response.url()).pathname;
    const route = pathname.startsWith("/event-history/workspaces/")
      ? "/event-history/workspaces/:workspace/events/:event"
      : pathname;
    if (
      [
        "/login",
        "/api/v1/auth/login",
        "/event-history/connect",
        "/api/event-history/oauth/authorize",
        "/event-history/workspaces/:workspace/events/:event",
        "/callback",
      ].includes(route)
    ) {
      requests.push([response.request().method(), route, response.status()]);
    }
  });
}
const phase = (value) => {
  currentPhase = value;
};
try {
  let citations = events.map((row) => ({
    row,
    url: origin + "/event-history/workspaces/" + row.workspace + "/events/" + row.event_id,
  }));
  if (protocolClient) {
    const consent = await browser.newPage();
    track(consent);
    consent.setDefaultTimeout(12_000);
    const observed = await lookupCheckout(consent, origin, signIn, phase);
    phase("matching MCP results to Core receipts");
    const ids = events.map((row) => row.event_id);
    assert.deepEqual(
      observed.map((row) => row.id),
      [...ids].reverse()
    );
    citations = observed.map((observed) => {
      const row = events.find((row) => row.event_id === observed.id);
      const canonical =
        origin + "/event-history/workspaces/" + row.workspace + "/events/" + row.event_id;
      assert.equal(observed.url, canonical);
      assert.equal(observed.entity, row.entity);
      assert.equal(timestamp(observed.timestamp), timestamp(row.recorded_at));
      assert.equal(
        observed.kind,
        row.event_type === "deployment.released.v1" ? "released" : "rolled_back"
      );
      for (const [field, value] of Object.entries(row.change)) assert.equal(observed[field], value);
      return { row, url: observed.url };
    });
    await consent.close();
  }
  const page = await browser.newPage();
  track(page);
  const proof = [];
  page.setDefaultTimeout(12_000);
  for (const [index, { row, url }] of citations.entries()) {
    const citation = new URL(url);
    phase("opening the citation");
    await page.goto(citation.href, { waitUntil: "domcontentloaded" });
    if (index === 0) {
      phase("evidence sign-in and return");
      await signIn(page);
      await page.waitForURL(citation.href);
    }
    assert.equal(page.url(), citation.href);
    phase("checking the cited event");
    await page.getByRole("button", { name: "Open event" }).click();
    const title =
      row.event_type === "deployment.released.v1"
        ? "Released " + row.change.version
        : "Rolled back from " + row.change.from_version + " to " + row.change.to_version;
    await page.getByRole("heading", { name: title, exact: true }).waitFor();
    await page.getByText(row.event_id, { exact: true }).waitFor();
    await page.getByText(row.entity, { exact: true }).waitFor();
    const displayed = await page.locator("time").innerText();
    assert.equal(timestamp(displayed), timestamp(row.recorded_at));
    proof.push({
      client: protocolClient ? "independent MCP protocol verifier" : "browser",
      event_id: row.event_id,
      workspace: row.workspace,
      entity: row.entity,
      title,
      source_recorded_at: row.recorded_at,
      displayed_at: displayed,
    });
  }
  const filename = protocolClient ? "protocol-proof.jsonl" : "browser-proof.jsonl";
  const lines = proof.map((row) => JSON.stringify(row)).join("\n") + "\n";
  await writeFile(path.join(directory, filename), lines, { mode: 0o600 });
  process.stdout.write(lines);
} catch (error) {
  const http = String(error?.message).match(/returned HTTP (\d{3})/)?.[1];
  process.stderr.write(
    `Event History proof failed during ${currentPhase}${http ? " (HTTP " + http + ")" : ""}.\n`
  );
  process.stderr.write("Browser responses: " + JSON.stringify(requests) + "\n");
  process.exitCode = 1;
} finally {
  await browser.close();
}
