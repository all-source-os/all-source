use anyhow::{Result, bail};
use clap::ValueEnum;
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum AccessProfile {
    Local,
    HostedTenant,
    Operator,
}

#[derive(Clone, Debug)]
pub struct DiagnosticPolicy {
    profile: AccessProfile,
    tenant_id: Option<String>,
    source_id: String,
}

impl DiagnosticPolicy {
    /// Build a diagnostic policy and validate required immutable bindings.
    pub fn new(profile: AccessProfile, tenant_id: Option<String>, source_id: &str) -> Result<Self> {
        let tenant_id = tenant_id
            .map(|tenant| tenant.trim().to_string())
            .filter(|tenant| !tenant.is_empty());
        let source_id = source_id.trim().to_string();

        if source_id.is_empty() {
            bail!("--source-id must not be empty");
        }
        if matches!(profile, AccessProfile::HostedTenant) && tenant_id.is_none() {
            bail!("--tenant-id is required with --profile hosted-tenant");
        }

        Ok(Self {
            profile,
            tenant_id,
            source_id,
        })
    }

    /// Return the configured tenant boundary, when present.
    pub fn tenant_id(&self) -> Option<&str> {
        self.tenant_id.as_deref()
    }

    /// Return the safe source label emitted in diagnostic provenance.
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Report whether this policy represents a hosted tenant caller.
    pub fn is_hosted_tenant(&self) -> bool {
        matches!(self.profile, AccessProfile::HostedTenant)
    }

    /// Build evidence provenance for one observation.
    pub fn context(&self, fresh_through: Option<&str>) -> Value {
        let (binding, verified) = match (self.profile, self.tenant_id.as_deref()) {
            (AccessProfile::HostedTenant | AccessProfile::Local, Some(_)) => {
                ("server_configuration", true)
            }
            (AccessProfile::Operator, Some(_)) => ("operator_profile", true),
            (AccessProfile::Operator, None) => ("operator_profile_unscoped", false),
            (AccessProfile::Local | AccessProfile::HostedTenant, None) => ("unverified", false),
        };

        json!({
            "contractVersion": "1.0",
            "tenant": {
                "id": self.tenant_id,
                "binding": binding,
                "verified": verified,
            },
            "environment": match self.profile {
                AccessProfile::Local => "local",
                AccessProfile::HostedTenant | AccessProfile::Operator => "production",
            },
            "region": env_value("ALLSOURCE_MCP_REGION"),
            "service": "allsource-mcp",
            "serverVersion": env!("CARGO_PKG_VERSION"),
            "release": env_value("ALLSOURCE_MCP_RELEASE"),
            "sourceId": self.source_id,
            "observedAt": chrono::Utc::now().to_rfc3339(),
            "freshThrough": fresh_through,
            "correlation": {
                "requestId": Value::Null,
                "traceId": Value::Null,
                "runId": Value::Null,
                "workflowRunId": Value::Null,
                "entityId": Value::Null,
                "conversationId": Value::Null,
            }
        })
    }

    /// Copy accepted request correlation identifiers into result context.
    pub fn attach_correlation(result: &mut Value, args: &Value) {
        let Some(context) = result.get_mut("context") else {
            return;
        };
        context["correlation"] = correlation(args);
    }
}

/// Normalize supported wire and storage names into one correlation object.
fn correlation(args: &Value) -> Value {
    let diagnostic = args.get("diagnostic").unwrap_or(&Value::Null);
    let value = |camel: &str, snake: &str| {
        diagnostic
            .get(camel)
            .or_else(|| diagnostic.get(snake))
            .or_else(|| args.get(camel))
            .or_else(|| args.get(snake))
            .cloned()
            .unwrap_or(Value::Null)
    };
    json!({
        "requestId": value("requestId", "request_id"),
        "traceId": value("traceId", "trace_id"),
        "runId": value("runId", "run_id"),
        "workflowRunId": value("workflowRunId", "workflow_run_id"),
        "entityId": value("entityId", "entity_id"),
        "conversationId": value("conversationId", "conversation_id"),
    })
}

/// Read one non-empty environment value.
fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{AccessProfile, DiagnosticPolicy};

    #[test]
    fn hosted_profile_requires_bound_tenant() {
        let error = DiagnosticPolicy::new(AccessProfile::HostedTenant, None, "production-store")
            .expect_err("hosted profile must fail closed");

        assert!(error.to_string().contains("--tenant-id is required"));
    }

    #[test]
    fn local_unbound_context_is_explicitly_unverified() {
        let policy =
            DiagnosticPolicy::new(AccessProfile::Local, None, "local-store").expect("valid policy");

        let context = policy.context(None);
        assert_eq!(context["tenant"]["verified"], false);
        assert_eq!(context["tenant"]["binding"], "unverified");
        assert!(context["correlation"]["runId"].is_null());
    }

    #[test]
    fn unbound_operator_context_is_explicitly_unverified() {
        let policy = DiagnosticPolicy::new(AccessProfile::Operator, None, "production-store")
            .expect("valid policy");

        let context = policy.context(None);
        assert_eq!(context["tenant"]["verified"], false);
        assert_eq!(context["tenant"]["binding"], "operator_profile_unscoped");
        assert!(context["tenant"]["id"].is_null());
    }

    #[test]
    fn correlation_accepts_wire_and_storage_naming() {
        let policy =
            DiagnosticPolicy::new(AccessProfile::Local, None, "local-store").expect("valid policy");
        let mut result = serde_json::json!({ "context": policy.context(None) });

        DiagnosticPolicy::attach_correlation(
            &mut result,
            &serde_json::json!({
                "entity_id": "entity-1",
                "diagnostic": { "runId": "run-1", "request_id": "request-1" }
            }),
        );

        assert_eq!(result["context"]["correlation"]["entityId"], "entity-1");
        assert_eq!(result["context"]["correlation"]["runId"], "run-1");
        assert_eq!(result["context"]["correlation"]["requestId"], "request-1");
    }
}
