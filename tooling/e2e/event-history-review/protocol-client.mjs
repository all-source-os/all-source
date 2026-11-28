import assert from "node:assert/strict";
import { createHash, randomBytes } from "node:crypto";

const protocol = "2025-11-25";
const client = "allsource-history-verifier";
const redirect = "https://history-verifier.example.test/callback";
const scope = "allsource.history.read";

async function readJson(url, options, label) {
  const response = await fetch(url, {
    ...options,
    redirect: "error",
    signal: AbortSignal.timeout(25_000),
  });
  assert.ok(response.ok, `${label} returned HTTP ${response.status}`);
  assert.equal(response.headers.get("referrer-policy"), "no-referrer");
  return response.json();
}

export async function lookupCheckout(page, origin, signIn, phase) {
  phase("OAuth discovery");
  const resource = origin + "/mcp/event-history";
  const issuer = origin + "/event-history";
  const protectedResource = await readJson(
    origin + "/.well-known/oauth-protected-resource/mcp/event-history",
    {},
    "Resource discovery",
  );
  assert.equal(protectedResource.resource, resource);
  assert.ok(protectedResource.authorization_servers.includes(issuer));
  const metadata = await readJson(
    origin + "/.well-known/oauth-authorization-server/event-history",
    {},
    "Issuer discovery",
  );
  assert.equal(metadata.issuer, issuer);
  assert.equal(
    metadata.authorization_endpoint,
    origin + "/api/event-history/oauth/authorize",
  );
  assert.equal(
    metadata.token_endpoint,
    origin + "/api/event-history/oauth/token",
  );
  assert.ok(metadata.code_challenge_methods_supported.includes("S256"));
  const verifier = randomBytes(32).toString("base64url");
  const state = randomBytes(32).toString("base64url");
  const start = new URL(metadata.authorization_endpoint);
  start.search = new URLSearchParams({
    client_id: client,
    response_type: "code",
    scope,
    resource,
    redirect_uri: redirect,
    code_challenge_method: "S256",
    code_challenge: createHash("sha256").update(verifier).digest("base64url"),
    state,
  }).toString();

  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Fetch.enable", {
    patterns: [{ urlPattern: redirect + "*", requestStage: "Request" }],
  });
  let callback;
  cdp.on("Fetch.requestPaused", async (event) => {
    const incoming = new URL(event.request.url);
    if (incoming.origin + incoming.pathname !== redirect) {
      await cdp.send("Fetch.failRequest", {
        requestId: event.requestId,
        errorReason: "BlockedByClient",
      });
      return;
    }
    callback = incoming;
    await cdp.send("Fetch.fulfillRequest", {
      requestId: event.requestId,
      responseCode: 200,
      responseHeaders: [
        { name: "Content-Type", value: "text/html" },
        { name: "Cache-Control", value: "no-store" },
      ],
      body: Buffer.from(
        "<title>Local protocol verifier</title><p>Authorization received.</p>",
      ).toString("base64"),
    });
  });
  phase("OAuth authorization and product sign-in");
  const [authorize] = await Promise.all([
    page.waitForResponse((response) => response.url() === start.href),
    page.goto(start.href, { waitUntil: "domcontentloaded" }),
  ]);
  assert.equal(authorize.status(), 303);
  assert.equal(await authorize.headerValue("referrer-policy"), "no-referrer");
  await signIn(page);
  phase("workspace selection");
  await page.waitForURL(
    (url) => url.origin === origin && url.pathname === "/event-history/connect",
  );
  await page
    .locator('input[name="workspace_id"][value="review-engineering"]')
    .check();
  phase("submitting consent");
  const [consent] = await Promise.all([
    page.waitForResponse(
      (response) =>
        response.url() === origin + "/event-history/connect" &&
        response.request().method() === "POST",
    ),
    page.getByRole("button", { name: "Allow read access" }).click(),
  ]);
  assert.equal(consent.status(), 303);
  assert.equal(await consent.headerValue("referrer-policy"), "no-referrer");
  phase("opening the authorization callback");
  await page.getByText("Authorization received.", { exact: true }).waitFor();
  await cdp.detach();
  assert.ok(
    callback?.searchParams.get("state") === state,
    "OAuth state did not match",
  );
  assert.ok(
    callback.searchParams.get("iss") === issuer,
    "OAuth issuer did not match",
  );
  const code = callback.searchParams.get("code");
  assert.ok(code, "OAuth callback supplied no authorization code");
  phase("OAuth token exchange");
  const token = await readJson(
    metadata.token_endpoint,
    {
      method: "POST",
      body: new URLSearchParams({
        grant_type: "authorization_code",
        client_id: client,
        code,
        code_verifier: verifier,
        redirect_uri: redirect,
        resource,
      }),
    },
    "Token exchange",
  );
  assert.ok(
    token.scope === scope &&
      token.token_type?.toLowerCase() === "bearer" &&
      typeof token.access_token === "string",
    "Scoped access token missing",
  );

  let session;
  const call = async (id, method, params) => {
    const response = await fetch(resource, {
      method: "POST",
      redirect: "error",
      signal: AbortSignal.timeout(25_000),
      headers: {
        accept: "application/json, text/event-stream",
        "content-type": "application/json",
        authorization: "Bearer " + token.access_token,
        ...(session
          ? { "Mcp-Session-Id": session, "MCP-Protocol-Version": protocol }
          : {}),
      },
      body: JSON.stringify({
        jsonrpc: "2.0",
        ...(id ? { id } : {}),
        method,
        ...(params ? { params } : {}),
      }),
    });
    assert.ok(response.ok, `MCP ${method} returned HTTP ${response.status}`);
    assert.equal(response.headers.get("referrer-policy"), "no-referrer");
    if (!id) {
      assert.equal(response.status, 202);
      return;
    }
    const reply = await response.json();
    assert.ok(
      reply.id === id && !reply.error && reply.result && !reply.result.isError,
      `MCP ${method} failed`,
    );
    if (method === "initialize") {
      session = response.headers.get("Mcp-Session-Id");
      assert.ok(session, "Missing issued MCP session");
      assert.equal(reply.result.protocolVersion, protocol);
    }
    return reply.result;
  };
  phase("MCP initialization");
  await call(1, "initialize", {
    protocolVersion: protocol,
    capabilities: {},
    clientInfo: { name: "allsource-history-protocol-verifier", version: "1" },
  });
  await call(null, "notifications/initialized");
  phase("MCP tool discovery");
  const catalogue = await call(2, "tools/list", {});
  assert.deepEqual(
    catalogue.tools.map((tool) => tool.name),
    ["get_event_history"],
  );
  phase("MCP checkout lookup");
  const result = await call(3, "tools/call", {
    name: "get_event_history",
    arguments: { entity: "checkout" },
  });
  const history = result.structuredContent;
  assert.ok(
    history?.workspace?.id === "review-engineering" &&
      history.entity === "checkout" &&
      history.coverage === "complete",
    "Expected complete checkout history for selected workspace",
  );
  assert.equal(Date.parse(history.to) - Date.parse(history.from), 86_400_000);
  assert.equal(history.events.length, 2);
  return history.events;
}
