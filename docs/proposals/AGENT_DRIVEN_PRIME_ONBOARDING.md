# Agent-Driven Prime Onboarding

## Status: NOT PLANNED as of 2026-09-16 · Implemented and kept: gap-1, gap-3 · Closed won't-do: gap-2, gap-4, gap-5, gap-6 (#290, #293)

> **Why it stopped.** Gaps 1 and 3 shipped and stay live. Everything remaining
> either buys an Apple Developer ID certificate to remove one Gatekeeper click
> (gap 2), or waits on Anthropic shipping install-from-URL and a signature
> contract that does not exist yet (gaps 4–6). The install path works today at
> two human actions; the remaining gaps chase zero. That was judged not to feed a
> real goal. Reopen gap 2 if signing is wanted for its own sake; reopen 4–6 only
> if Anthropic ships the protocol handler — the unblock conditions are recorded
> in #293.
## Date: 2026-05-26 (updated 2026-05-29)

> **Shipped so far.** Gap 1 (anonymous-trial mint `POST /api/v1/agents/anonymous-trial` + `/connect?claim=` association) and Gap 3 (`/connect` deep-link params: `source`, `key_name`, `return=close`) are merged and live. The recommended path's unavoidable human actions are now down to **two** — drag the `.dxt` and paste the key — plus the conditional Gatekeeper click (Gap 2, open) and the restart (Gap 6, upstream). Trial-claim records the trial→account association only; event re-keying is still a deferred design call (see Gap 1 detail).

## Problem

`apps/web/content/allsource-as-cms-from-claude-desktop.mdx` was originally written for a human reader. When a real Claude Desktop user pasted the URL into a fresh conversation and asked the agent for help installing Prime, the agent produced an 8-step checklist and handed it back to the human — exactly the friction the article was supposed to eliminate.

The article has now been rewritten as an agent-executable protocol (two-party: agent + human, with explicit ownership at each step). That rewrite reduced the unavoidable human-action count on the recommended `.dxt` path to four distinct physical interactions, with the two load-bearing ones being the `.dxt` drag-and-drop and the API key paste. The full count, honestly:

1. Open `/connect`, sign in if not already, click **Create connection**, copy the resulting API key. (Step 3 of the article.)
2. Download the `.dxt`, drag into Claude Desktop, paste the API key into the install dialog, click Install. (Step 4 of the article — this is the "drag + paste" the brief targets as the minimum.)
3. If macOS Gatekeeper fires its "developer cannot be verified" popup, click through **System Settings → Privacy & Security → Allow Anyway**. (Step 4 failure recovery.)
4. Quit and reopen Claude Desktop so the MCP server loads. (Step 5.)

Two of these (the Gatekeeper click-through and the restart) are conditional or platform-imposed; the other two (mint key, drag `.dxt`) are the load-bearing actions. The remaining friction is structural — it lives in the product, not in the article. This proposal enumerates each gap the article cannot close on its own and frames each as a follow-up bead candidate so we can decide which are worth closing and in which order.

Out of scope: rewriting the article (already done). Out of scope: shipping the gaps (each is its own bead).

## Gaps the Article Can't Close

Six gaps prevent a fully agent-driven install. Three are addressable inside this repo, two require coordination with Anthropic, one is purely upstream.

| # | Gap | Owner | Effort | Eliminates |
|---|---|---|---|---|
| 1 | Anonymous-trial API key endpoint | AllSource (this repo) | Small-Medium | "human must sign in + click Create connection" — collapses to "agent calls an endpoint" |
| 2 | `.dxt` macOS code-signing + notarisation | AllSource (this repo, plus Apple Developer enrolment) | Medium | The Gatekeeper "developer cannot be verified" popup and the "Allow Anyway" click-path |
| 3 | Deep-linkable `/connect` flow with pre-filled context | AllSource (this repo) | Small | Tab-switching back-and-forth between Claude Desktop and the browser |
| 4 | Pre-signed `.dxt` install URL (one-click install handoff to Claude Desktop) | AllSource + Anthropic coordination | Medium-Large | The drag-and-drop step itself |
| 5 | MCP-server-installation API in the model surface | Anthropic (upstream) | Out of our control | The entire human-driven install loop — the agent could run the protocol without any human action at all |
| 6 | Hot-load of MCP servers without Claude Desktop restart | Anthropic (upstream) | Out of our control | The "quit and reopen Claude Desktop" step after install |

## Per-Gap Detail

### Gap 1 — Anonymous-trial API key endpoint

**What it'd look like.** A new endpoint, `POST /api/v1/agents/anonymous-trial`, that mints a low-quota, time-limited API key without requiring an authenticated user. Returns the key plus a short-lived claim token the user can later attach to a real account at `/connect?claim=<token>` to migrate the events into their tenant.

```
POST /api/v1/agents/anonymous-trial
{
  "agent_name": "claude-desktop-trial",
  "client_fingerprint": "<opaque ua/ip hash>"
}

→ 201 Created
{
  "api_key": "ask_trial_...",
  "tenant_id": "tnt_anon_...",
  "expires_at": "2026-06-09T00:00:00Z",
  "quota": { "events": 1000, "queries": 100 },
  "claim_token": "clm_...",
  "claim_url": "https://www.all-source.xyz/connect?claim=clm_..."
}
```

**Why it matters for the article.** Today the agent must direct the human to `/connect`, which requires a signed-in session. If the human isn't signed in, the agent has to wait through a signup round-trip, lose the OAuth return path (`/login` doesn't preserve `next` cleanly), and then continue. An anonymous-trial endpoint collapses Step 2 in the article from "human signs up + signs in + clicks Create connection + copies key + pastes key" to "agent calls endpoint, agent hands the key to the human only for the `.dxt` install dialog paste."

**Effort.** Small-Medium. The Control Plane already issues API keys; this adds an unauthenticated route, a "trial" tenant class with capped quotas, and a claim-token table. Existing `apps/auth/` and `apps/control-plane/` patterns cover most of it.

**Risk if we don't ship.** The article's recommended path stays at 2 human actions instead of 1; the agent still has to talk the human through the `/connect` round-trip. Acceptable, not great.

**Bead candidate.** `prime: anonymous-trial API key endpoint with claim-token migration`. Acceptance: agent can call the endpoint from prompt, get a working key, push events; human can later claim those events into a real tenant via `/connect?claim=...` without losing the event history.

### Gap 2 — `.dxt` macOS code-signing + notarisation

**What it'd look like.** The release pipeline at `apps/prime-mcp/scripts/build-dxt.sh` signs both bundled binaries (`server/darwin/allsource-prime` and the future `server/linux/...` — Linux doesn't need this) with an Apple Developer ID, then submits the bundle for notarisation, then staples the notarisation ticket. End result: macOS Gatekeeper opens the binary without the "cannot be verified" dialog.

**Why it matters for the article.** Apple Gatekeeper currently fires a one-shot blocking dialog on first launch of an unsigned binary, even with `com.apple.quarantine` stripped (which works for the `curl | sh` path because we run `xattr -d`, but does NOT work for the `.dxt` path because Claude Desktop runs the binary as a child process — the quarantine attribute is on the unzipped binary inside `~/Library/Application Support/Claude/Claude Extensions/`). The article currently handles this by predicting the popup text and walking the human through System Settings → Privacy & Security → Allow Anyway. That's an extra human action we shouldn't need.

**Effort.** Medium. Requires:
- An Apple Developer Program membership ($99/yr) under the `all.source` org
- App-specific password and notarisation creds wired into the GitHub Actions release secrets
- `codesign` + `notarytool` invocations added to `build-dxt.sh`
- A test on a clean macOS install where Gatekeeper has never seen the binary, to verify the popup is gone

**Risk if we don't ship.** First-time macOS users hit the Gatekeeper popup. Some will give up before clicking through. We lose conversion at the worst possible moment — right after the human has done their part of the install.

**Bead candidate.** `prime: code-sign and notarise the .dxt bundle for macOS`. Acceptance: on a clean macOS install, double-clicking the `.dxt` from Claude Desktop's import flow does NOT produce the "developer cannot be verified" dialog. Verified via a screenshot or video on a fresh VM.

### Gap 3 — Deep-linkable `/connect` flow that pre-fills context

**What it'd look like.** `/connect` accepts URL params the article (or the agent) can hand to it:

- `?source=claude-desktop-agent` — tags the minted API key with where it came from
- `?key_name=My%20Laptop` — pre-fills the key name
- `?return=close` — after the human copies the key, shows a "you can close this tab" state instead of expecting them to navigate back

The current `/connect` page is generic; it doesn't know who sent the human there, so the minted key is named `Claude Desktop (Prime)` for everyone and the post-mint UX doesn't acknowledge that the human came from an article that's waiting on them.

**Why it matters for the article.** Today the agent says "go to /connect, click Create connection, copy the key, come back to this conversation, paste it to me." If the deep link existed, the agent could say "open https://www.all-source.xyz/connect?source=claude-desktop&return=close — copy the key, then come back" — and the page itself would tell the human "great, copy this and close the tab," reducing the cognitive overhead of switching back.

Combined with Gap 1, this isn't needed at all (the agent calls the endpoint directly). On its own, it's a nice-to-have that reduces tab-switching friction.

**Effort.** Small. `apps/web/src/app/(marketing)/connect/connect-client.tsx` reads search params, passes `source` to `createApiKey` as part of `description`, and renders a "you can close this tab" state when `return=close`.

**Risk if we don't ship.** Minor. The human still does what they do today; they just have a slightly fuzzier handoff back to the agent.

**Bead candidate.** `web: /connect accepts source, key_name, return URL params and renders a close-tab state after mint`. Acceptance: visiting `/connect?source=claude-desktop&return=close` mints a key tagged with that source and shows a clear "you can close this tab and return to your agent" state.

### Gap 4 — Pre-signed `.dxt` install URL

**What it'd look like.** A URL of the form `claude://install-mcp?source=https://www.all-source.xyz/install.dxt&signature=...` that Claude Desktop registers as a protocol handler. Clicking the link from any browser opens Claude Desktop and triggers the install flow without a download-then-drag-and-drop step.

**Why it matters for the article.** Drag-and-drop is the second of our two unavoidable human actions. A protocol-handler link would collapse it to a single browser click.

**Effort.** Medium-Large, and most of it is upstream. Claude Desktop would need to register a `claude://` URL scheme and add install-from-URL semantics. We can lobby for this through the MCPB spec ([github.com/anthropics/dxt](https://github.com/anthropics/dxt)) but cannot build it ourselves. On our side: a signed-URL endpoint and a clearer install-from-URL story on `/prime`.

**Risk if we don't ship.** The recommended path stays at 2 human actions instead of 1. Acceptable; we're not the only `.dxt` publisher who'd benefit from this, so it's a reasonable upstream ask.

**Bead candidate.** `prime: pre-signed .dxt URL endpoint + filed upstream MCPB issue requesting claude:// install-from-URL protocol handler`. Acceptance: a `GET /install/prime.dxt?ts=...&sig=...` route exists and returns a 302 to the latest GitHub release artifact; an issue is open at github.com/anthropics/dxt referencing this use case.

### Gap 5 — MCP-server-installation API in the model surface

**What it'd look like.** A tool the model can call from inside a conversation — `mcp.install_server(manifest_url, user_config)` — that hands off to Claude Desktop's install pipeline. The model would describe the install to the human ("I'm about to install AllSource Prime from https://... — it'll prompt you for an API key"), the human would approve, and the install would happen in-conversation.

**Why it matters for the article.** This is the only way to fully eliminate the human-action count. Without it, the agent can never directly cause an MCP server to appear in its own tool surface — by construction, the install must happen out-of-band (drag-and-drop, `curl | sh`, manual config edit), because the agent is sandboxed inside the conversation. The model can describe, ask, predict, and verify — but it cannot install.

**Owner.** Anthropic. This is a model-surface and Claude Desktop product change. We can lobby for it through the same MCPB channels as Gap 4 and through Anthropic's developer relations.

**Effort.** Out of our control.

**Risk if we don't ship.** No risk to AllSource specifically — every MCP server publisher faces this same gap. The article works around it by being explicit about which steps the agent owns and which the human owns. We should still flag it in conversations with Anthropic.

**Bead candidate.** `meta: file feature request with Anthropic for in-conversation MCP server install`. Acceptance: a public issue or developer-forum post exists; we link to it from `/prime` and from this proposal.

### Gap 6 — Hot-load of MCP servers without Claude Desktop restart

**What it'd look like.** Claude Desktop watches `claude_desktop_config.json` and the Extensions directory for changes and registers new MCP servers without requiring a quit-and-reopen of the app. Today the only way to make a freshly installed MCP server appear in the agent's tool surface is to fully restart Claude Desktop.

**Why it matters for the article.** Step 5 of the install protocol — "quit and reopen Claude Desktop" — exists only because the app does not hot-load. The agent cannot perform a Cmd-Q or `kill` on the user's process from inside its sandbox, and even if it could, the conversation itself would terminate when Claude Desktop quits (the model is hosted but the chat session lives in the desktop app). So this is structurally a human action: the human must close and reopen the app, then return to a fresh conversation, and the agent loses any context built up before the restart.

**Owner.** Anthropic. Claude Desktop product change.

**Effort.** Out of our control.

**Risk if we don't ship.** No risk to AllSource specifically — every MCP server suffers the same restart requirement. The article handles it by labelling it clearly. The bigger downstream cost is the loss of conversation context across the restart, which is a Claude Desktop concern, not an AllSource one.

**Bead candidate.** `meta: file feature request with Anthropic for hot-load of MCP servers (no app restart)`. Acceptance: a public issue or developer-forum post exists.

## Recommended Sequencing

If we ship the gaps above, the order that moves the most friction per unit effort:

1. **Gap 3 (deep-link params on `/connect`)** — Half-day. Cheapest. Removes the "where do I paste the key" awkwardness immediately.
2. **Gap 1 (anonymous-trial endpoint)** — 2-3 days. Collapses the recommended path from 2 human actions to 1.
3. **Gap 2 (macOS code-signing)** — 1-2 days of work + the Apple Developer enrolment lead time. Removes the most-likely failure mode. Should be scheduled before any large outreach push (Hacker News, Show HN, etc.) where first-impression conversion matters.
4. **Gap 4 (pre-signed install URL + upstream MCPB issue)** — 1-2 days of work + months of waiting on Anthropic. Open the issue now even if we don't implement our side immediately.
5. **Gap 5 (in-conversation MCP install)** — File the request; everything else is upstream.
6. **Gap 6 (hot-load of MCP servers)** — File the request alongside Gap 5; same upstream channel.

After Gaps 1 + 2 ship, the article's recommended path is: agent calls the trial endpoint → hands the human a `.dxt` download URL → human double-clicks, no Gatekeeper popup, no API key paste (the endpoint returned the key, the agent injected it into the install URL via Gap 3-style deep-link). Human-action count: 1 (the double-click).

After Gap 4 ships (and the upstream `claude://` handler exists): human-action count drops to 0 in the browser path — one click from a link.

After Gap 5 ships: human-action count is 0 except for the consent dialog the install pipeline itself shows.

## Open Questions

1. **Quota for the anonymous-trial tier.** 1000 events / 100 queries was a guess. Should it be lower (more pressure to claim into a real account) or higher (more room to actually use the system before deciding)?
2. **Claim-token expiry.** 14 days feels right but is also a guess. What's the typical "I'll get back to this later" window for a developer who installed something on a Tuesday?
3. **Apple Developer enrolment under which entity?** Personal account of the maintainer is fastest; an `all.source`-registered LLC is cleaner but takes longer.
4. **Should Gap 4 wait until Anthropic's MCPB spec adds install-from-URL?** Building our half now and waiting for the protocol-handler half on the other side is fine, but the article can't recommend it until both halves exist.
5. **Telemetry on the article-to-install funnel.** We currently don't know where the friction is in the new article. Worth instrumenting `/connect?source=claude-desktop-article` mint counts vs `.dxt` download counts vs first-event-from-tenant counts so we can measure each gap's impact when shipped.
