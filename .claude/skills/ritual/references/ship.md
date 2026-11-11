# Ship

One PR, one commit, with evidence a reviewer can open.

1. **Collapse the branch to one commit.** Reset to the merge-base, never a commit count:
   `git reset --soft "$(git merge-base origin/<trunk> HEAD)"`, commit with a message that lists
   what the PR carries, and confirm `git diff <old-head> HEAD` is empty. Push with
   `--force-with-lease`. Re-run the gates: the new SHA has no status.
2. **Evidence.** For a user-visible change, `pr-proof` writes the HTML report under `docs/`. For a
   backend change, the PR body carries the commands run and their output, each labelled as
   observed.
3. **Open the PR** with a title in the repo's convention (see the org's notes, `repo-notes.md`, for ticket keys), a body
   with a summary, a test plan of ticked checkboxes, and a link to the design. pr-guard asks
   while the test plan has unticked items and denies a second open PR in the session.
4. **Name it on the epic:** `ritual-pr: <url>`.
5. **Deploys are reported in two parts.** A workflow dispatched is not a service deployed: report
   the dispatch, then read the deployed state (the revision, the version, the URL) before saying
   it is live. Gated environments park for approval; say so.

After the merge, confirm the change is on the trunk (`git show origin/<trunk>:<a changed path>`),
close the beads still open with `cn done`, name the merge on the epic, then Reflect
([reflect.md](reflect.md)):

```bash
cn task edit <epic> --append-description "ritual-shipped: <merge commit>" --toon
```
