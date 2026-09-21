//! Explicit one-time recovery of the auth service's invalid storage credential.
//! Secrets remain in memory and reach Fly through stdin, never argv or output.
use anyhow::{Context, Result, bail};
use base64::Engine;
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn post(path: &str, body: Value) -> Result<Value> {
    let body = base64::engine::general_purpose::STANDARD.encode(body.to_string());
    let remote = format!(
        "sh -c 'printf %s {body} | base64 -d | curl --fail --silent --show-error --max-time 20 -H \"X-API-Key: $ALLSOURCE_BOOTSTRAP_API_KEY\" -H \"Content-Type: application/json\" --data-binary @- http://localhost:3900{path}'"
    );
    let output = Command::new("fly")
        .args(["ssh", "console", "-a", "allsource-core", "-C", &remote])
        .output()?;
    if !output.status.success() {
        bail!(
            "Core provisioning failed at {path}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let text = String::from_utf8(output.stdout)?;
    Ok(serde_json::from_str(
        &text[text.find('{').context("JSON response")?..],
    )?)
}

fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() != Some("--apply") {
        bail!("requires --apply; creates a scoped key and stages auth secrets");
    }
    post(
        "/api/v1/tenants",
        json!({"id":"allsource-auth","name":"AllSource authentication storage","slug":"allsource-auth"}),
    )?;
    let key = post(
        "/api/v1/auth/api-keys",
        json!({"name":"allsource-auth-storage-2026-09-21","tenant_id":"allsource-auth","role":"serviceaccount"}),
    )?;
    let secret = key["key"].as_str().context("key missing")?;
    if !secret.starts_with("ask_") || secret.contains('\n') {
        bail!("invalid key format");
    }
    let mut child = Command::new("fly")
        .args(["secrets", "import", "--app", "allsource-auth", "--stage"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    writeln!(
        child.stdin.take().context("stdin")?,
        "ALLSOURCE_API_KEY={secret}\nAUTH_ALLSOURCE_QUERY_URL=http://allsource-core.internal:3900"
    )?;
    if !child.wait()?.success() {
        bail!("secret staging failed; key retained, do not rerun blindly");
    }
    println!("Scoped auth-storage credential staged. No existing tenant or user records changed.");
    Ok(())
}
