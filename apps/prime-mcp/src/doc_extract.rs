//! LLM semantic extractor for Prime Hound — turn prose (docs, READMEs, design
//! notes) into graph nodes + edges + vectors. Tree-sitter handles code; English
//! has no AST, so unstructured text goes to an LLM that returns the entities and
//! relationships it describes, which we fold into the same graph.
//!
//! The model is bring-your-own at runtime (an `OpenAI`-compatible chat endpoint —
//! a cloud key or a local Ollama). Hound itself stays free; your model does the
//! pass. Config is env-driven so the MCP launcher / CLI sets it once:
//!   `PRIME_LLM_ENDPOINT`  e.g. `<http://localhost:11434/v1/chat/completions>` (Ollama)
//!   `PRIME_LLM_API_KEY`   bearer token (optional for local models)
//!   `PRIME_LLM_MODEL`     model id (default: `gpt-4o-mini`)
//!
//! Extractions are tagged INFERRED (weight 0.6) — they are a model's reading of
//! the text, not AST-certain like code.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use allsource_core::prime::Prime;
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::json;

const MAX_CHUNK: usize = 6000;

const SYSTEM_PROMPT: &str = "You build a knowledge graph from documentation. \
Read the text and return ONLY a JSON object of the form \
{\"entities\":[{\"name\":\"...\",\"type\":\"...\",\"summary\":\"...\"}],\
\"relationships\":[{\"from\":\"...\",\"to\":\"...\",\"relation\":\"...\"}]}. \
Entities are the concepts, components, systems, or people the text is about. \
type is a short lowercase noun (e.g. component, service, concept, person). \
relationships connect entities by their exact `name`, with a short lowercase \
relation (e.g. depends_on, uses, part_of, owns). Output JSON only, no prose.";

/// Runtime LLM endpoint configuration (`OpenAI` chat-completions shape).
#[derive(Clone, Debug)]
pub struct LlmConfig {
    pub endpoint: String,
    pub api_key: Option<String>,
    pub model: String,
}

impl LlmConfig {
    /// Read config from the environment, or `None` if `PRIME_LLM_ENDPOINT` is unset.
    pub fn from_env() -> Option<Self> {
        let endpoint = std::env::var("PRIME_LLM_ENDPOINT")
            .ok()
            .filter(|s| !s.is_empty())?;
        Some(Self {
            endpoint,
            api_key: std::env::var("PRIME_LLM_API_KEY")
                .ok()
                .filter(|s| !s.is_empty()),
            model: std::env::var("PRIME_LLM_MODEL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "gpt-4o-mini".to_string()),
        })
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct Extraction {
    #[serde(default)]
    pub entities: Vec<Entity>,
    #[serde(default)]
    pub relationships: Vec<Relationship>,
}

#[derive(Debug, Deserialize)]
pub struct Entity {
    pub name: String,
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub summary: String,
}

#[derive(Debug, Deserialize)]
pub struct Relationship {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub relation: String,
}

#[derive(Debug, Default)]
pub struct DocSummary {
    pub files: usize,
    pub chunks: usize,
    pub entities: usize,
    pub relationships: usize,
    pub embedded: usize,
    /// Sources that couldn't be read (e.g. an image with no `tesseract`, or a
    /// malformed PDF) — skipped, not fatal.
    pub skipped: usize,
    /// LLM calls made (one per chunk) — the metered unit.
    pub llm_calls: usize,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// Token usage reported by an `OpenAI`-compatible response's `usage` block.
#[derive(Debug, Default, Clone, Copy)]
struct Usage {
    prompt: u64,
    completion: u64,
    total: u64,
}

/// Sanitize an LLM-supplied type/relation into a safe lowercase graph token.
fn safe_token(s: &str, fallback: &str) -> String {
    let t: String = s
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let t = t.trim_matches('_').to_string();
    if t.is_empty() {
        fallback.to_string()
    } else {
        t
    }
}

/// Pull the JSON object out of an LLM response (handles fenced and bare JSON
/// with leading/trailing prose) and parse it.
pub fn parse_extraction(content: &str) -> Result<Extraction> {
    let s = content.trim();
    let start = s.find('{').context("no JSON object in LLM response")?;
    let end = s.rfind('}').context("no JSON object in LLM response")?;
    if end < start {
        anyhow::bail!("malformed JSON object in LLM response");
    }
    serde_json::from_str(&s[start..=end]).context("parse extraction JSON")
}

/// Split text into chunks no larger than `MAX_CHUNK`, breaking on blank lines.
fn chunk(text: &str) -> Vec<String> {
    if text.len() <= MAX_CHUNK {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    for para in text.split("\n\n") {
        if !cur.is_empty() && cur.len() + para.len() > MAX_CHUNK {
            out.push(std::mem::take(&mut cur));
        }
        cur.push_str(para);
        cur.push_str("\n\n");
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// Recursively collect doc files (md/markdown/txt/rst/pdf), skipping hidden dirs.
fn find_docs(root: &Path, out: &mut Vec<PathBuf>) {
    if root.is_file() {
        out.push(root.to_path_buf());
        return;
    }
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            let hidden = p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'));
            if !hidden {
                find_docs(&p, out);
            }
        } else if matches!(
            p.extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some(
                "md" | "markdown" | "txt" | "rst" | "pdf"
                // images (OCR)
                | "png" | "jpg" | "jpeg" | "webp" | "gif" | "tif" | "tiff" | "bmp"
                // audio / video (transcription)
                | "wav" | "mp3" | "m4a" | "aiff" | "aif" | "flac" | "ogg"
                | "mp4" | "mov" | "webm" | "mkv" | "avi"
            )
        ) {
            out.push(p);
        }
    }
}

/// Read a document's text. PDFs go through `pdf-extract` (pure Rust); images go
/// through the `tesseract` CLI (OCR, runtime dep — no build-time linking, like
/// the git hook shells out to git); everything else is read as UTF-8. The
/// recovered text then takes the same chunk → LLM → fold path as a markdown file.
fn read_doc_text(path: &Path, transcribe_cmd: Option<&str>) -> Result<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("pdf") => pdf_extract::extract_text(path)
            .with_context(|| format!("extract text from PDF {}", path.display())),
        Some("png" | "jpg" | "jpeg" | "webp" | "gif" | "tif" | "tiff" | "bmp") => ocr_image(path),
        Some(
            "wav" | "mp3" | "m4a" | "aiff" | "aif" | "flac" | "ogg" | "mp4" | "mov" | "webm"
            | "mkv" | "avi",
        ) => {
            let cmd = transcribe_cmd.context(
                "audio/video needs a transcriber — set PRIME_TRANSCRIBE_CMD to a command that \
                 prints a transcript to stdout (e.g. a whisper.cpp wrapper)",
            )?;
            transcribe(path, cmd)
        }
        _ => std::fs::read_to_string(path)
            .with_context(|| format!("read text document {}", path.display())),
    }
}

/// Transcribe audio/video by shelling out to `PRIME_TRANSCRIBE_CMD` with the
/// media path appended (`<cmd…> <path>`), capturing stdout. Runtime dep, no
/// build-time linking — the transcriber (whisper.cpp, faster-whisper, …) is the
/// user's to install and configure, like the LLM endpoint.
fn transcribe(path: &Path, cmd: &str) -> Result<String> {
    let mut parts = cmd.split_whitespace();
    let prog = parts.next().context("PRIME_TRANSCRIBE_CMD is empty")?;
    let out = std::process::Command::new(prog)
        .args(parts)
        .arg(path)
        .output()
        .with_context(|| format!("running transcriber `{cmd}` on {}", path.display()))?;
    if !out.status.success() {
        anyhow::bail!(
            "transcriber failed on {}: {}",
            path.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// OCR an image by shelling out to the `tesseract` CLI (`tesseract <img> stdout`).
fn ocr_image(path: &Path) -> Result<String> {
    let out = std::process::Command::new("tesseract")
        .arg(path)
        .arg("stdout")
        .output()
        .with_context(|| {
            format!(
                "running `tesseract` on {} — is it installed? (e.g. `brew install tesseract`)",
                path.display()
            )
        })?;
    if !out.status.success() {
        anyhow::bail!(
            "tesseract failed on {}: {}",
            path.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// POST one chunk to the chat endpoint and parse the extraction from its reply,
/// returning the extraction plus the call's token usage (for metering/billing).
async fn call_llm(
    http: &reqwest::Client,
    cfg: &LlmConfig,
    text: &str,
) -> Result<(Extraction, Usage)> {
    let body = json!({
        "model": cfg.model,
        "temperature": 0,
        "response_format": { "type": "json_object" },
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": text },
        ],
    });
    let mut req = http.post(&cfg.endpoint).json(&body);
    if let Some(key) = &cfg.api_key {
        req = req.bearer_auth(key);
    }
    let resp = req.send().await.context("LLM request failed")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        anyhow::bail!("LLM endpoint returned HTTP {status}: {detail}");
    }
    let v: serde_json::Value = resp.json().await.context("LLM response was not JSON")?;
    let usage = Usage {
        prompt: v["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
        completion: v["usage"]["completion_tokens"].as_u64().unwrap_or(0),
        total: v["usage"]["total_tokens"].as_u64().unwrap_or(0),
    };
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .context("LLM response had no choices[0].message.content")?;
    Ok((parse_extraction(content)?, usage))
}

/// Fold one document's extraction into the graph: a `document` node, an entity
/// node per entity (embedded for hybrid recall), `mentions` edges from the doc,
/// and the model's relationships between entities. Pure over `prime` — no LLM —
/// so it is unit-testable on its own.
async fn fold(
    prime: &Prime,
    doc_path: &str,
    ex: &Extraction,
    embed: bool,
    s: &mut DocSummary,
) -> Result<()> {
    let doc_uuid = prime
        .add_node("document", json!({ "path": doc_path, "domain": "docs" }))
        .await?;
    let doc_wire = format!("node:document:{}", doc_uuid.as_str());

    let mut by_name: HashMap<String, String> = HashMap::new();
    for e in &ex.entities {
        if e.name.trim().is_empty() {
            continue;
        }
        let kind = safe_token(&e.kind, "concept");
        let uuid = prime
            .add_node(
                &kind,
                json!({ "name": e.name, "summary": e.summary, "source": doc_path, "domain": "docs" }),
            )
            .await?;
        let wire = format!("node:{kind}:{}", uuid.as_str());
        s.entities += 1;

        prime
            .add_edge_weighted(
                &doc_wire,
                &wire,
                "mentions",
                0.6,
                Some(json!({ "confidence": "INFERRED" })),
            )
            .await?;

        if embed {
            let text = if e.summary.trim().is_empty() {
                e.name.clone()
            } else {
                format!("{}: {}", e.name, e.summary)
            };
            if let Ok(vector) = prime.embed_text(&text) {
                prime.embed(&wire, Some(&text), vector).await?;
                s.embedded += 1;
            }
        }
        by_name.insert(e.name.clone(), wire);
    }

    for r in &ex.relationships {
        if let (Some(from), Some(to)) = (by_name.get(&r.from), by_name.get(&r.to)) {
            let rel = safe_token(&r.relation, "related_to");
            prime
                .add_edge_weighted(
                    from,
                    to,
                    &rel,
                    0.6,
                    Some(json!({ "confidence": "INFERRED" })),
                )
                .await?;
            s.relationships += 1;
        }
    }
    Ok(())
}

/// Extract every doc under `root` using the env-configured LLM. Errors with an
/// actionable message if `PRIME_LLM_ENDPOINT` is unset.
pub async fn extract_docs(prime: &Prime, root: &Path, embed: bool) -> Result<DocSummary> {
    let cfg = LlmConfig::from_env().context(
        "doc extraction needs an LLM — set PRIME_LLM_ENDPOINT to an OpenAI-compatible \
         chat endpoint (e.g. Ollama at http://localhost:11434/v1/chat/completions), plus \
         PRIME_LLM_MODEL and optionally PRIME_LLM_API_KEY",
    )?;
    let transcribe_cmd = std::env::var("PRIME_TRANSCRIBE_CMD")
        .ok()
        .filter(|s| !s.trim().is_empty());
    extract_docs_with(prime, root, embed, &cfg, transcribe_cmd.as_deref()).await
}

/// Extraction core, with the LLM config (and optional transcriber) injected —
/// the seam wiremock / mock-command tests use.
pub async fn extract_docs_with(
    prime: &Prime,
    root: &Path,
    embed: bool,
    cfg: &LlmConfig,
    transcribe_cmd: Option<&str>,
) -> Result<DocSummary> {
    if embed {
        prime.embed_text("warm").map_err(|e| {
            anyhow::anyhow!("embedding requested but the embedder is unavailable: {e}")
        })?;
    }

    let mut files = Vec::new();
    find_docs(root, &mut files);

    let http = reqwest::Client::new();
    let mut s = DocSummary::default();
    for path in files {
        let text = match read_doc_text(&path, transcribe_cmd) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(file = %path.display(), error = %e, "Hound docs: skipping unreadable source");
                s.skipped += 1;
                continue;
            }
        };
        if text.trim().is_empty() {
            continue;
        }
        s.files += 1;
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        for piece in chunk(&text) {
            s.chunks += 1;
            let (ex, usage) = call_llm(&http, cfg, &piece).await?;
            s.llm_calls += 1;
            s.prompt_tokens += usage.prompt;
            s.completion_tokens += usage.completion;
            s.total_tokens += usage.total;
            fold(prime, &rel, &ex, embed, &mut s).await?;
        }
    }

    // Record the metered unit (LLM calls/tokens) as a durable event the
    // control-plane can bill hosted extraction off. Tenant attribution rides the
    // sync API key. Best-effort — never fails the extraction.
    if s.llm_calls > 0 {
        emit_usage(prime, &cfg.model, &s).await;
    }
    Ok(s)
}

/// Emit a `prime.extraction.usage` event for the control-plane's billing.
async fn emit_usage(prime: &Prime, model: &str, s: &DocSummary) {
    use allsource_core::embedded::IngestEvent;
    let entity_id = format!("usage:{}", uuid::Uuid::new_v4());
    let event = IngestEvent {
        entity_id: &entity_id,
        event_type: "prime.extraction.usage",
        payload: json!({
            "kind": "doc_extraction",
            "model": model,
            "files": s.files,
            "llm_calls": s.llm_calls,
            "prompt_tokens": s.prompt_tokens,
            "completion_tokens": s.completion_tokens,
            "total_tokens": s.total_tokens,
        }),
        metadata: None,
        tenant_id: None,
    };
    if let Err(e) = prime.core().ingest(event).await {
        tracing::warn!(error = %e, "Hound docs: failed to record extraction usage");
    }
}

#[cfg(test)]
#[path = "doc_extract_pdf_tests.rs"]
mod pdf_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path as mpath},
    };

    #[test]
    fn parse_handles_fenced_and_bare_json() {
        let fenced = "```json\n{\"entities\":[{\"name\":\"Auth\",\"type\":\"service\"}],\"relationships\":[]}\n```";
        let ex = parse_extraction(fenced).unwrap();
        assert_eq!(ex.entities.len(), 1);
        assert_eq!(ex.entities[0].name, "Auth");
        assert_eq!(ex.entities[0].kind, "service");

        let prosey = "Here is the graph:\n{\"entities\":[],\"relationships\":[{\"from\":\"A\",\"to\":\"B\",\"relation\":\"uses\"}]} hope that helps";
        let ex = parse_extraction(prosey).unwrap();
        assert_eq!(ex.relationships.len(), 1);
        assert_eq!(ex.relationships[0].relation, "uses");
    }

    #[test]
    fn parse_errors_on_no_json() {
        assert!(parse_extraction("sorry, I can't do that").is_err());
    }

    #[test]
    fn safe_token_sanitizes() {
        assert_eq!(
            safe_token("Software Component", "concept"),
            "software_component"
        );
        assert_eq!(safe_token("", "concept"), "concept");
        assert_eq!(safe_token("depends-on!", "related_to"), "depends_on");
    }

    #[tokio::test]
    async fn fold_creates_doc_entities_and_relationships() {
        let prime = Prime::open_in_memory().await.unwrap();
        let ex = Extraction {
            entities: vec![
                Entity {
                    name: "LoginForm".into(),
                    kind: "component".into(),
                    summary: "the UI".into(),
                },
                Entity {
                    name: "AuthService".into(),
                    kind: "service".into(),
                    summary: "auth".into(),
                },
            ],
            relationships: vec![Relationship {
                from: "LoginForm".into(),
                to: "AuthService".into(),
                relation: "calls".into(),
            }],
        };
        let mut s = DocSummary::default();
        fold(&prime, "README.md", &ex, false, &mut s).await.unwrap();
        assert_eq!(s.entities, 2);
        assert_eq!(s.relationships, 1);
        // 1 document + 2 entities = 3 nodes; 2 mentions + 1 calls = 3 edges.
        let g = prime.full_graph(None, None, None);
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.edges.len(), 3);
        assert!(g.nodes.iter().any(|n| n.node_type == "component"));
        assert!(g.edges.iter().any(|e| e.relation == "calls"));
    }

    #[tokio::test]
    async fn extract_docs_with_mocked_llm_populates_the_graph() {
        // Mock an OpenAI-compatible endpoint returning a canned extraction.
        let server = MockServer::start().await;
        let reply = json!({
            "choices": [{
                "message": {
                    "content": "{\"entities\":[{\"name\":\"Billing\",\"type\":\"service\",\"summary\":\"charges users\"},{\"name\":\"Queue\",\"type\":\"component\",\"summary\":\"async jobs\"}],\"relationships\":[{\"from\":\"Billing\",\"to\":\"Queue\",\"relation\":\"depends_on\"}]}"
                }
            }]
        });
        Mock::given(method("POST"))
            .and(mpath("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(reply))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("design.md"),
            "Billing depends on the Queue for async work.",
        )
        .unwrap();

        let prime = Prime::open_in_memory().await.unwrap();
        let cfg = LlmConfig {
            endpoint: format!("{}/v1/chat/completions", server.uri()),
            api_key: None,
            model: "test-model".into(),
        };
        let s = extract_docs_with(&prime, dir.path(), false, &cfg, None)
            .await
            .unwrap();
        assert_eq!(s.files, 1);
        assert_eq!(s.entities, 2);
        assert_eq!(s.relationships, 1);

        let g = prime.full_graph(None, None, None);
        // 1 document + 2 entities.
        assert_eq!(g.nodes.len(), 3);
        assert!(
            g.nodes
                .iter()
                .any(|n| n.properties.get("name").and_then(|v| v.as_str()) == Some("Billing"))
        );
        assert!(g.edges.iter().any(|e| e.relation == "depends_on"));
    }

    #[test]
    fn read_doc_text_extracts_pdf() {
        // A real PDF fixture; pdf-extract should recover its text.
        let bytes = include_bytes!("../tests/fixtures/sample.pdf");
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sample.pdf");
        std::fs::write(&p, bytes).unwrap();
        let text = read_doc_text(&p, None).unwrap();
        assert!(
            text.contains("AuthService"),
            "PDF text missing marker: {text:?}"
        );
    }

    #[tokio::test]
    async fn extract_docs_handles_a_pdf_through_the_llm_path() {
        let server = MockServer::start().await;
        let reply = json!({ "choices": [{ "message": {
            "content": "{\"entities\":[{\"name\":\"AuthService\",\"type\":\"service\"}],\"relationships\":[]}"
        }}]});
        Mock::given(method("POST"))
            .and(mpath("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(reply))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("design.pdf"),
            include_bytes!("../tests/fixtures/sample.pdf"),
        )
        .unwrap();

        let prime = Prime::open_in_memory().await.unwrap();
        let cfg = LlmConfig {
            endpoint: format!("{}/v1/chat/completions", server.uri()),
            api_key: None,
            model: "m".into(),
        };
        let s = extract_docs_with(&prime, dir.path(), false, &cfg, None)
            .await
            .unwrap();
        // The PDF was discovered, text-extracted, sent to the LLM, and folded.
        assert_eq!(s.files, 1);
        assert_eq!(s.entities, 1);
    }

    #[test]
    fn find_docs_includes_pdfs_images_and_media() {
        let dir = tempfile::tempdir().unwrap();
        for f in ["a.md", "b.pdf", "c.PNG", "talk.wav", "demo.mp4", "d.rs"] {
            std::fs::write(dir.path().join(f), "x").unwrap();
        }
        let mut out = Vec::new();
        find_docs(dir.path(), &mut out);
        let names: Vec<String> = out
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        for want in ["a.md", "b.pdf", "c.PNG", "talk.wav", "demo.mp4"] {
            assert!(names.contains(&want.to_string()), "missing {want}");
        }
        assert!(!names.contains(&"d.rs".to_string()), "code is not a doc");
    }

    #[test]
    fn audio_without_a_transcriber_errors() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("clip.mp3");
        std::fs::write(&p, b"x").unwrap();
        // No transcribe cmd → clear error (the loop turns this into skip + count).
        assert!(read_doc_text(&p, None).is_err());
    }

    #[tokio::test]
    async fn extract_docs_transcribes_audio_via_mock_command() {
        let server = MockServer::start().await;
        let reply = json!({ "choices": [{ "message": {
            "content": "{\"entities\":[{\"name\":\"AuthService\",\"type\":\"service\"}],\"relationships\":[]}"
        }}]});
        Mock::given(method("POST"))
            .and(mpath("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(reply))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("talk.wav"), b"RIFF-dummy").unwrap();
        // A stand-in transcriber: prints a canned transcript regardless of input.
        let script = dir.path().join("mock_whisper.sh");
        std::fs::write(&script, "#!/bin/sh\necho 'AuthService validates tokens'\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let prime = Prime::open_in_memory().await.unwrap();
        let cfg = LlmConfig {
            endpoint: format!("{}/v1/chat/completions", server.uri()),
            api_key: None,
            model: "m".into(),
        };
        let s = extract_docs_with(&prime, dir.path(), false, &cfg, script.to_str())
            .await
            .unwrap();
        // talk.wav was transcribed (mock) → LLM → folded; the .sh isn't a doc ext.
        assert_eq!(s.files, 1);
        assert_eq!(s.entities, 1);
    }

    #[test]
    #[ignore = "needs the tesseract CLI on PATH; run with --ignored"]
    fn ocr_reads_image_fixture() {
        let bytes = include_bytes!("../tests/fixtures/sample.png");
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("diagram.png");
        std::fs::write(&p, bytes).unwrap();
        let text = read_doc_text(&p, None).unwrap();
        assert!(text.contains("AuthService"), "OCR missed marker: {text:?}");
    }

    #[tokio::test]
    async fn extraction_meters_tokens_and_emits_a_usage_event() {
        use allsource_core::embedded::Query;

        let server = MockServer::start().await;
        let reply = json!({
            "choices": [{ "message": {
                "content": "{\"entities\":[{\"name\":\"X\",\"type\":\"concept\"}],\"relationships\":[]}"
            }}],
            "usage": { "prompt_tokens": 120, "completion_tokens": 30, "total_tokens": 150 }
        });
        Mock::given(method("POST"))
            .and(mpath("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(reply))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.md"), "some prose to extract").unwrap();
        let prime = Prime::open_in_memory().await.unwrap();
        let cfg = LlmConfig {
            endpoint: format!("{}/v1/chat/completions", server.uri()),
            api_key: None,
            model: "meter-model".into(),
        };
        let s = extract_docs_with(&prime, dir.path(), false, &cfg, None)
            .await
            .unwrap();

        // Usage is captured from the response's `usage` block.
        assert_eq!(s.llm_calls, 1);
        assert_eq!(s.prompt_tokens, 120);
        assert_eq!(s.total_tokens, 150);

        // And recorded as a durable, billable event.
        let events = prime
            .core()
            .query(Query::new().event_type_prefix("prime.extraction"))
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload["total_tokens"], 150);
        assert_eq!(events[0].payload["model"], "meter-model");
    }
}
