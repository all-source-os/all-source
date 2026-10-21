use super::*;

#[test]
fn mail_uses_tls_private_recipient_and_recovery_notifications() {
    let config = email_config(
        "monitor@example.test",
        "owner@example.test",
        "/run/private-password",
        "https://monitor.example.test/?app=example&view=metrics",
    );
    assert_eq!(config["global"]["smtp_require_tls"], true);
    assert_eq!(
        config["global"]["smtp_auth_password_file"],
        "/run/private-password"
    );
    assert!(config["global"].get("smtp_auth_password").is_none());
    let receiver = &config["receivers"][0]["email_configs"][0];
    assert_eq!(receiver["to"], "owner@example.test");
    assert_eq!(receiver["send_resolved"], true);
    assert!(receiver["html"]
        .as_str()
        .unwrap()
        .contains("https://monitor.example.test/?app=example&amp;view=metrics"));
    assert!(!receiver["html"].as_str().unwrap().contains("9093"));
    assert_eq!(config["route"]["repeat_interval"], "6h");
    assert_eq!(config["inhibit_rules"][0]["equal"], json!(["service"]));
}

#[test]
fn addresses_cannot_inject_headers_or_add_recipients() {
    for address in [
        "x@example.test\r\nBcc: attacker@example.test",
        "x@example.test,y@example.test",
        "Name <x@example.test>",
    ] {
        assert!(validate_address(address).is_err());
    }
    assert!(validate_address("owner@example.test").is_ok());
}

#[test]
fn existing_env_quotes_and_export_prefix_are_parsed_without_shell_execution() {
    let values = dotenv(
        "export EMAIL_FROM='monitor@example.test'\nRESEND_API_KEY=\"synthetic-only\"\n# comment\n",
    );
    assert_eq!(values["EMAIL_FROM"], "monitor@example.test");
    assert_eq!(values["RESEND_API_KEY"], "synthetic-only");
}

#[test]
fn deployed_bandwidth_rules_match_the_canonical_tested_rules() {
    assert_eq!(
        include_str!("../rules/bandwidth.rules.yml"),
        include_str!("../../../deploy/prometheus/bandwidth.rules.yml")
    );
}
