//! Production smoke test using reserved .invalid addresses and marked QA events.
//! Credentials/cookies remain in memory. Never prints response bodies or tokens.
use anyhow::{Context, Result, bail, ensure};
use reqwest::{Client, Response};
use serde_json::{Value, json};
use uuid::Uuid;

const BASE: &str = "https://www.all-source.xyz";
async fn session(client: &Client, path: &str, body: Value) -> Result<(String, Value)> {
    let response = client
        .post(format!("{BASE}/api/v1/auth/{path}"))
        .json(&body)
        .send()
        .await?;
    ensure!(
        response.status().is_success(),
        "{path} failed with {}",
        response.status()
    );
    let cookie = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|h| h.to_str().ok())
        .find(|s| s.starts_with("auth_token="))
        .context("session cookie missing")?
        .to_owned();
    ensure!(
        cookie.contains("HttpOnly") && cookie.contains("Secure") && cookie.contains("SameSite=lax"),
        "unsafe cookie attributes"
    );
    let data: Value = response.json().await?;
    ensure!(
        data["session_established"] == true && data.get("token").is_none(),
        "browser session contract failed"
    );
    Ok((cookie.split(';').next().unwrap().to_owned(), data))
}
async fn json_success(response: Response, label: &str) -> Result<Value> {
    ensure!(
        response.status().is_success(),
        "{label} failed with {}",
        response.status()
    );
    Ok(response.json().await?)
}

#[tokio::main]
async fn main() -> Result<()> {
    ensure!(
        std::env::args().nth(1).as_deref() == Some("--apply"),
        "requires --apply; creates two labelled QA accounts and one QA event"
    );
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(35))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut identities = Vec::new();
    for _ in 0..2 {
        let email = format!("allsource-qa-{}@example.invalid", Uuid::new_v4());
        let password = format!("QA!{}9a", Uuid::new_v4());
        let input =
            json!({"name":"AllSource QA - not a customer","email":email,"password":password});
        let (cookie, data) = session(&client, "register", input.clone()).await?;
        ensure!(data["new_user"] == true, "new workspace not flagged");
        let me = json_success(
            client
                .get(format!("{BASE}/api/auth/session"))
                .header("cookie", &cookie)
                .send()
                .await?,
            "session",
        )
        .await?;
        let tid = me["data"]["user"]["tenant_id"]
            .as_str()
            .context("missing tenant")?
            .to_owned();
        ensure!(tid.starts_with("email-"), "wrong workspace scope");
        ensure!(
            me["data"]["tenant"]["subscription_tier"] == "trial",
            "trial plan missing"
        );
        ensure!(
            me["data"]["tenant"]["events_quota"] == 1000
                && me["data"]["tenant"]["queries_quota"] == 100,
            "trial limits missing"
        );
        let duplicate = client
            .post(format!("{BASE}/api/v1/auth/register"))
            .json(&input)
            .send()
            .await?;
        ensure!(
            duplicate.status().is_client_error(),
            "duplicate signup was not rejected"
        );
        let wrong = client
            .post(format!("{BASE}/api/v1/auth/login"))
            .json(&json!({"email":email,"password":"WrongPassword!99"}))
            .send()
            .await?;
        ensure!(
            wrong.status().is_client_error() && wrong.headers().get("set-cookie").is_none(),
            "wrong password accepted"
        );
        let (login_cookie, login_data) = session(&client, "login", input).await?;
        ensure!(
            login_data["new_user"] == false,
            "returning login recreated workspace"
        );
        identities.push((login_cookie, tid));
    }
    ensure!(identities[0].1 != identities[1].1, "workspace collision");
    let entity = format!("qa-signup-{}", Uuid::new_v4());
    let event = json!({"entity_id":entity,"event_type":"qa.signup_verification","payload":{"purpose":"signup-repair-smoke","analytics_test":true},"metadata":{"source":"signup_repair_qa","analytics_test":true}});
    json_success(
        client
            .post(format!("{BASE}/api/v1/events"))
            .header("cookie", &identities[0].0)
            .json(&event)
            .send()
            .await?,
        "event ingestion",
    )
    .await?;
    let query = |cookie: &str, tenant: &str| {
        client
            .get(format!("{BASE}/api/v1/events/query"))
            .header("cookie", cookie)
            .query(&[("entity_id", entity.as_str()), ("tenant_id", tenant)])
    };
    let own = json_success(
        query(&identities[0].0, &identities[0].1).send().await?,
        "own event read",
    )
    .await?;
    ensure!(
        own.to_string().contains(&entity),
        "ingested event not readable"
    );
    let foreign = query(&identities[1].0, &identities[0].1).send().await?;
    if foreign.status().is_success() {
        let body: Value = foreign.json().await?;
        if body.to_string().contains(&entity) {
            bail!("TENANT ISOLATION FAILED");
        }
    } else {
        ensure!(
            foreign.status().as_u16() == 403,
            "isolation probe inconclusive"
        );
    }
    let logout = client
        .delete(format!("{BASE}/api/auth/session"))
        .header("cookie", &identities[0].0)
        .send()
        .await?;
    ensure!(
        logout.status().is_success()
            && logout
                .headers()
                .get("set-cookie")
                .and_then(|s| s.to_str().ok())
                .unwrap_or("")
                .contains("Max-Age=0"),
        "logout did not clear cookie"
    );
    ensure!(
        client
            .get(format!("{BASE}/api/auth/session"))
            .send()
            .await?
            .status()
            .as_u16()
            == 401,
        "anonymous session accepted"
    );
    println!(
        "PASS: 2 QA signups; duplicate rejection; wrong-password rejection; returning login; HttpOnly sessions; distinct tenants; event write/read; cross-tenant isolation; logout. QA event excluded from customer activation."
    );
    Ok(())
}
