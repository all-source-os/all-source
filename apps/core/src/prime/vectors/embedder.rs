//! In-process (and optionally remote) text embedder for Prime.
//!
//! Wraps `fastembed`'s `TextEmbedding` (AllMiniLML6V2, 384 dims) so callers
//! that only have text can produce embedding vectors without standing up
//! a separate embedding service. This is the same model the rest of
//! AllSource uses for vector search.
//!
//! ## Where the model comes from
//!
//! Load paths, tried in this precedence order:
//!
//! 1. **Remote endpoint** *(feature `prime-remote-embed`)* — if
//!    `PRIME_EMBED_ENDPOINT` is set, text is embedded by an OpenAI- or
//!    Ollama-compatible HTTP endpoint instead of the in-process model. The HTTP
//!    client honors `HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY` / `NO_PROXY`, so
//!    pointing this at a local Ollama sidesteps every upstream-network concern.
//!    See `docs/proposals/PLUGGABLE_EMBEDDER_DESIGN.md`.
//!
//! 2. **Offline / vendored dir** — if `PRIME_EMBED_MODEL_DIR` (alias
//!    `ALLSOURCE_EMBED_MODEL_DIR`) points at a directory with the five model
//!    files, the model is loaded straight from disk with **zero network**.
//!
//! 3. **Bundled model** *(feature `prime-bundled-model`)* — the model is baked
//!    into the binary via the `allsource-prime-models` crate (fetched at build
//!    time). Fully offline at runtime with no setup.
//!
//! 4. **Network download** *(default)* — the files are auto-downloaded from
//!    HuggingFace into the fastembed cache on first use. Cache dir defaults to
//!    `.fastembed_cache/`, overridable via `FASTEMBED_CACHE_DIR`; `HF_HOME` and
//!    `HF_ENDPOINT` (mirror) are honored too. Requires outbound access the first
//!    time.

use std::path::{Path, PathBuf};

use fastembed::{
    InitOptionsUserDefined, Pooling, QuantizationMode, TextEmbedding, TokenizerFiles,
    UserDefinedEmbeddingModel,
};
// Only the network download path needs the built-in model registry types.
#[cfg(not(feature = "prime-bundled-model"))]
use fastembed::{EmbeddingModel, TextInitOptions};
use parking_lot::Mutex;

use crate::prime::error::{PrimeError, PrimeResult};

/// Output dimensionality of the default embedding model (AllMiniLML6V2).
pub const DEFAULT_EMBEDDING_DIMENSIONS: usize = 384;

/// HuggingFace repo the default model is fetched from (for diagnostics).
const MODEL_REPO: &str = "Qdrant/all-MiniLM-L6-v2-onnx";

/// Env var pointing at a local directory of vendored model files (offline path).
const MODEL_DIR_ENV: &str = "PRIME_EMBED_MODEL_DIR";
/// Back-compat alias for [`MODEL_DIR_ENV`].
const MODEL_DIR_ENV_ALIAS: &str = "ALLSOURCE_EMBED_MODEL_DIR";

/// The five files fastembed needs to build the AllMiniLML6V2 embedder offline.
const ONNX_FILE: &str = "model.onnx";
const TOKENIZER_FILE: &str = "tokenizer.json";
const CONFIG_FILE: &str = "config.json";
const SPECIAL_TOKENS_FILE: &str = "special_tokens_map.json";
const TOKENIZER_CONFIG_FILE: &str = "tokenizer_config.json";

/// Backend that actually turns text into a vector.
enum Backend {
    /// In-process fastembed model (mutex because it is `!Sync` during inference).
    /// Boxed: `TextEmbedding` is far larger than the `Remote` variant.
    Local(Box<Mutex<TextEmbedding>>),
    /// Remote HTTP embedding endpoint.
    #[cfg(feature = "prime-remote-embed")]
    Remote(remote::RemoteEmbedder),
}

/// Text → vector embedder used by `Prime::embed_text`.
pub struct TextEmbedder {
    backend: Backend,
    dimensions: usize,
}

impl TextEmbedder {
    /// Initialize the embedder, honoring the precedence documented at the top of
    /// this module. On failure the error spells out exactly what was tried,
    /// where it looked, and how to recover.
    pub fn new() -> PrimeResult<Self> {
        // 1. Remote endpoint (opt-in feature).
        #[cfg(feature = "prime-remote-embed")]
        if let Some(cfg) = remote::RemoteConfig::from_env() {
            let remote = remote::RemoteEmbedder::connect(cfg)?;
            let dimensions = remote.dimensions();
            return Ok(Self {
                backend: Backend::Remote(remote),
                dimensions,
            });
        }
        #[cfg(not(feature = "prime-remote-embed"))]
        warn_if_remote_requested();

        // 2–4. Local fastembed model (vendored dir → bundled → network download).
        let model = Self::load_local()?;
        Ok(Self {
            backend: Backend::Local(Box::new(Mutex::new(model))),
            dimensions: DEFAULT_EMBEDDING_DIMENSIONS,
        })
    }

    /// Resolve the local fastembed model per precedence steps 2–4.
    fn load_local() -> PrimeResult<TextEmbedding> {
        if let Some(dir) = resolve_model_dir() {
            return Self::try_from_dir(&dir).map_err(|e| init_error(&InitSource::Dir(dir), &e));
        }

        #[cfg(feature = "prime-bundled-model")]
        {
            Self::try_from_bundled().map_err(|e| init_error(&InitSource::Bundled, &e))
        }
        #[cfg(not(feature = "prime-bundled-model"))]
        {
            Self::try_from_network().map_err(|e| init_error(&InitSource::Network, &e))
        }
    }

    /// Load the model from a local directory of vendored files (no network).
    fn try_from_dir(dir: &Path) -> anyhow::Result<TextEmbedding> {
        let read = |name: &str| -> anyhow::Result<Vec<u8>> {
            let path = dir.join(name);
            std::fs::read(&path)
                .map_err(|e| anyhow::anyhow!("could not read {} ({e})", path.display()))
        };

        let tokenizer_files = TokenizerFiles {
            tokenizer_file: read(TOKENIZER_FILE)?,
            config_file: read(CONFIG_FILE)?,
            special_tokens_map_file: read(SPECIAL_TOKENS_FILE)?,
            tokenizer_config_file: read(TOKENIZER_CONFIG_FILE)?,
        };

        build_user_defined(read(ONNX_FILE)?, tokenizer_files)
    }

    /// Load the model from bytes baked into the binary at build time.
    #[cfg(feature = "prime-bundled-model")]
    fn try_from_bundled() -> anyhow::Result<TextEmbedding> {
        let tokenizer_files = TokenizerFiles {
            tokenizer_file: allsource_prime_models::tokenizer_json().to_vec(),
            config_file: allsource_prime_models::config_json().to_vec(),
            special_tokens_map_file: allsource_prime_models::special_tokens_map_json().to_vec(),
            tokenizer_config_file: allsource_prime_models::tokenizer_config_json().to_vec(),
        };
        build_user_defined(allsource_prime_models::onnx().to_vec(), tokenizer_files)
    }

    /// Load the model via fastembed's HuggingFace download path.
    #[cfg(not(feature = "prime-bundled-model"))]
    fn try_from_network() -> anyhow::Result<TextEmbedding> {
        TextEmbedding::try_new(
            TextInitOptions::new(EmbeddingModel::AllMiniLML6V2).with_show_download_progress(false),
        )
    }

    /// Embed a single string. Returns a `dimensions()`-length vector.
    pub fn embed(&self, text: &str) -> PrimeResult<Vec<f32>> {
        match &self.backend {
            Backend::Local(model) => {
                let mut out = model
                    .lock()
                    .embed(vec![text], None)
                    .map_err(|e| PrimeError::CoreError(anyhow::anyhow!("embedding failed: {e}")))?;
                out.pop().ok_or_else(|| {
                    PrimeError::CoreError(anyhow::anyhow!("embedder produced no output"))
                })
            }
            #[cfg(feature = "prime-remote-embed")]
            Backend::Remote(remote) => remote.embed(text),
        }
    }

    /// Embedding dimensionality for the configured model/backend.
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }
}

/// Build a fastembed embedder from raw model + tokenizer bytes.
///
/// AllMiniLML6V2 uses mean pooling and is not quantized — match what the network
/// path (`TextEmbedding::try_new`) configures so vendored/bundled and online
/// embeddings are identical.
fn build_user_defined(
    onnx: Vec<u8>,
    tokenizer_files: TokenizerFiles,
) -> anyhow::Result<TextEmbedding> {
    let model = UserDefinedEmbeddingModel::new(onnx, tokenizer_files)
        .with_pooling(Pooling::Mean)
        .with_quantization(QuantizationMode::None);
    TextEmbedding::try_new_from_user_defined(model, InitOptionsUserDefined::new())
}

/// Resolve the offline model directory from env, if configured.
fn resolve_model_dir() -> Option<PathBuf> {
    for var in [MODEL_DIR_ENV, MODEL_DIR_ENV_ALIAS] {
        if let Ok(dir) = std::env::var(var) {
            let dir = dir.trim();
            if !dir.is_empty() {
                return Some(PathBuf::from(dir));
            }
        }
    }
    None
}

/// The fastembed cache directory that the network path reads/writes.
fn cache_dir() -> String {
    std::env::var("FASTEMBED_CACHE_DIR").unwrap_or_else(|_| ".fastembed_cache".to_string())
}

/// Warn (once, at init) if a remote endpoint was requested but the feature that
/// implements it was compiled out — otherwise the env var silently does nothing.
#[cfg(not(feature = "prime-remote-embed"))]
fn warn_if_remote_requested() {
    let set = std::env::var("PRIME_EMBED_ENDPOINT")
        .ok()
        .is_some_and(|s| !s.trim().is_empty());
    if set {
        tracing::warn!(
            "PRIME_EMBED_ENDPOINT is set but this binary was built without the \
             `prime-remote-embed` feature — ignoring it and using the in-process embedder."
        );
    }
}

/// Which load path produced an init failure, for the error message.
enum InitSource {
    Dir(PathBuf),
    #[cfg(feature = "prime-bundled-model")]
    Bundled,
    #[cfg(not(feature = "prime-bundled-model"))]
    Network,
}

/// Build an actionable error from a failed embedder init.
///
/// The previous message — `failed to initialize embedder: Failed to retrieve
/// model.onnx` — distinguished none of the distinct failure modes. This one
/// names the source it tried, the path/URL involved, and concrete recovery
/// steps including the bring-your-own-vector escape hatch.
fn init_error(source: &InitSource, cause: &anyhow::Error) -> PrimeError {
    let msg = match source {
        InitSource::Dir(dir) => format!(
            "failed to initialize embedder from {env}={dir} — {cause}\n\
             Looked for these files in that directory: {onnx}, {tok}, {cfg}, {special}, {tok_cfg}.\n\
             To fix:\n\
             • Confirm all five files exist in {dir} (populate it by running `allsource-prime --mode warm` \
             once with network access, then copy the fastembed cache snapshot dir here).\n\
             • Or unset {env} to fall back to the bundled/network model.\n\
             • Or skip the embedder entirely and supply your own 384-dim vector: \
             prime_embed {{ id, vector: [...] }} (compute it with any AllMiniLM-L6-v2 embedder).",
            env = MODEL_DIR_ENV,
            dir = dir.display(),
            onnx = ONNX_FILE,
            tok = TOKENIZER_FILE,
            cfg = CONFIG_FILE,
            special = SPECIAL_TOKENS_FILE,
            tok_cfg = TOKENIZER_CONFIG_FILE,
        ),
        #[cfg(feature = "prime-bundled-model")]
        InitSource::Bundled => format!(
            "failed to initialize the bundled embedder model — {cause}\n\
             This model is baked into the binary at build time. A failure here means the \
             embedded bytes are corrupt or incompatible with this fastembed version.\n\
             To fix: rebuild, or set {MODEL_DIR_ENV}=<dir> to load a known-good vendored model, or \
             supply your own 384-dim vector via prime_embed {{ id, vector: [...] }}.",
        ),
        #[cfg(not(feature = "prime-bundled-model"))]
        InitSource::Network => format!(
            "failed to initialize embedder (network download path) — {cause}\n\
             Tried to fetch model `{repo}` into cache dir `{cache}`.\n\
             To fix one of:\n\
             • No network / behind a proxy / on a flight: vendor the model and set {env}=<dir> \
             to load offline (run `allsource-prime --mode warm` once online to populate the cache, \
             then point {env} at it). fastembed honors HF_ENDPOINT=<mirror> and HF_HOME=<dir> too.\n\
             • Point PRIME_EMBED_ENDPOINT at an OpenAI/Ollama embeddings endpoint (requires a build \
             with the `prime-remote-embed` feature).\n\
             • Stale/partial cache: delete `{cache}` and retry.\n\
             • Don't want a network-fetched model at all: supply your own 384-dim vector via \
             prime_embed {{ id, vector: [...] }} (compute with any AllMiniLM-L6-v2 embedder).",
            repo = MODEL_REPO,
            cache = cache_dir(),
            env = MODEL_DIR_ENV,
        ),
    };
    PrimeError::CoreError(anyhow::anyhow!(msg))
}

/// Remote HTTP embedding backend (OpenAI- / Ollama-compatible).
#[cfg(feature = "prime-remote-embed")]
mod remote {
    use std::time::Duration;

    use crate::prime::error::{PrimeError, PrimeResult};

    const ENDPOINT_ENV: &str = "PRIME_EMBED_ENDPOINT";
    const API_KEY_ENV: &str = "PRIME_EMBED_API_KEY";
    const MODEL_ENV: &str = "PRIME_EMBED_MODEL";
    const PROTOCOL_ENV: &str = "PRIME_EMBED_PROTOCOL";

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Protocol {
        /// `{"model","input"}` → `{"data":[{"embedding":[...]}]}`
        OpenAi,
        /// `{"model","prompt"}` → `{"embedding":[...]}`
        Ollama,
    }

    pub(super) struct RemoteConfig {
        endpoint: String,
        api_key: Option<String>,
        model: String,
        protocol: Protocol,
    }

    impl RemoteConfig {
        /// Read remote config from env. `None` when `PRIME_EMBED_ENDPOINT` unset.
        pub(super) fn from_env() -> Option<Self> {
            let endpoint = non_empty(ENDPOINT_ENV)?;
            let api_key = non_empty(API_KEY_ENV);
            // Default model name suits a local Ollama `all-minilm`; OpenAI users
            // set PRIME_EMBED_MODEL=text-embedding-3-small etc.
            let model = non_empty(MODEL_ENV).unwrap_or_else(|| "all-minilm".to_string());
            let protocol = match non_empty(PROTOCOL_ENV).as_deref() {
                Some(p) if p.eq_ignore_ascii_case("ollama") => Protocol::Ollama,
                _ => Protocol::OpenAi,
            };
            Some(Self {
                endpoint,
                api_key,
                model,
                protocol,
            })
        }
    }

    fn non_empty(var: &str) -> Option<String> {
        std::env::var(var)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    pub(super) struct RemoteEmbedder {
        client: reqwest::Client,
        cfg: RemoteConfig,
        dimensions: usize,
    }

    impl RemoteEmbedder {
        /// Build the client and probe the endpoint once to learn the embedding
        /// dimension and fail fast with an actionable error.
        pub(super) fn connect(cfg: RemoteConfig) -> PrimeResult<Self> {
            // reqwest honors HTTPS_PROXY/HTTP_PROXY/ALL_PROXY/NO_PROXY by default.
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|e| {
                    PrimeError::CoreError(anyhow::anyhow!("failed to build HTTP client: {e}"))
                })?;

            let mut me = Self {
                client,
                cfg,
                dimensions: 0,
            };
            let probe = me
                .embed("warm")
                .map_err(|e| connect_error(&me.cfg, &anyhow::anyhow!("{e}")))?;
            if probe.is_empty() {
                return Err(connect_error(
                    &me.cfg,
                    &anyhow::anyhow!("endpoint returned an empty embedding"),
                ));
            }
            me.dimensions = probe.len();
            tracing::info!(
                endpoint = %me.cfg.endpoint,
                dims = me.dimensions,
                "Prime remote embedder connected"
            );
            Ok(me)
        }

        pub(super) fn dimensions(&self) -> usize {
            self.dimensions
        }

        pub(super) fn embed(&self, text: &str) -> PrimeResult<Vec<f32>> {
            block_on(self.embed_async(text))
        }

        async fn embed_async(&self, text: &str) -> PrimeResult<Vec<f32>> {
            let body = match self.cfg.protocol {
                Protocol::OpenAi => {
                    serde_json::json!({ "model": self.cfg.model, "input": text })
                }
                Protocol::Ollama => {
                    serde_json::json!({ "model": self.cfg.model, "prompt": text })
                }
            };

            let mut req = self.client.post(&self.cfg.endpoint).json(&body);
            if let Some(key) = &self.cfg.api_key {
                req = req.bearer_auth(key);
            }

            let resp = req.send().await.map_err(|e| {
                PrimeError::CoreError(anyhow::anyhow!(
                    "embedding request to {} failed: {e}",
                    self.cfg.endpoint
                ))
            })?;

            let status = resp.status();
            if !status.is_success() {
                let detail = resp.text().await.unwrap_or_default();
                return Err(PrimeError::CoreError(anyhow::anyhow!(
                    "embedding endpoint {} returned HTTP {status}: {}",
                    self.cfg.endpoint,
                    truncate(&detail, 300)
                )));
            }

            let json: serde_json::Value = resp.json().await.map_err(|e| {
                PrimeError::CoreError(anyhow::anyhow!(
                    "embedding endpoint {} returned non-JSON: {e}",
                    self.cfg.endpoint
                ))
            })?;

            let arr = match self.cfg.protocol {
                Protocol::OpenAi => json
                    .get("data")
                    .and_then(|d| d.get(0))
                    .and_then(|d| d.get("embedding")),
                Protocol::Ollama => json.get("embedding"),
            }
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                PrimeError::CoreError(anyhow::anyhow!(
                    "unexpected embedding response shape from {} — expected {}",
                    self.cfg.endpoint,
                    match self.cfg.protocol {
                        Protocol::OpenAi => "data[0].embedding",
                        Protocol::Ollama => "embedding",
                    }
                ))
            })?;

            Ok(arr
                .iter()
                .filter_map(|x| x.as_f64().map(|f| f as f32))
                .collect())
        }
    }

    fn connect_error(cfg: &RemoteConfig, cause: &anyhow::Error) -> PrimeError {
        PrimeError::CoreError(anyhow::anyhow!(
            "failed to connect Prime remote embedder at {endpoint} (model `{model}`) — {cause}\n\
             To fix:\n\
             • Confirm {endpoint_env} is reachable and the model name ({model_env}) is correct.\n\
             • For an API that needs auth, set {api_key_env}.\n\
             • For Ollama, set {protocol_env}=ollama (default is OpenAI-compatible).\n\
             • Behind a proxy: HTTPS_PROXY/HTTP_PROXY/ALL_PROXY/NO_PROXY are honored.\n\
             • Or unset {endpoint_env} to use the in-process model.",
            endpoint = cfg.endpoint,
            model = cfg.model,
            endpoint_env = ENDPOINT_ENV,
            model_env = MODEL_ENV,
            api_key_env = API_KEY_ENV,
            protocol_env = PROTOCOL_ENV,
        ))
    }

    fn truncate(s: &str, max: usize) -> String {
        if s.len() <= max {
            s.to_string()
        } else {
            format!("{}…", &s[..max])
        }
    }

    /// Run a future to completion from a synchronous context.
    ///
    /// `Prime::embed_text` is sync but is called from inside the async MCP/HTTP
    /// handlers, so a plain blocking client would panic ("Cannot start a runtime
    /// from within a runtime"). When already on a (multi-threaded) runtime we use
    /// `block_in_place` + `block_on`; otherwise (tests, sync callers) we spin up a
    /// temporary current-thread runtime. Requires the multi-threaded flavor when
    /// called on a runtime worker — `allsource-prime`'s `#[tokio::main]` provides it.
    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        use tokio::runtime::Handle;
        match Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(move || handle.block_on(fut)),
            Err(_) => tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build temporary tokio runtime for remote embed")
                .block_on(fut),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_var_names_are_stable() {
        assert_eq!(MODEL_DIR_ENV, "PRIME_EMBED_MODEL_DIR");
        assert_eq!(MODEL_DIR_ENV_ALIAS, "ALLSOURCE_EMBED_MODEL_DIR");
    }

    #[test]
    fn offline_dir_error_is_actionable() {
        let dir = PathBuf::from("/nonexistent/prime-model-dir");
        let cause = anyhow::anyhow!(
            "could not read /nonexistent/prime-model-dir/model.onnx (No such file)"
        );
        let err = init_error(&InitSource::Dir(dir), &cause);
        let s = err.to_string();
        assert!(s.contains("PRIME_EMBED_MODEL_DIR"), "missing env var: {s}");
        assert!(s.contains("model.onnx"), "missing file list: {s}");
        assert!(s.contains("vector: [...]"), "missing escape hatch: {s}");
    }

    #[cfg(not(feature = "prime-bundled-model"))]
    #[test]
    fn network_error_is_actionable() {
        let cause = anyhow::anyhow!("Failed to retrieve model.onnx");
        let err = init_error(&InitSource::Network, &cause);
        let s = err.to_string();
        assert!(s.contains(MODEL_REPO), "missing repo: {s}");
        assert!(s.contains("HF_ENDPOINT"), "missing mirror hint: {s}");
        assert!(
            s.contains("PRIME_EMBED_MODEL_DIR"),
            "missing offline hint: {s}"
        );
        assert!(s.contains("vector: [...]"), "missing escape hatch: {s}");
    }

    #[cfg(feature = "prime-bundled-model")]
    #[test]
    fn bundled_dimensions_match() {
        assert_eq!(
            allsource_prime_models::DIMENSIONS,
            DEFAULT_EMBEDDING_DIMENSIONS
        );
    }

    // These tests download the embedding model on first run (~25 MB) and
    // need network + a writable fastembed cache. Marked `#[ignore]` so the
    // default `cargo test` is offline-safe; run with `cargo test -- --ignored`.

    #[test]
    #[ignore = "downloads model — run with --ignored"]
    fn embed_returns_expected_dimensions() {
        let embedder = TextEmbedder::new().expect("embedder init");
        let v = embedder.embed("hello world").expect("embed");
        assert_eq!(v.len(), DEFAULT_EMBEDDING_DIMENSIONS);
        assert_eq!(v.len(), embedder.dimensions());
    }

    #[test]
    #[ignore = "downloads model — run with --ignored"]
    fn embed_is_deterministic_for_same_input() {
        let embedder = TextEmbedder::new().expect("embedder init");
        let a = embedder.embed("test sentence").expect("embed a");
        let b = embedder.embed("test sentence").expect("embed b");
        assert_eq!(a, b);
    }

    #[test]
    #[ignore = "downloads model — run with --ignored"]
    fn embed_differs_for_different_input() {
        let embedder = TextEmbedder::new().expect("embedder init");
        let a = embedder.embed("project status update").expect("embed a");
        let b = embedder.embed("apple pie recipe").expect("embed b");
        assert_ne!(a, b);
    }
}
