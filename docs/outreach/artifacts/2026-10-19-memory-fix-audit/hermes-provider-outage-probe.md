# Hermes provider-outage write-destination probe

Status: [executed against Hermes v0.22.2 with a synthetic standalone SQLite provider](../../2026-10-19-live-memory-evaluation.md). No AllSource integration or production vendor migration test.

Target release: [v0.22.2](https://github.com/NousResearch/hermes-agent/releases/tag/v0.22.2), commit `d923a5eb487a17c0a68801bba9ef0f56c81040a8`.
Comparison main: `8f5693af418256b89da17ddda18a9242f113dcf6`.

Question: when a configured external provider cannot activate, where does an attempted memory write land, and what does recovery later recall?

Use a disposable container and test-only provider database/home. Keep production mounts and credentials outside the experiment. Use the provider's supported API or query engine to inspect records; do not grep its database files.

| Step | Action | Evidence to retain |
| --- | --- | --- |
| 1 | Install chosen released Hermes version and standalone provider in isolated container. Configure test-only provider. | Hermes version, image digest, provider version, configured provider, actual activation output. |
| 2 | Store `baseline-HERMES-DESTINATION-A` through normal memory flow. Query provider through its supported interface. | Tool result plus record in intended provider. |
| 3 | Recreate disposable image without provider package, retaining test data volumes and configuration. Capture startup recovery, consent and warnings. | Which recovery route ran; whether provider activated; exact user-visible warning. Do not assume install recovery fails. |
| 4 | If provider remains unavailable, ask normal memory flow to store `outage-HERMES-DESTINATION-B`. If recovered automatically, record that and do not force an outage by changing unrelated settings. | Accepted/rejected tool response, actual provider identity, and supported read from each possible destination. |
| 5 | Recreate container again with provider restored. Recall both markers through ordinary agent path, then query provider and built-in memory independently. | Which destination contains each marker, which recall path returned it, and any migration event. |
| 6 | Compare release with pinned main only if both environments can be reproduced. | Separate versioned records. No fabricated identical behavior. |

Evaluate independently:

- Was unavailable configuration visible to the user?
- Did the runtime identify the actual write destination?
- Was the write accepted, rejected, or explicitly degraded?
- Does recovery discover the outage marker? Does it explain its source?
- Was migration attempted, completed, skipped, or unsupported?

Fallback can be intentional. Record behavior before deciding whether the user wants fail-closed writes, an explicit degraded mode, or supported migration. Existing warning/recovery tests already cover much of the original complaint. This procedure supplies no evidence of absent receipts, data loss, or a current deployed defect until run.

Suggested result record:

```json
{
  "hermes_commit": "",
  "provider_version": "",
  "image_digest": "",
  "startup_recovery": "not_run",
  "configured_provider": "",
  "actual_write_destination": "unknown",
  "outage_write_result": "not_run",
  "marker_in_builtin": "unknown",
  "marker_in_external": "unknown",
  "recall_after_recovery": "not_run",
  "source_attribution": "unknown",
  "evidence_files": []
}
```

No AllSource provider is supplied. Hermes requires new memory backends and third-party integrations to ship as standalone plugins, not in-tree provider directories.
