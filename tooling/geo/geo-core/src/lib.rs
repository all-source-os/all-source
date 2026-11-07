//! # geo-core
//!
//! Typed `geo.*` event payloads and the transport that writes them to
//! AllSource, for the GEO (Generative Engine Optimization) measurement
//! program for `www.all-source.xyz`.
//!
//! The authority for the event family is
//! `docs/contracts/geo-events/README.md`. Everything here is derived from it,
//! and `tests/contract_examples.rs` fails the build if the two disagree.
//!
//! ## Shape
//!
//! - [`events`] — one struct per `geo.*` payload, plus the [`GeoEvent`] union
//!   that knows each event's `entity_id` and idempotency key.
//! - [`bots`] — the versioned AI-bot taxonomy (three categories that must
//!   never be blended) and identity verification against the vendors' own
//!   published IP ranges.
//! - [`prompts`] — the frozen, versioned layer-3 probe set, compiled into the
//!   binary and digested into every `run_id`.
//! - [`probe`] — the four generative engines, normalised to one answer type.
//! - [`brand`] — who counts as us and who counts as a competitor.
//! - [`discovery`] — the canonical layer-4 "how did you find us?" vocabulary,
//!   which the web form and the Control Plane both mirror.
//! - [`scoring`] — share-of-voice scoring, Wilson intervals, source
//!   attribution and observed-vocabulary extraction.
//! - [`judge`] — the LLM-as-judge verdict vocabulary and its (auditable)
//!   reply parsing.
//! - [`emitter`] — turns a [`GeoEvent`] into a Core ingest envelope and either
//!   POSTs it through the Control Plane gateway or prints it (`--dry-run`).
//! - [`config`] — `ALLSOURCE_API_URL`, and `ALLSOURCE_API_KEY` or a registered
//!   client key (`ALLSOURCE_CLIENT_*`).
//! - [`idempotency`] — natural-key hashing, so a re-ingest of the same log
//!   window does not double-count.
//! - [`samples`] — the canonical example of every event, shared by the
//!   contract doc, the tests and the CLI's dry-run.
//!
//! ## What is deliberately absent
//!
//! No probe prompt set, no scoring formula, no notion of a measurement
//! "layer", and no log parsing. Those belong to the layer slices; the
//! *emitter* stays transport-thin so those slices can be built independently.
//! [`bots`] is the one exception and it earns its place: the taxonomy is a
//! versioned reference table that both the ingest and the report must agree
//! on, and duplicating it would let them drift.
//!
//! ## Writes go through the gateway
//!
//! GEO tooling talks to the Control Plane (`https://api.all-source.xyz`),
//! which authenticates the key, injects `tenant_id` and forwards to Core.
//! Core does not authenticate public traffic and is never addressed directly.
//! Core is the database — the events written here are durable (WAL + Parquet)
//! and survive restarts, which is what makes a 12-week trend window possible.

pub mod bots;
pub mod brand;
pub mod config;
pub mod discovery;
pub mod emitter;
pub mod error;
pub mod events;
pub mod idempotency;
pub mod judge;
pub mod probe;
pub mod prompts;
pub mod samples;
pub mod scoring;

pub use brand::{COMPETITOR_SET_VERSION, COMPETITORS, CompetitorKind, CompetitorSpec};
pub use judge::{Judgement, Verdict as JudgeVerdict};
pub use probe::{Engine, EngineConfig, EngineStatus, LlmClient, ProbeAnswer, ProbeOutcome};
pub use prompts::{Claim, Family, Intent, Prompt, PromptSet, Severity};
pub use scoring::{RateEstimate, RateTally, SourceOwner, SovScore, TermCount};

pub use bots::{
    BOTS, BotCategory, BotSpec, Cidr, PublishedPrefixes, RangeCatalog, TAXONOMY_VERSION,
    Verdict, Verification,
};
pub use config::{
    DEFAULT_API_URL, ENV_API_KEY, ENV_API_URL, ENV_ASSERTION_AUDIENCE, ENV_CLIENT_ID,
    ENV_CLIENT_KID, ENV_CLIENT_SIGNING_KEY, ENV_TENANT_ID, GeoConfig,
};
pub use discovery::{
    DISCOVERY_SOURCES, DiscoverySource, capture_path, discovery_label, discovery_source,
    is_ai_source,
};
pub use emitter::{
    EMITTER_NAME, EmitMode, EmitOutcome, GeoEmitter, IngestEnvelope, render_envelope,
};
pub use error::GeoError;
pub use events::{
    Aggregation, CrawlObserved, ENTITY_PREFIX, EVENT_TYPE_PREFIX, ExperimentScored,
    ExperimentStarted, GeoEvent, GeoEventType, InterrogationProbed, ReferralObserved,
    SCHEMA_VERSION, SelfReportCaptured, SovProbed,
};
pub use idempotency::derive_key;
