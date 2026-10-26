# Hosted Prime sync acceptance request

Verify the real hosted AllSource sync path through native Allstack skills with authenticated Codex and Claude. Keep AllSource as the event authority, Chronis as task ownership, and Prime as source-linked project context.

The user authorized branch `test/hosted-prime-sync`, one proof PR, and a temporary tenant key named `Allstack hosted sync proof` with only `events:read` and `events:write`, a 30-day expiry, and revocation after qualification. No merge, deployment, or unrelated tenant mutation is included.

## Acceptance

- Claude produces a source-grounded native plan and independently reviews acceptance code.
- Codex uses native QA to create two isolated synthetic Prime decisions across a local process restart. The actual MCP binary pushes them to the hosted tenant.
- Hosted Core exact-entity reads return the expected properties and one stable remote event ID per decision; the persisted local cursor advances and restart recovers the first decision without replay duplicates.
- Claude uses native QA from a separate process to verify both event IDs and hosted Prime exact-node results without opening the writer local store.
- Missing and invalid authentication return HTTP 401. Exercise temporary-key revocation with an executable HTTP 401 assertion; retain and clearly report any failure. Publication of the proof does not mark a failed credential lifecycle as passed.
- Required package gates execute on the final published commit. Publish the verification artifacts and truthful findings through one PR, then read back actual GitHub, Chronis, and Prime state. Keep the root lifecycle task open when live cleanup acceptance fails.

The two coding hosts run sequentially on one machine. A second physical computer, another tenant, and a hosted-server restart are explicit limits. Synthetic proof nodes remain in the tenant audit trail. Secrets remain in private runtime state and never enter repository artifacts.
