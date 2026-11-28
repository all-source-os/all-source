# AllSource Event History

An engineer connects a workspace from ChatGPT or Claude, asks “What changed in checkout?”, and opens the underlying events. The first answer names the workspace, time window and changes it could substantiate. A workspace with no checkout events gets an honest empty result and instructions for choosing a recorded entity.

The buyer is a CTO whose team already records events in AllSource. The first user has permission to read the selected workspace's engineering history. Data onboarding uses the existing AllSource ingestion paths. The reviewer workspace will contain synthetic checkout releases, a rollback and unrelated events. A new real workspace always queries its own records.

## Existing parts

- The Elixir MCP HTTP handler, remote client and customer review protocol are connected in the repository graph (36 nodes in the scoped Graphify traversal). The current handler serves `/mcp/customer-review`. Query Service already implements authorization-code redemption, current membership checks and consent. Its policy defines one Claude client and one review scope. [Current authorization design](2026-09-27-customer-remote-oauth-design.md)
- Query Service's event controller passes tenant, entity and time filters into Core's paged event reader. Browser event inspection exists in the dashboard. A link that opens one authorized, projected event is still needed. [Query controller](../../apps/query-service/lib/query_service_ex_web/controllers/event_controller.ex)
- Allstack recall found prior Prime sync interval decisions. They concern CLI startup and do not establish a hosted Event History grant. The earlier suite assessment identified read-only event history as the first public connector. [Marketplace assessment](../reports/2026-11-26-harness-suite-marketplaces.md)

Use the existing remote transport and add a dedicated read profile. It fits both cloud hosts and keeps setup self-service. A local desktop extension requires per-device installation. The broader admin catalog brings unrelated operations into the review. The ritual decision check selected the dedicated history flow with confidence 1.00.

**Done for an engineer** means establishing the connection unaided, getting a real checkout answer and following a citation after signing into the same workspace. **Ready for directory review** additionally requires an actual run from each host and its submission material. Approval, publishing and a production rollout are owner decisions. [Reviewed design](2026-11-26-event-history-design.md)
