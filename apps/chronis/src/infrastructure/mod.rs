pub mod backend;
pub mod config;
pub mod core_task_repo;
pub mod http_core_client;
pub mod id;
#[cfg(feature = "prime")]
pub mod prime_memory;
pub mod prime_setup;
pub mod projection;
pub mod remote_auth;
pub mod remote_stream;
pub mod wal_tail;
pub mod workspace;

/// Derive the short claim-identity label for the current actor — the value
/// that lands in a task's `claimed_by` via the `workflow.claimed` event.
///
/// Precedence (first match wins):
/// 1. `CN_AGENT_ID` — explicit override (CI, scripts, or a named agent).
/// 2. A Claude Code thread — when `CLAUDECODE=1` and `CLAUDE_CODE_SESSION_ID`
///    is set, label as `claude:<first-8-of-session>`. The session UUID is
///    stable for the life of the thread (it survives every short-lived `cn`
///    invocation), which a pid cannot — so two concurrent Claude threads get
///    distinct, traceable labels instead of both silently claiming as
///    `"human"`. See bead t-b34e.
/// 3. An interactive human — `<user>@<host>`.
///
/// This is the human-readable *display* label only. The full structured actor
/// (session UUID, host, pid, per-claim fencing id) belongs in the claim event
/// payload — that is Phase 2 of t-b34e and is intentionally not done here.
pub fn agent_id() -> String {
    derive_agent_id(&ActorEnv::from_env())
}

/// Raw environment inputs for [`derive_agent_id`], captured into a struct so
/// the derivation logic stays a pure function and can be unit-tested without
/// mutating process-global env vars (which would race under the test harness).
#[derive(Default)]
struct ActorEnv {
    cn_agent_id: Option<String>,
    claudecode: Option<String>,
    session_id: Option<String>,
    user: Option<String>,
    host: Option<String>,
}

impl ActorEnv {
    fn from_env() -> Self {
        Self {
            cn_agent_id: env_nonempty("CN_AGENT_ID"),
            claudecode: env_nonempty("CLAUDECODE"),
            session_id: env_nonempty("CLAUDE_CODE_SESSION_ID"),
            user: env_nonempty("USER").or_else(|| env_nonempty("USERNAME")),
            host: env_nonempty("HOSTNAME"),
        }
    }
}

/// Read an env var, trimmed, treating unset-or-blank as absent.
fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Pure derivation of the actor label from captured env inputs. See
/// [`agent_id`] for the precedence rationale.
fn derive_agent_id(env: &ActorEnv) -> String {
    if let Some(explicit) = &env.cn_agent_id {
        return explicit.clone();
    }
    if env.claudecode.as_deref() == Some("1")
        && let Some(session) = &env.session_id
    {
        let short: String = session.chars().take(8).collect();
        return format!("claude:{short}");
    }
    let user = env.user.clone().unwrap_or_else(|| "user".to_string());
    let host = env.host.clone().unwrap_or_else(|| "local".to_string());
    format!("{user}@{host}")
}

#[cfg(test)]
mod agent_id_tests {
    use super::{ActorEnv, derive_agent_id};

    const SESSION: &str = "f132c66c-3a54-401e-ae89-6cff327779b0";

    #[test]
    fn explicit_cn_agent_id_wins_over_everything() {
        let env = ActorEnv {
            cn_agent_id: Some("ci-runner".into()),
            claudecode: Some("1".into()),
            session_id: Some(SESSION.into()),
            user: Some("decebal".into()),
            host: Some("mac".into()),
        };
        assert_eq!(derive_agent_id(&env), "ci-runner");
    }

    #[test]
    fn claude_session_yields_short_stable_label() {
        let env = ActorEnv {
            claudecode: Some("1".into()),
            session_id: Some(SESSION.into()),
            ..Default::default()
        };
        assert_eq!(derive_agent_id(&env), "claude:f132c66c");
    }

    #[test]
    fn distinct_sessions_get_distinct_labels() {
        let a = ActorEnv {
            claudecode: Some("1".into()),
            session_id: Some("aaaaaaaa-1111-2222-3333-444444444444".into()),
            ..Default::default()
        };
        let b = ActorEnv {
            claudecode: Some("1".into()),
            session_id: Some("bbbbbbbb-1111-2222-3333-444444444444".into()),
            ..Default::default()
        };
        assert_ne!(derive_agent_id(&a), derive_agent_id(&b));
    }

    #[test]
    fn claudecode_without_session_falls_back_to_human_identity() {
        // CLAUDECODE set but no session id (older harness / odd env): we can't
        // distinguish threads, so treat it as an interactive human rather than
        // collapsing every thread onto a single "claude" label.
        let env = ActorEnv {
            claudecode: Some("1".into()),
            user: Some("decebal".into()),
            host: Some("mac".into()),
            ..Default::default()
        };
        assert_eq!(derive_agent_id(&env), "decebal@mac");
    }

    #[test]
    fn interactive_human_is_user_at_host() {
        let env = ActorEnv {
            user: Some("decebal".into()),
            host: Some("mac".into()),
            ..Default::default()
        };
        assert_eq!(derive_agent_id(&env), "decebal@mac");
    }

    #[test]
    fn empty_env_has_a_safe_default() {
        assert_eq!(derive_agent_id(&ActorEnv::default()), "user@local");
    }

    #[test]
    fn blank_override_is_ignored_not_used_as_label() {
        // env_nonempty maps "" -> None upstream; the derivation must never
        // emit an empty/whitespace label, so a None override falls through.
        let env = ActorEnv {
            cn_agent_id: None,
            user: Some("decebal".into()),
            host: Some("mac".into()),
            ..Default::default()
        };
        assert_eq!(derive_agent_id(&env), "decebal@mac");
    }
}
