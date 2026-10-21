use std::{
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};

pub fn execute(action: &str) -> Result<(), String> {
    let path = Path::new("/data/rules/delivery-probe.yml");
    match action {
        "probe-start" => {
            if path.exists() {
                return Err(
                    "delivery probe already exists; finish or clean that probe first".into(),
                );
            }
            let id = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| "clock is before Unix epoch")?
                .as_millis()
                .to_string();
            let probe = json!({"groups": [{"name": "email-delivery-verification", "rules": [{
                "alert": "BandwidthEmailDeliveryProbe", "expr": "vector(1)", "for": "0s",
                "labels": {"severity": "info", "service": "allsource-bandwidth-alerts", "verification_id": id},
                "annotations": {
                    "summary": "TEST: production bandwidth email delivery",
                    "action": "This synthetic rule verifies firing and recovery emails. It does not indicate recovery of a real bandwidth incident."
                }
            }]}]});
            atomic_write(path, &probe)?;
            println!("monitor: delivery probe created, verification_id={id}");
        }
        "probe-stop" | "probe-clean" => {
            let contents = std::fs::read(path).map_err(|_| "delivery probe does not exist")?;
            let mut probe: Value =
                serde_json::from_slice(&contents).map_err(|_| "invalid delivery probe")?;
            let rule = &mut probe["groups"][0]["rules"][0];
            if rule["alert"] != "BandwidthEmailDeliveryProbe" {
                return Err("refusing to modify an unrelated rule".into());
            }
            if action == "probe-clean" {
                if rule["expr"] != "vector(0) == 1" {
                    return Err("resolve the delivery probe before cleaning it".into());
                }
                std::fs::remove_file(path).map_err(|_| "cannot remove resolved probe")?;
            } else {
                rule["expr"] = json!("vector(0) == 1");
                atomic_write(path, &probe)?;
            }
        }
        _ => return Err("unknown delivery-probe command".into()),
    }
    super::process::capture(
        Command::new("curl").args([
            "--silent",
            "--show-error",
            "--fail",
            "--max-time",
            "10",
            "--request",
            "POST",
            "http://127.0.0.1:9090/-/reload",
        ]),
        None,
    )?;
    println!("monitor: evaluator configuration reloaded");
    Ok(())
}

fn atomic_write(path: &Path, value: &Value) -> Result<(), String> {
    let pending = path.with_extension("pending");
    std::fs::write(&pending, value.to_string()).map_err(|_| "cannot stage verification rule")?;
    std::fs::rename(pending, path).map_err(|_| "cannot install verification rule".into())
}
