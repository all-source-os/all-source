# Event History acceptance and host review

An engineering lead links one workspace, asks “What changed in checkout?” and follows each citation to the corresponding AllSource record. The reviewer uses a verified account in an isolated workspace. The connection shares the event ID, entity, recorded time, recognized release or rollback fields and citation. A timeline lookup and an explicit event open each consume one query unit.

## Test the service boundaries

| Boundary | Acceptance evidence |
| --- | --- |
| Core retained read | Restart a persisted store. Compare the two checkout receipts against the timeline and exact event lookup. Check equal timestamps, both interval boundaries, another tenant, an unreadable archive and an empty interval. |
| OAuth and billing | Select from current memberships, exchange a single-use S256 code, initialize a host session and replay its request ID. Inspect the workspace meter. Revoke the grant, then repeat the read. |
| Browser | Sign in from a citation. Confirm opening the shell leaves usage unchanged. Open the event and compare ID, timestamp and projected release fields against the ingest receipt. Repeat after membership removal. |
| ChatGPT and Claude | Start disconnected in each host. Select the reviewer workspace. Ask the bare question, record the tool name and actual UTC interval, open **both** citations and compare receipts. Disconnect each grant separately. |

## Configure a reviewer deployment

Use the same public HTTPS web origin for discovery, consent, the token endpoint and evidence links. Enable `ALLSOURCE_EVENT_HISTORY_ENABLED=true` in the web application, Query Service and MCP service. Set `ALLSOURCE_EVENT_HISTORY_WEB_ORIGIN` in all three. Configure the web application's `QUERY_SERVICE_URL` and `ALLSOURCE_EVENT_HISTORY_HTTP_URL`. Set the MCP service's `ALLSOURCE_EVENT_HISTORY_QUERY_URL` to its Query Service origin and `CUSTOMER_REVIEW_HTTP_IP` and `CUSTOMER_REVIEW_HTTP_PORT` for the shared listener. Origins supplied by browser-based clients must be explicitly permitted in `ALLSOURCE_EVENT_HISTORY_ORIGINS`.

Point Query Service at the review Core leader with its dedicated service credential. Set Core's `ALLSOURCE_HISTORY_GATEWAY_SUBJECT` to that credential's subject. Use independent reviewer signing secrets. Configure Query Service's `ALLSOURCE_EVENT_HISTORY_CLIENTS` with separate entries for ChatGPT and Claude. Each entry contains the host's display name, exact callback URI and token endpoint authentication method. The ChatGPT package supplies the public client ID `allsource-history-chatgpt`. Copy the callback shown by the ChatGPT connection management screen. In Claude, choose the pre-registered client and register `https://claude.ai/api/mcp/auth_callback`. Keep any confidential client secret in deployment and host configuration. [ChatGPT authentication](https://developers.openai.com/plugins/build/auth) · [Claude custom connectors](https://support.claude.com/en/articles/11175166-get-started-with-custom-connectors-using-remote-mcp)

Use the review origin with the path `/mcp/event-history` in both host configurations. A bearer challenge links to resource metadata, which identifies the history issuer. Use the verified workspace's own email sign-in. Store the source receipts in the private review handoff and compare the IDs in the host response and opened browser pages.

## Marketplace review cases

### Run the isolated fixture

Build Core with multi-tenant support, the Auth library tests with `cargo test --locked --manifest-path apps/auth/Cargo.toml --lib --no-run`, and the control-plane test executable with `go test -c -o <private-dir>/history-review` from `apps/control-plane`. Build the web application with the local review service URLs. Each Elixir application needs a test build. Run these builds sequentially.

Set `ALLSOURCE_HISTORY_REVIEW_FIXTURE=1`, `ALLSOURCE_AUTH_FIXTURE_BINARY`, `ALLSOURCE_CORE_BINARY`, `HISTORY_WEB_APP_DIR` and `ALLSOURCE_EVENT_HISTORY_WEB_ORIGIN` in the fixture's environment. Start `history-review -test.v -test.run '^TestEventHistoryReviewFixture$' -test.timeout 285s`. Its private directory contains `services.json` for the services, `login.json` for the synthetic reviewer and `events.json` for source comparisons. Use the verified account to select `review-engineering`. The fixture owns the local sign-in, Core and web processes.

From each Elixir application's directory run `MIX_ENV=test mix run --no-start --no-compile ../../tooling/e2e/event-history-review/service.exs PROFILE /absolute/private/path/services.json`, replacing `PROFILE` with `mcp` or `query`. Supply the registered review clients in the Query Service environment. Both processes exit before the fixture expires. Forward HTTPS traffic only to the fixture's restricted web ingress on loopback port 14909. Re-seed before replaying a review after the fixture exits.

### Host prompts

The portable manifest for ChatGPT validates against the Agent Plugins 1.0.0 plugin schema. Its companion MCP file uses OpenAI's documented per-server OAuth extension. The generic 1.0.0 streamable HTTP schema rejects this extension as an additional property. Validate the complete package through the OpenAI portal before submitting it. Claude's directory-profile CLI validates the separate Claude package. [OpenAI OAuth packaging](https://developers.openai.com/plugins/build/plugins#configure-mcp-authentication) · [Generic transport schema](https://agent-plugins.org/schemas/1.0.0/mcp.schema.json)

Seed a fresh checkout release to 2.4, then a rollback from 2.4 to 2.3. Include an unrelated service, a service with 21 matching events and a known empty interval. Use the timestamp from the source receipts to replace the explicit interval placeholders before uploading review cases.

| Host | Reviewer prompt or action | Expected observation |
| --- | --- | --- |
| Both | What changed in checkout? | One history call for checkout, preceding 24 hours. Rollback first. Separate citations for release and rollback. |
| Both | What changed in checkout between [release start UTC] and [rollback time UTC]? | Release appears. Rollback at the exclusive upper bound is absent. |
| Both | What changed in search? | Only events for the exact entity search appear. |
| ChatGPT | What changed in checkout between [empty start UTC] and [empty end UTC]? | Complete coverage with zero events, or explicit unavailable coverage if the store cannot certify it. |
| ChatGPT | Show changes for the seeded high-volume service. | At most 20 events, partial coverage and a suggestion to narrow the interval. |
| ChatGPT | Read checkout in a workspace outside my grant. | No lookup outside the selected workspace. |
| ChatGPT | Roll checkout back to 2.3. | No write tool is available. |
| ChatGPT | Disconnect AllSource, then ask what changed in checkout. | No event disclosure. Reconnection requires consent. |

Use the [ChatGPT plugin package](../../integrations/event-history/chatgpt/plugin.json) for a With MCP submission. The [Claude plugin package](../../integrations/event-history/claude/.claude-plugin/plugin.json) includes the remote server mapping. Claude Desktop's custom web connector can also point directly at the same endpoint. Both checked-in packages target the canonical product domain. For private validation, set a copy's endpoint to the isolated review origin. Capture the native recording, redacted reviewer evidence and support contact before directory submission. [ChatGPT review portal requirements](https://developers.openai.com/plugins/deploy/submission) · [Claude directory policy](https://support.claude.com/en/articles/13145358-anthropic-software-directory-policy)
