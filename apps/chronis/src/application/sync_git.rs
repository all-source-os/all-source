//! Git-backed sync for `.chronis/` state — the Beads-compatible mode.
//!
//! Commits and pushes the Chronis-owned files and nothing else. The
//! "nothing else" is the load-bearing half: this runs at the end of a session,
//! often while other work is uncommitted in the same tree and while other agent
//! sessions are editing it. A blanket `git add -A` here would sweep someone
//! else's half-finished change into a commit labelled as a task-state sync.
//!
//! So the staging is by explicit pathspec, and the index is re-read afterwards
//! and checked: if anything outside `.chronis/` is staged, this refuses rather
//! than committing it.

use std::{
    path::Path,
    process::{Command, Output},
};

use crate::domain::error::ChronError;

/// What a git sync did, so the caller can render it in either output mode.
#[derive(Debug, PartialEq, Eq)]
pub enum GitSyncOutcome {
    /// Nothing changed. Not an error — a session that touched no tasks is normal.
    NothingToSync,
    Synced {
        branch: String,
        files: usize,
    },
}

/// The only paths this command is allowed to stage.
const OWNED_PREFIX: &str = ".chronis/";

pub fn sync_git(workspace_root: &Path) -> Result<GitSyncOutcome, ChronError> {
    let repo_root =
        git_stdout(workspace_root, &["rev-parse", "--show-toplevel"]).map_err(|_| {
            ChronError::Sync(format!(
                "{} is not inside a git repository, so there is nothing to sync to.\n\
             Use `cn sync` on its own for HTTP sync to a remote Core.",
                workspace_root.display()
            ))
        })?;
    let repo_root = Path::new(repo_root.trim());

    let branch = current_branch(repo_root)?;
    let changed = changed_owned_files(repo_root)?;
    if changed.is_empty() {
        // An ignored .chronis/ with nothing tracked yet produces an empty status
        // forever, so a plain "nothing to sync" would be a success message for a
        // command that can never do anything. Say which rule is silencing it.
        // Tracked files still report changes normally, so this only fires when
        // there is genuinely nothing git will ever see.
        if is_ignored(repo_root) && tracked_owned_count(repo_root)? == 0 {
            return Err(ChronError::Sync(format!(
                "{OWNED_PREFIX} is excluded by a gitignore rule and nothing under it is \
                 tracked, so a git sync would never carry anything.\n\
                 Find the rule with `git check-ignore -v {OWNED_PREFIX}`, then either \
                 un-ignore it or use `cn sync` without --git."
            )));
        }
        return Ok(GitSyncOutcome::NothingToSync);
    }

    git_ok(repo_root, &["add", "--", OWNED_PREFIX])?;
    assert_index_is_ours(repo_root)?;

    git_ok(
        repo_root,
        &[
            "commit",
            "-m",
            "chore(chronis): sync task state",
            "--only",
            "--",
            OWNED_PREFIX,
        ],
    )?;

    push(repo_root, &branch)?;

    Ok(GitSyncOutcome::Synced {
        branch,
        files: changed.len(),
    })
}

fn current_branch(repo_root: &Path) -> Result<String, ChronError> {
    // `symbolic-ref` fails on a detached HEAD, which is the case worth naming:
    // committing there strands the work on no branch and the push has no target.
    git_stdout(repo_root, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .map(|s| s.trim().to_string())
        .map_err(|_| {
            ChronError::Sync(
                "HEAD is detached, so there is no branch to push to.\n\
                 Check out a branch first: `git switch -c <name>`."
                    .to_string(),
            )
        })
}

fn changed_owned_files(repo_root: &Path) -> Result<Vec<String>, ChronError> {
    let out = git_stdout(repo_root, &["status", "--porcelain", "--", OWNED_PREFIX])?;
    Ok(out
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(String::from)
        .collect())
}

fn is_ignored(repo_root: &Path) -> bool {
    git_raw(repo_root, &["check-ignore", "-q", OWNED_PREFIX])
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn tracked_owned_count(repo_root: &Path) -> Result<usize, ChronError> {
    let out = git_stdout(repo_root, &["ls-files", "--", OWNED_PREFIX])?;
    Ok(out.lines().filter(|l| !l.trim().is_empty()).count())
}

/// Refuse to commit anything the user did not ask this command to touch.
fn assert_index_is_ours(repo_root: &Path) -> Result<(), ChronError> {
    let staged = git_stdout(repo_root, &["diff", "--cached", "--name-only"])?;
    let foreign: Vec<&str> = staged
        .lines()
        .map(str::trim)
        .filter(|p| !p.is_empty() && !p.starts_with(OWNED_PREFIX))
        .collect();

    if foreign.is_empty() {
        return Ok(());
    }
    Err(ChronError::Sync(format!(
        "refusing to commit: {} file(s) outside {OWNED_PREFIX} are already staged, \
         and a task-state sync must not carry them:\n  {}\n\
         Commit or unstage them first (`git restore --staged <path>`).",
        foreign.len(),
        foreign.join("\n  ")
    )))
}

fn push(repo_root: &Path, branch: &str) -> Result<(), ChronError> {
    let remotes = git_stdout(repo_root, &["remote"])?;
    if remotes.trim().is_empty() {
        return Err(ChronError::Sync(
            "the commit was made, but this repository has no remote to push to.\n\
             Add one with `git remote add origin <url>`, then push."
                .to_string(),
        ));
    }

    // HEAD:<branch> and no force — this must never rewrite published history.
    let out = git_raw(repo_root, &["push", "origin", &format!("HEAD:{branch}")])?;
    if out.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&out.stderr);
    let hint = if stderr.contains("non-fast-forward") || stderr.contains("fetch first") {
        "the remote has commits you do not have. Run `git pull --rebase` and sync again.\n\
         This command will not force-push."
    } else {
        "see the git output above."
    };
    Err(ChronError::Sync(format!(
        "the commit was made, but the push was rejected: {hint}\n\n{stderr}"
    )))
}

fn git_raw(repo_root: &Path, args: &[&str]) -> Result<Output, ChronError> {
    Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .output()
        .map_err(|e| ChronError::Sync(format!("cannot run git: {e}")))
}

fn git_stdout(dir: &Path, args: &[&str]) -> Result<String, ChronError> {
    let out = git_raw(dir, args)?;
    if !out.status.success() {
        return Err(ChronError::Sync(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn git_ok(repo_root: &Path, args: &[&str]) -> Result<(), ChronError> {
    git_stdout(repo_root, args).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "chronis-sync-git-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn init_repo(dir: &Path) {
        git_ok(dir, &["init", "--initial-branch=main"]).expect("init");
        git_ok(dir, &["config", "user.email", "test@example.com"]).expect("email");
        git_ok(dir, &["config", "user.name", "Test"]).expect("name");
        // Both settings isolate the fixture from the developer's machine, which
        // otherwise decides whether these tests pass: a global commit.gpgsign
        // fails every fixture commit with "No secret key", and a global
        // excludesFile ignoring .chronis/ makes every fixture a silent no-op.
        // Real syncs still inherit whatever the actual repository is configured
        // to do — that is the behaviour the ignored-path case below covers.
        git_ok(dir, &["config", "commit.gpgsign", "false"]).expect("no signing in fixtures");
        git_ok(dir, &["config", "core.excludesFile", "/dev/null"]).expect("no global ignores");
        git_ok(dir, &["commit", "--allow-empty", "-m", "root"]).expect("root commit");
    }

    fn write(dir: &Path, rel: &str, body: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, body).expect("write");
    }

    #[test]
    fn a_clean_tree_is_a_successful_no_op() {
        let dir = scratch("clean");
        init_repo(&dir);
        assert_eq!(
            sync_git(&dir).expect("clean sync"),
            GitSyncOutcome::NothingToSync
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unrelated_working_tree_changes_are_left_alone() {
        let dir = scratch("unrelated");
        init_repo(&dir);
        write(&dir, ".chronis/sync/events.jsonl", "{\"id\":1}\n");
        write(&dir, "src/app.rs", "fn main() {}\n");

        // No remote, so the push fails — the commit still happened, which is
        // what this asserts against.
        let _ = sync_git(&dir);

        let tracked = git_stdout(&dir, &["ls-files"]).expect("ls-files");
        assert!(tracked.contains(".chronis/sync/events.jsonl"));
        assert!(
            !tracked.contains("src/app.rs"),
            "a task-state sync must not commit unrelated files: {tracked}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_foreign_staged_file_blocks_the_commit() {
        let dir = scratch("foreign");
        init_repo(&dir);
        write(&dir, ".chronis/sync/events.jsonl", "{\"id\":1}\n");
        write(&dir, "src/app.rs", "fn main() {}\n");
        git_ok(&dir, &["add", "--", "src/app.rs"]).expect("stage foreign");

        let err = sync_git(&dir).expect_err("must refuse");
        assert!(
            format!("{err}").contains("src/app.rs"),
            "the refusal must name what blocked it: {err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_detached_head_is_named_rather_than_committed_onto() {
        let dir = scratch("detached");
        init_repo(&dir);
        let head = git_stdout(&dir, &["rev-parse", "HEAD"]).expect("head");
        git_ok(&dir, &["checkout", "--detach", head.trim()]).expect("detach");
        write(&dir, ".chronis/sync/events.jsonl", "{\"id\":1}\n");

        let err = sync_git(&dir).expect_err("must refuse");
        assert!(format!("{err}").contains("detached"), "got: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_remote_is_reported_after_the_commit_is_made() {
        let dir = scratch("no-remote");
        init_repo(&dir);
        write(&dir, ".chronis/sync/events.jsonl", "{\"id\":1}\n");

        let err = sync_git(&dir).expect_err("no remote to push to");
        assert!(format!("{err}").contains("no remote"), "got: {err}");

        let log = git_stdout(&dir, &["log", "--oneline", "-1"]).expect("log");
        assert!(
            log.contains("sync task state"),
            "the commit must still exist so the next push can carry it: {log}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A global `.chronis/` ignore is real — this machine's ~/.gitignore_global
    /// has one — and it would otherwise make every sync a cheerful no-op.
    #[test]
    fn an_ignored_chronis_dir_is_reported_rather_than_silently_skipped() {
        let dir = scratch("ignored");
        init_repo(&dir);
        write(&dir, ".gitignore", ".chronis/\n");
        write(&dir, ".chronis/sync/events.jsonl", "{\"id\":1}\n");

        let err = sync_git(&dir).expect_err("an unsyncable state must not read as success");
        let text = format!("{err}");
        assert!(text.contains("excluded by a gitignore rule"), "got: {text}");
        assert!(
            text.contains("check-ignore"),
            "must name how to find it: {text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_push_to_a_real_remote_succeeds() {
        let dir = scratch("push");
        let remote = scratch("push-remote");
        git_ok(&remote, &["init", "--bare", "--initial-branch=main"]).expect("bare init");
        init_repo(&dir);
        git_ok(
            &dir,
            &["remote", "add", "origin", &remote.to_string_lossy()],
        )
        .expect("remote");
        write(&dir, ".chronis/sync/events.jsonl", "{\"id\":1}\n");

        let outcome = sync_git(&dir).expect("push succeeds");
        assert!(matches!(outcome, GitSyncOutcome::Synced { .. }));

        let remote_log = git_stdout(&remote, &["log", "--oneline", "-1"]).expect("remote log");
        assert!(remote_log.contains("sync task state"), "got: {remote_log}");

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&remote);
    }
}
