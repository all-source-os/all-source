//! Rendered-DOM evidence from a local headless Chrome.
//!
//! Needs no credential, which makes it the one way to answer "is the audited
//! content actually in the DOM, or behind JavaScript" without a paid crawler.
//! Comparing the rendered schema and title against the static response is what
//! licenses treating a static-HTML audit as a proxy for the real page.

use std::{process::Stdio, time::Duration};

use anyhow::Result;
use serde_json::json;
use tokio::{io::AsyncReadExt, process::Command};

use crate::{
    config::RUNBOOK,
    ledger::{Entry, Evidence},
};

/// Chrome exits non-zero after a successful `--dump-dom` often enough that the
/// exit status cannot be the success signal; the DOM on stdout is. A run is
/// judged by the bytes it produced.
const MIN_PLAUSIBLE_DOM_BYTES: usize = 1024;

/// One deadline for the whole render, enforced by the code that owns the child
/// handle. A thread blocked reading stdout cannot kill anything, so a hung
/// Chrome would otherwise outlive this process and reparent to init.
const RENDER_TIMEOUT: Duration = Duration::from_secs(45);

const CHROME_CANDIDATES: &[&str] = &[
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
];

fn chrome_path() -> Option<String> {
    if let Some(explicit) = crate::config::env_opt("CHROME_PATH") {
        return Some(explicit);
    }
    CHROME_CANDIDATES
        .iter()
        .find(|candidate| std::path::Path::new(candidate).exists())
        .map(|candidate| (*candidate).to_owned())
}

pub async fn collect(pages: &[String], profile_dir: &str) -> Result<Vec<Entry>> {
    let measures = "rendered DOM: schema types, title and description after JavaScript runs";

    let Some(chrome) = chrome_path() else {
        return Ok(pages
            .iter()
            .map(|page| Entry {
                source: format!("render.{page}"),
                measures: measures.to_owned(),
                evidence: Evidence::setup_required(
                    vec!["CHROME_PATH".to_owned()],
                    "a Chrome or Chromium binary exists on this machine (set CHROME_PATH if it is not in a standard location)",
                    RUNBOOK,
                ),
            })
            .collect());
    };

    let mut entries = Vec::new();
    for page in pages {
        let evidence = match dump_dom(&chrome, page, profile_dir).await {
            Ok(dom) => Evidence::observed(json!({
                "url": page,
                "dom_bytes": dom.len(),
                "schema_types": schema_types(&dom),
                "title": extract_between(&dom, "<title>", "</title>"),
                "canonical": extract_attr(&dom, "rel=\"canonical\"", "href=\""),
                "robots_meta": extract_attr(&dom, "name=\"robots\"", "content=\""),
            })),
            Err(error) => Evidence::failed(None, error.to_string()),
        };
        entries.push(Entry {
            source: format!("render.{page}"),
            measures: measures.to_owned(),
            evidence,
        });
    }
    Ok(entries)
}

async fn dump_dom(chrome: &str, url: &str, profile_dir: &str) -> Result<String> {
    let mut child = Command::new(chrome)
        .args([
            "--headless=new",
            "--disable-gpu",
            "--no-sandbox",
            "--no-first-run",
            "--disable-extensions",
            "--virtual-time-budget=5000",
            &format!("--user-data-dir={profile_dir}"),
            "--dump-dom",
            url,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("chrome stdout was not piped"))?;

    let mut dom = String::new();
    let read = tokio::time::timeout(RENDER_TIMEOUT, stdout.read_to_string(&mut dom)).await;

    // Kill THEN wait: killing alone turns the orphan into a zombie.
    let _ = child.start_kill();
    let _ = child.wait().await;

    match read {
        Err(_) => anyhow::bail!("chrome did not finish rendering {url} within {RENDER_TIMEOUT:?}"),
        Ok(Err(error)) => anyhow::bail!("reading chrome stdout failed: {error}"),
        Ok(Ok(_)) if dom.len() < MIN_PLAUSIBLE_DOM_BYTES => {
            anyhow::bail!("chrome produced only {} bytes for {url}", dom.len())
        }
        Ok(Ok(_)) => Ok(dom),
    }
}

pub fn schema_types(dom: &str) -> Vec<String> {
    let mut types: Vec<String> = dom
        .match_indices("\"@type\"")
        .filter_map(|(index, _)| {
            let rest = &dom[index..];
            let value_start = rest.find(':')? + 1;
            let quoted = rest[value_start..].trim_start();
            let quoted = quoted.strip_prefix('"')?;
            let end = quoted.find('"')?;
            Some(quoted[..end].to_owned())
        })
        .collect();
    types.sort_unstable();
    types.dedup();
    types
}

fn extract_between(haystack: &str, open: &str, close: &str) -> Option<String> {
    let start = haystack.find(open)? + open.len();
    let end = haystack[start..].find(close)? + start;
    Some(haystack[start..end].trim().to_owned())
}

/// Reads an attribute from whichever tag carries `marker`, searching forwards
/// and backwards because `rel` and `href` appear in either order.
fn extract_attr(dom: &str, marker: &str, attr: &str) -> Option<String> {
    let marker_at = dom.find(marker)?;
    let tag_start = dom[..marker_at].rfind('<')?;
    let tag_end = dom[marker_at..].find('>')? + marker_at;
    let tag = &dom[tag_start..tag_end];
    let attr_at = tag.find(attr)? + attr.len();
    let end = tag[attr_at..].find('"')? + attr_at;
    Some(tag[attr_at..end].to_owned())
}
