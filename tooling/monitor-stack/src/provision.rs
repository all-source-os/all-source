use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
    process::Command,
};

use serde_json::{json, Value};

fn required(name: &str) -> Result<String, String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{name} is required"))
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| "cannot write private runtime configuration")?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|_| "cannot restrict runtime configuration")?;
    file.write_all(bytes)
        .map_err(|_| "cannot write runtime configuration".into())
}

pub fn prepare(directory: &Path) -> Result<(), String> {
    let token = required("FLY_METRICS_TOKEN")?;
    let token = token.trim().strip_prefix("FlyV1 ").unwrap_or(token.trim());
    if !token.starts_with("fm1") && !token.starts_with("fm2_") {
        return Err("Fly metrics credential must be a read-only Fly macaroon".into());
    }
    let password = required("RESEND_API_KEY")?;
    let from = required("ALERT_FROM")?;
    let to = required("ALERT_TO")?;
    let dashboard = required("ALERT_DASHBOARD_URL")?;
    if !dashboard.starts_with("https://") {
        return Err("dashboard link must use HTTPS".into());
    }
    validate_address(&from)?;
    validate_address(&to)?;
    std::fs::create_dir_all(directory).map_err(|_| "cannot create private runtime directory")?;
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
        .map_err(|_| "cannot restrict runtime directory")?;
    write_private(&directory.join("fly-token"), token.as_bytes())?;
    write_private(&directory.join("smtp-password"), password.as_bytes())?;
    let config = email_config(
        &from,
        &to,
        &directory.join("smtp-password").to_string_lossy(),
        &dashboard,
    );
    write_private(
        &directory.join("alertmanager.json"),
        config.to_string().as_bytes(),
    )
}

fn validate_address(value: &str) -> Result<(), String> {
    if value.contains(['\n', '\r', '<', '>', ',']) || !value.contains('@') || value.contains(' ') {
        return Err("mail address must be one bare address".into());
    }
    Ok(())
}

pub fn email_config(from: &str, to: &str, password_file: &str, dashboard: &str) -> Value {
    let dashboard = dashboard
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&#39;");
    let html = format!(
        "<html><head><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"></head>\
         <body style=\"font-family:Arial,sans-serif;line-height:1.5;max-width:600px;margin:auto;padding:16px\">\
         {{{{ range .Alerts }}}}<h2>{{{{ .Status }}}}: {{{{ .Annotations.summary }}}}</h2>\
         <p>Service: {{{{ .Labels.service }}}}</p><p>{{{{ .Annotations.action }}}}</p>{{{{ end }}}}\
         <p><a href=\"{dashboard}\">Open private Fly metrics dashboard</a></p>\
         <p style=\"font-size:12px\">Private operations alert. Verify caller activity before restricting traffic.</p>\
         </body></html>"
    );
    json!({
        "global": {
            "smtp_smarthost": "smtp.resend.com:587", "smtp_from": from,
            "smtp_auth_username": "resend", "smtp_auth_password_file": password_file,
            "smtp_require_tls": true, "resolve_timeout": "5m"
        },
        "route": {"receiver": "private-email", "group_by": ["alertname", "service"],
            "group_wait": "10s", "group_interval": "1m", "repeat_interval": "6h"},
        "receivers": [{"name": "private-email", "email_configs": [{
            "to": to, "send_resolved": true,
            "headers": {"Subject": "[AllSource {{ .Status }}] {{ .CommonLabels.alertname }}{{ with .CommonLabels.verification_id }} {{ . }}{{ end }}"},
            "html": html,
            "text": "{{ range .Alerts }}Status: {{ .Status }}\nService: {{ .Labels.service }}\n{{ .Annotations.summary }}\nAction: {{ .Annotations.action }}\n\n{{ end }}Private operations alert. Review current Fly metrics before restricting traffic."
        }]}],
        "inhibit_rules": [{
            "source_matchers": ["alertname=ControlPlaneDailyBandwidthCritical"],
            "target_matchers": ["alertname=ControlPlaneDailyBandwidthWarning"], "equal": ["service"]
        }]
    })
}

pub fn import(args: &[String]) -> Result<(), String> {
    if args.len() != 4 {
        return Err("usage: import-secrets ENV_FILE APP RECIPIENT ORG".into());
    }
    validate_address(&args[2])?;
    let contents =
        std::fs::read_to_string(&args[0]).map_err(|_| "cannot read existing mail configuration")?;
    let values = dotenv(&contents);
    let key = values
        .get("RESEND_API_KEY")
        .filter(|value| !value.is_empty())
        .ok_or("mail credential is empty")?;
    let from = values
        .get("EMAIL_FROM")
        .ok_or("sender is missing from existing mail configuration")?;
    validate_address(from)?;
    let token = super::process::capture(
        Command::new("fly").args([
            "tokens",
            "create",
            "readonly",
            "--org",
            &args[3],
            "--name",
            "bandwidth-monitor",
            "--expiry",
            "2160h",
        ]),
        None,
    )?;
    let token = std::str::from_utf8(&token)
        .map_err(|_| "Fly token was not UTF-8")?
        .trim();
    if !token.starts_with("FlyV1 ") && !token.starts_with("fm2_") && !token.starts_with("fm1") {
        return Err("read-only token response had an unexpected format".into());
    }
    let input = format!(
        "FLY_METRICS_TOKEN={token}\nRESEND_API_KEY={key}\nALERT_FROM={from}\nALERT_TO={}\n",
        args[2]
    );
    super::process::capture(
        Command::new("fly").args(["secrets", "import", "--stage", "-a", &args[1]]),
        Some(input.as_bytes()),
    )?;
    println!(
        "monitor: read-only metrics token and existing mail configuration staged; values withheld"
    );
    Ok(())
}

fn dotenv(contents: &str) -> BTreeMap<String, String> {
    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim().strip_prefix("export ").unwrap_or(line.trim());
            let (name, value) = line.split_once('=')?;
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .or_else(|| {
                    value
                        .strip_prefix('\'')
                        .and_then(|value| value.strip_suffix('\''))
                })
                .unwrap_or(value);
            Some((name.trim().to_string(), value.to_string()))
        })
        .collect()
}

#[cfg(test)]
#[path = "provision_tests.rs"]
mod tests;
