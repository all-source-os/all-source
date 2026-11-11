# Review

A reviewer who did not build it reads the branch and the design, and runs what they can. Output:
`docs/plans/YYYY-MM-DD-<slug>-code-review.md` on the branch, or a review on the PR, named on the
epic with its verdict:

```bash
cn task edit <epic> --append-description "ritual-review: <path or PR URL> clean" --toon
cn task edit <epic> --append-description "ritual-review: <path or PR URL> findings" --toon
```

The newest line wins, so a flow with findings stays in Review until a later clean review is named.

## Fresh context, always

The builder's account of the work is testimony. The reviewer starts from the design, the diff and
the repository, not from the build conversation:

- `cos` hires `reviewer` on the branch;
- a fresh subagent given only the design path, the branch and the repo root;
- the repo's own prove-it skill (`/prove-it`), or a peer.

Never review in the builder's context.

## What the reviewer checks

1. **Does it do what the design says?** Run it. A feature is proven by being exercised, not by
   its tests passing.
2. **Do the tests prove it?** Each test can fail; none passes by never running; the trophy
   layers in the plan file exist.
3. **Security:** `security-review` on the diff for anything that touches input, auth, secrets,
   crypto or the browser.
4. **Scope:** nothing outside the epic, no dead code, no comment narrating the fix.
5. **Drift:** the design still describes what was built. If not, update the design on the
   branch; the flow then reads `drift` and needs a new design review and approval before Ship.

Each finding: severity, file:line, the problem, the evidence (the command and its output), the
fix. Findings go back to Build as beads under the epic. Clean means Ship ([ship.md](ship.md)).
