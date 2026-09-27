# Compaction preserves unreadable retained inputs

Date: 27 September 2026. Task: `t-079e5c`, discovered while resolving strict
archive task `t-e1d5cf` under the customer-agent delivery goal.

## Reproduction

`compact_tenant` logged a selected Parquet read failure and continued with the
readable candidates. After writing their snapshot, it deleted all selected
inputs, including the unreadable file whose contents were never preserved.
When all readable events expired, the retention branch deleted those inputs
without writing any snapshot. Both paths erased the archive evidence that
strict conditional writes and retained reads need to refuse partial history.

All three new synthetic regressions failed against source `20542247`:

- Partial snapshot compaction replaced the input set and deleted the damaged
  original.
- Fully expired readable events caused every original, including the damaged
  file, to disappear.
- A multi-tenant sweep also removed the damaged tenant's unknown history.

## Repair

The first selected-candidate read error now aborts that tenant's pass before
retention, cold-tier writes, snapshot writes or original-file deletion. The
error includes candidate context but no event payload. A whole-store sweep
continues other tenants according to the existing contract: its successful
aggregate counts only completed work and logs failed tenants. The HTTP sweep
can therefore still return 200 while an individual tenant was refused; this
is not evidence that every tenant compacted.

## Acceptance evidence

- [x] Normal snapshot and fully expired branches preserve every original
  filename and byte when one selected input is unreadable.
- [x] A healthy tenant still compacts while the failed tenant's inputs remain
  intact. Existing healthy compaction/cold-tier/retention unit cases pass.
- [x] All-feature Core library: 2,009 passed, five pre-existing ignored; three
  new compaction, six strict-store and two strict-handler integration cases pass.
- [x] Rust format and all-target/all-feature Clippy with warnings denied pass.
  The first lint run flagged assertion style in the new fixture; it was corrected
  and all three fixture cases reran successfully.
- [x] Rebuilt enterprise + analytics binary passes all sixteen real Core HTTP
  and restart cases, seed 342434, in 15.5 seconds. The damaged-history scenario
  now invokes the real compaction endpoint twice across restart, confirms every
  input is unchanged, and confirms AgentRunStore still refuses that history.
- [x] Query Service formatting, warnings-as-errors compilation and strict Credo
  pass for the updated HTTP fixture.
- [x] No production compaction, retention operation or customer-data mutation
  was performed. Reproduction uses owned temporary files and synthetic events.

## Scope and rollout

This repair is not included in the earlier `20542247` image build. Production
remains release 46 (`0b51f7d4`). Deployment still needs the current source image,
CI, a fresh recovery snapshot and resolution of strict cold-archive compatibility.

This patch does not change enumeration policy, snapshot naming/concurrent
compaction, resource admission, retention policy or durable sequence high-water
marks. It does not prove that externally removed or previously lost history can
be reconstructed, and does not activate customer evidence disclosure.

Source/binary manifests identify proof inputs. Temporary logs:
`/private/tmp/compaction-integrity-{red,green,final,clippy,binary,http,credo}.log`.
Committed regression tests are the durable reproduction.
