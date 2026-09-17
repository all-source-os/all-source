//! [`ProjectionWorker`] — the builder and event loop for custom projections.
//!
//! See the crate-level docs for an end-to-end example.
//!
//! This module is only available with the `projection-worker` feature.

use futures_util::{Stream, StreamExt};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    sync::{Notify, RwLock},
    task::JoinHandle,
};

use crate::{
    client::CoreClient,
    error::Error,
    types::Event,
    ws::{EventStreamClient, StreamItem, StreamMode, StreamedEvent},
};

/// Bounded state that the reducer reads and writes.
///
/// Implementors need `Default` (for fresh workers) and `Send + Sync + 'static`
/// (for the worker's tokio task). `Serialize + DeserializeOwned` are required
/// for optional state push-back to Core.
pub trait WorkerState: Default + Send + Sync + 'static + Serialize + DeserializeOwned {}

impl<T> WorkerState for T where T: Default + Send + Sync + 'static + Serialize + DeserializeOwned {}

/// Reducer closure signature: mutate `state` based on `event`, return an error
/// to abort the worker. Returning `Ok(())` on unknown event types is expected
/// (the reducer is authoritative about what it cares about).
pub type Reducer<S> = dyn FnMut(&mut S, &Event) -> Result<(), Error> + Send + 'static;

/// Extract `(entity_id, state_json)` tuples from the worker's state for
/// periodic push-back to Core. Called under a read lock — keep it cheap.
pub type StateFlusher<S> = dyn Fn(&S) -> Vec<(String, serde_json::Value)> + Send + Sync + 'static;

/// Builder for a [`ProjectionWorker`].
///
/// Create via [`ProjectionWorker::builder`], chain the setters, then call
/// [`Self::build`] to validate and produce the worker.
pub struct ProjectionWorkerBuilder<S: WorkerState> {
    core: CoreClient,
    name: Option<String>,
    event_types: Vec<String>,
    reducer: Option<Box<Reducer<S>>>,
    checkpoint_interval: u64,
    state_flusher: Option<Arc<StateFlusher<S>>>,
    state_flush_every: Option<u64>,
    state_flush_interval: Duration,
}

impl<S: WorkerState> ProjectionWorkerBuilder<S> {
    fn new(core: CoreClient) -> Self {
        Self {
            core,
            name: None,
            event_types: Vec::new(),
            reducer: None,
            checkpoint_interval: 100,
            state_flusher: None,
            state_flush_every: None,
            state_flush_interval: Duration::from_secs(5),
        }
    }

    /// Unique consumer name. Required. Used both for Core's durable-consumer
    /// registry and for disambiguating multiple workers against the same Core.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Event-type prefix filters (e.g. `["asset.", "trade."]`). Core applies
    /// these server-side during replay and live delivery.
    pub fn event_types(mut self, types: &[&str]) -> Self {
        self.event_types = types.iter().map(|s| (*s).to_string()).collect();
        self
    }

    /// Domain-specific reduction function. Required. Runs once per event, in
    /// WAL order. Errors abort the worker.
    pub fn reducer<F>(mut self, f: F) -> Self
    where
        F: FnMut(&mut S, &Event) -> Result<(), Error> + Send + 'static,
    {
        self.reducer = Some(Box::new(f));
        self
    }

    /// Save a checkpoint after every `n` processed events. Lower = more
    /// frequent restart-resume, higher = less Core traffic. Default 100.
    pub fn checkpoint_interval(mut self, n: u64) -> Self {
        self.checkpoint_interval = n.max(1);
        self
    }

    /// Configure periodic state push-back to Core.
    ///
    /// The closure maps the worker's state to a list of `(entity_id, state)`
    /// tuples that will be bulk-written to Core's projection KV. Called under
    /// a read lock on state — keep it cheap and avoid nested awaits.
    ///
    /// State push-back is off by default. Enabling it gives other consumers
    /// (or the Query Service) a way to read projected state without running
    /// the reducer themselves, at the cost of one bulk PUT per flush interval.
    pub fn state_flush_entities<F>(mut self, f: F) -> Self
    where
        F: Fn(&S) -> Vec<(String, serde_json::Value)> + Send + Sync + 'static,
    {
        self.state_flusher = Some(Arc::new(f));
        self
    }

    /// Flush state every `n` events (in addition to the time interval).
    /// Only meaningful if [`Self::state_flush_entities`] is configured.
    pub fn state_flush_every(mut self, n: u64) -> Self {
        self.state_flush_every = Some(n.max(1));
        self
    }

    /// Flush state at most every `interval`. Default 5 seconds.
    /// Only meaningful if [`Self::state_flush_entities`] is configured.
    pub fn state_flush_interval(mut self, interval: Duration) -> Self {
        self.state_flush_interval = interval;
        self
    }

    /// Validate configuration and produce the worker.
    pub fn build(self) -> Result<ProjectionWorker<S>, Error> {
        let name = self
            .name
            .ok_or_else(|| Error::Config("ProjectionWorker requires a name".into()))?;
        let reducer = self
            .reducer
            .ok_or_else(|| Error::Config("ProjectionWorker requires a reducer".into()))?;
        Ok(ProjectionWorker {
            core: self.core,
            name,
            event_types: self.event_types,
            reducer,
            checkpoint_interval: self.checkpoint_interval,
            state: Arc::new(RwLock::new(S::default())),
            last_applied_version_by_entity: HashMap::new(),
            state_flusher: self.state_flusher,
            state_flush_every: self.state_flush_every,
            state_flush_interval: self.state_flush_interval,
        })
    }
}

/// A projection worker — the first-class way to build long-lived read models
/// on AllSource.
///
/// `ProjectionWorker` subscribes to Core's event stream, runs your reducer on
/// each event, and keeps a server-tracked cursor so restarts replay only
/// events-since-last-ack rather than the full history. It's the right choice
/// over polling [`QueryClient::query_events`](crate::QueryClient::query_events)
/// whenever you want a read model that stays warm and caught-up with live
/// writes.
///
/// Construct via [`Self::builder`], then call
/// [`start()`](Self::start) to spawn a background task and get a
/// [`ProjectionHandle`] for state access and graceful shutdown.
///
/// For the full story on when to use this vs. other patterns, see the
/// [custom-projections guide](https://github.com/all-source-os/all-source/blob/main/docs/use-cases/custom-projections.md).
pub struct ProjectionWorker<S: WorkerState> {
    pub(crate) core: CoreClient,
    pub(crate) name: String,
    pub(crate) event_types: Vec<String>,
    pub(crate) reducer: Box<Reducer<S>>,
    pub(crate) checkpoint_interval: u64,
    pub(crate) state: Arc<RwLock<S>>,
    pub(crate) last_applied_version_by_entity: HashMap<String, i64>,
    pub(crate) state_flusher: Option<Arc<StateFlusher<S>>>,
    pub(crate) state_flush_every: Option<u64>,
    pub(crate) state_flush_interval: Duration,
}

impl<S: WorkerState> ProjectionWorker<S> {
    /// Start building a new worker bound to `core`.
    pub fn builder(core: CoreClient) -> ProjectionWorkerBuilder<S> {
        ProjectionWorkerBuilder::new(core)
    }

    /// Consume a stream of [`StreamItem`]s, applying each event to state.
    ///
    /// Handles:
    /// - per-entity version-based dedup (skips events with `version <=` last applied)
    /// - periodic checkpoint via `CoreClient::save_checkpoint` every
    ///   `checkpoint_interval` replay events
    /// - transition from replay → live (stops checkpointing until next replay)
    /// - propagating reducer errors upward
    ///
    /// Returns `Ok(())` when the stream ends cleanly, or the underlying error.
    /// This method is the pure event-processing loop; [`Self::start`] (US-005)
    /// wires it to a real WebSocket and manages reconnection.
    pub async fn run_with_stream<St>(&mut self, mut stream: St) -> Result<(), Error>
    where
        St: Stream<Item = Result<StreamItem, Error>> + Unpin,
    {
        let mut events_since_checkpoint: u64 = 0;
        let mut events_since_flush: u64 = 0;
        let mut last_replay_position: Option<u64> = None;

        while let Some(item) = stream.next().await {
            match item? {
                StreamItem::Event(streamed) => {
                    if !self.apply_event(&streamed).await? {
                        continue;
                    }
                    if streamed.mode == StreamMode::Replay {
                        if let Some(pos) = streamed.position {
                            last_replay_position = Some(pos);
                        }
                        events_since_checkpoint += 1;
                        events_since_flush += 1;

                        let should_flush = self
                            .state_flush_every
                            .is_some_and(|n| events_since_flush >= n);
                        if should_flush {
                            self.flush_state_if_configured().await;
                            events_since_flush = 0;
                        }
                        if events_since_checkpoint >= self.checkpoint_interval {
                            if let Some(pos) = last_replay_position {
                                self.core.save_checkpoint(&self.name, pos).await?;
                                events_since_checkpoint = 0;
                            }
                        }
                    }
                }
                StreamItem::ReplayComplete { replayed } => {
                    self.flush_state_if_configured().await;
                    events_since_flush = 0;
                    if let Some(pos) = last_replay_position {
                        self.core.save_checkpoint(&self.name, pos).await?;
                        events_since_checkpoint = 0;
                    }
                    tracing::info!(
                        worker = %self.name,
                        replayed,
                        "replay complete, entering live mode"
                    );
                }
                StreamItem::Lagged { missed } => {
                    tracing::warn!(
                        worker = %self.name,
                        missed,
                        "server broadcast lagged — events may have been dropped"
                    );
                }
            }
        }
        // Final flush on clean stream end.
        self.flush_state_if_configured().await;
        Ok(())
    }

    /// Convenience entry-point used by `run_with_stream`. The production lifecycle
    /// goes through [`flush_state`] directly to avoid holding `&self` across awaits
    /// (the reducer's Box<dyn FnMut> is not Sync, which breaks the spawned task's
    /// Send bound when `&ProjectionWorker` lives across an await point).
    pub(crate) async fn flush_state_if_configured(&mut self) {
        let flusher = self.state_flusher.clone();
        let Some(flusher) = flusher else {
            return;
        };
        let state = Arc::clone(&self.state);
        let core = self.core.clone();
        let name = self.name.clone();
        flush_state(&core, &name, &state, &flusher).await;
    }
}

/// Read state through `flusher` and bulk-write the entries to Core.
/// Errors are logged, not propagated — the next flush retries, and the
/// checkpoint stays behind a failed flush so nothing is silently lost.
async fn flush_state<S: WorkerState>(
    core: &CoreClient,
    name: &str,
    state: &Arc<RwLock<S>>,
    flusher: &Arc<StateFlusher<S>>,
) {
    let entries = {
        let guard = state.read().await;
        flusher(&*guard)
    };
    if entries.is_empty() {
        return;
    }
    match core.bulk_put_projection_state(name, &entries).await {
        Ok(()) => {
            tracing::debug!(worker = %name, entries = entries.len(), "state flushed to Core");
        }
        Err(e) => {
            tracing::error!(
                worker = %name,
                error = %e,
                entries = entries.len(),
                "state flush to Core failed — will retry next interval"
            );
        }
    }
}

impl<S: WorkerState> ProjectionWorker<S> {
    /// Apply a single event to state with per-entity version dedup.
    /// Returns true if the event was applied, false if skipped (duplicate).
    async fn apply_event(&mut self, streamed: &StreamedEvent) -> Result<bool, Error> {
        let entity_id = streamed.event.entity_id.clone();
        let version = streamed.event.version.unwrap_or(0);

        if let Some(&last) = self.last_applied_version_by_entity.get(&entity_id) {
            if version > 0 && version <= last {
                tracing::trace!(
                    worker = %self.name,
                    entity_id = %entity_id,
                    version,
                    last_applied = last,
                    "skipping duplicate event"
                );
                return Ok(false);
            }
        }

        {
            let mut state = self.state.write().await;
            (self.reducer)(&mut *state, &streamed.event)?;
        }

        if version > 0 {
            self.last_applied_version_by_entity
                .insert(entity_id, version);
        }
        Ok(true)
    }

    /// Name of this worker (the Core consumer_id).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Access the in-progress state.
    pub fn state(&self) -> Arc<RwLock<S>> {
        Arc::clone(&self.state)
    }

    /// Start the worker on a background tokio task.
    ///
    /// Registers a durable consumer with Core, opens a WebSocket subscription,
    /// and runs the reducer loop. Reconnects with exponential backoff on
    /// transport failures. Returns a [`ProjectionHandle`] for state access and
    /// graceful shutdown.
    pub async fn start(self) -> Result<ProjectionHandle<S>, Error> {
        self.core
            .register_consumer(&self.name, &self.event_types)
            .await?;

        let handle_core = self.core.clone();
        let handle_name = self.name.clone();
        let state_arc = Arc::clone(&self.state);
        let position = Arc::new(AtomicU64::new(0));
        let caught_up = Arc::new(AtomicBool::new(false));
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let shutdown_notify = Arc::new(Notify::new());
        let stopped_cleanly = Arc::new(AtomicBool::new(false));

        let task = tokio::spawn(run_forever(
            self,
            Arc::clone(&position),
            Arc::clone(&caught_up),
            Arc::clone(&shutdown_flag),
            Arc::clone(&shutdown_notify),
            Arc::clone(&stopped_cleanly),
        ));

        Ok(ProjectionHandle {
            core: handle_core,
            name: handle_name,
            state: state_arc,
            position,
            caught_up,
            shutdown_flag,
            shutdown_notify,
            stopped_cleanly,
            task: Some(task),
        })
    }
}

async fn run_forever<S: WorkerState>(
    mut worker: ProjectionWorker<S>,
    position: Arc<AtomicU64>,
    caught_up: Arc<AtomicBool>,
    shutdown_flag: Arc<AtomicBool>,
    shutdown_notify: Arc<Notify>,
    stopped_cleanly: Arc<AtomicBool>,
) {
    let mut backoff = ExpBackoff::new();

    loop {
        if shutdown_flag.load(Ordering::SeqCst) {
            break;
        }

        // The stream authenticates with the same key as the client's HTTP calls —
        // gateways in front of Core reject an empty bearer at the handshake.
        let stream_client = EventStreamClient::new(
            worker.core.transport().base_url.clone(),
            worker.core.transport().api_key.clone(),
        );

        caught_up.store(false, Ordering::SeqCst);

        match stream_client
            .connect(&worker.name, &worker.event_types)
            .await
        {
            Ok(stream) => {
                backoff.reset();
                let outcome = run_loop_with_tracking(
                    &mut worker,
                    stream,
                    &position,
                    &caught_up,
                    &shutdown_flag,
                    &shutdown_notify,
                )
                .await;
                match outcome {
                    LoopOutcome::Shutdown => {
                        stopped_cleanly.store(true, Ordering::SeqCst);
                        break;
                    }
                    LoopOutcome::Eof => {
                        tracing::warn!(
                            worker = %worker.name,
                            "event stream closed by server — reconnecting"
                        );
                    }
                    LoopOutcome::StreamError(e) => {
                        tracing::warn!(
                            worker = %worker.name,
                            error = %e,
                            "stream error — reconnecting"
                        );
                    }
                    LoopOutcome::ReducerError(e) => {
                        tracing::error!(
                            worker = %worker.name,
                            error = %e,
                            "reducer error — worker aborting"
                        );
                        break;
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    worker = %worker.name,
                    error = %e,
                    "WebSocket connect failed — backing off"
                );
            }
        }

        if shutdown_flag.load(Ordering::SeqCst) {
            break;
        }

        let delay = backoff.next();
        tracing::info!(
            worker = %worker.name,
            attempt = backoff.attempt,
            delay_ms = delay.as_millis(),
            "reconnecting"
        );
        tokio::select! {
            () = tokio::time::sleep(delay) => {}
            () = shutdown_notify.notified() => break,
        }
    }
}

enum LoopOutcome {
    Shutdown,
    Eof,
    StreamError(Error),
    ReducerError(Error),
}

async fn run_loop_with_tracking<S: WorkerState>(
    worker: &mut ProjectionWorker<S>,
    mut stream: crate::ws::EventStream,
    position: &Arc<AtomicU64>,
    caught_up: &Arc<AtomicBool>,
    shutdown_flag: &Arc<AtomicBool>,
    shutdown_notify: &Arc<Notify>,
) -> LoopOutcome {
    let mut events_since_checkpoint: u64 = 0;
    let mut events_since_flush: u64 = 0;
    let mut last_replay_position: Option<u64> = None;

    let mut flush_ticker = tokio::time::interval(worker.state_flush_interval);
    flush_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    flush_ticker.tick().await; // first tick is immediate — drain it

    loop {
        if shutdown_flag.load(Ordering::SeqCst) {
            worker.flush_state_if_configured().await;
            if let Some(pos) = last_replay_position {
                let _ = worker.core.save_checkpoint(&worker.name, pos).await;
            }
            return LoopOutcome::Shutdown;
        }

        tokio::select! {
            () = shutdown_notify.notified() => {
                worker.flush_state_if_configured().await;
                if let Some(pos) = last_replay_position {
                    let _ = worker.core.save_checkpoint(&worker.name, pos).await;
                }
                return LoopOutcome::Shutdown;
            }
            _ = flush_ticker.tick(), if worker.state_flusher.is_some() => {
                worker.flush_state_if_configured().await;
                events_since_flush = 0;
            }
            item = stream.next() => match item {
                None => {
                    worker.flush_state_if_configured().await;
                    return LoopOutcome::Eof;
                }
                Some(Err(e)) => return LoopOutcome::StreamError(e),
                Some(Ok(StreamItem::Event(streamed))) => {
                    match worker.apply_event(&streamed).await {
                        Ok(false) => continue,
                        Err(e) => return LoopOutcome::ReducerError(e),
                        Ok(true) => {}
                    }
                    if streamed.mode == StreamMode::Replay {
                        if let Some(pos) = streamed.position {
                            last_replay_position = Some(pos);
                            position.store(pos, Ordering::SeqCst);
                        }
                        events_since_checkpoint += 1;
                        events_since_flush += 1;

                        if worker.state_flush_every.is_some_and(|n| events_since_flush >= n) {
                            worker.flush_state_if_configured().await;
                            events_since_flush = 0;
                        }
                        if events_since_checkpoint >= worker.checkpoint_interval {
                            if let Some(pos) = last_replay_position {
                                if let Err(e) = worker.core.save_checkpoint(&worker.name, pos).await {
                                    return LoopOutcome::StreamError(e);
                                }
                                events_since_checkpoint = 0;
                            }
                        }
                    }
                }
                Some(Ok(StreamItem::ReplayComplete { replayed })) => {
                    worker.flush_state_if_configured().await;
                    if let Some(pos) = last_replay_position {
                        if let Err(e) = worker.core.save_checkpoint(&worker.name, pos).await {
                            return LoopOutcome::StreamError(e);
                        }
                        events_since_checkpoint = 0;
                    }
                    tracing::info!(worker = %worker.name, replayed, "replay complete");
                    caught_up.store(true, Ordering::SeqCst);
                }
                Some(Ok(StreamItem::Lagged { missed })) => {
                    tracing::warn!(worker = %worker.name, missed, "server broadcast lagged");
                }
            }
        }
    }
}

#[derive(Debug)]
struct ExpBackoff {
    attempt: u32,
    base: Duration,
    max: Duration,
}

impl ExpBackoff {
    fn new() -> Self {
        Self {
            attempt: 0,
            base: Duration::from_millis(100),
            max: Duration::from_secs(30),
        }
    }

    fn reset(&mut self) {
        self.attempt = 0;
    }

    fn next(&mut self) -> Duration {
        self.attempt += 1;
        let exp = 2u32.saturating_pow(self.attempt.saturating_sub(1));
        self.base.saturating_mul(exp).min(self.max)
    }
}

/// Handle to a running projection worker.
///
/// Dropping without calling [`Self::stop`] logs a warning and aborts the
/// background task to prevent leaks.
pub struct ProjectionHandle<S: WorkerState> {
    core: CoreClient,
    name: String,
    state: Arc<RwLock<S>>,
    position: Arc<AtomicU64>,
    caught_up: Arc<AtomicBool>,
    shutdown_flag: Arc<AtomicBool>,
    shutdown_notify: Arc<Notify>,
    stopped_cleanly: Arc<AtomicBool>,
    task: Option<JoinHandle<()>>,
}

impl<S: WorkerState> ProjectionHandle<S> {
    /// Read an entity's state from Core's projection KV.
    ///
    /// This calls `CoreClient::get_projection_state` under the hood.
    ///
    /// Core resolves the registered projection first, then falls back to the
    /// projection state cache, so the state this worker flushes is readable
    /// back even though the worker never registers a projection in Core's
    /// manager. Requires Core ≥ 0.19.1; older versions had no cache fallback
    /// and returned `Ok(None)` here.
    ///
    /// [`Self::state`] reads the in-memory reduced state with no round-trip and
    /// is the cheaper choice in-process; this method is what other processes
    /// (or other nodes) use to read the same state from Core.
    pub async fn get_state<T: DeserializeOwned>(
        &self,
        entity_id: &str,
    ) -> Result<Option<T>, Error> {
        self.core.get_projection_state(&self.name, entity_id).await
    }

    /// Direct handle to the in-memory reduced state — the state the reducer
    /// has been mutating. Hold the read guard briefly.
    pub fn state(&self) -> Arc<RwLock<S>> {
        Arc::clone(&self.state)
    }

    /// Last WAL position applied during replay. Stays at the replay-end value
    /// once in live mode (live events don't carry positions).
    pub fn current_position(&self) -> u64 {
        self.position.load(Ordering::SeqCst)
    }

    /// `true` once Core has sent `replay_complete`. Resets to `false` during
    /// reconnection attempts.
    pub fn is_caught_up(&self) -> bool {
        self.caught_up.load(Ordering::SeqCst)
    }

    /// Gracefully stop the worker: signals shutdown, awaits the task, flushes
    /// a final checkpoint.
    pub async fn stop(mut self) -> Result<(), Error> {
        self.shutdown_flag.store(true, Ordering::SeqCst);
        self.shutdown_notify.notify_waiters();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
        Ok(())
    }
}

impl<S: WorkerState> Drop for ProjectionHandle<S> {
    fn drop(&mut self) {
        if self.task.is_some() && !self.stopped_cleanly.load(Ordering::SeqCst) {
            tracing::warn!(
                worker = %self.name,
                "ProjectionHandle dropped without stop() — aborting background task"
            );
            self.shutdown_flag.store(true, Ordering::SeqCst);
            if let Some(task) = self.task.take() {
                task.abort();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ws::{StreamItem, StreamMode, StreamedEvent};
    use futures_util::stream;
    use serde_json::json;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn sample_event(entity: &str, event_type: &str, version: i64) -> Event {
        Event {
            id: format!("evt-{entity}-{version}"),
            event_type: event_type.into(),
            entity_id: entity.into(),
            payload: json!({}),
            metadata: json!({}),
            timestamp: "2026-04-17T00:00:00Z".into(),
            version: Some(version),
            tenant_id: None,
        }
    }

    fn replay(position: u64, event: Event) -> Result<StreamItem, Error> {
        Ok(StreamItem::Event(StreamedEvent {
            position: Some(position),
            event,
            mode: StreamMode::Replay,
        }))
    }

    fn live(event: Event) -> Result<StreamItem, Error> {
        Ok(StreamItem::Event(StreamedEvent {
            position: None,
            event,
            mode: StreamMode::Live,
        }))
    }

    async fn make_core(server: &MockServer) -> CoreClient {
        CoreClient::new(&server.uri(), "test").unwrap()
    }

    async fn mock_ack_endpoint(server: &MockServer) -> Arc<std::sync::atomic::AtomicU64> {
        let counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let counter_clone = Arc::clone(&counter);
        Mock::given(method("POST"))
            .and(path("/api/v1/consumers/test-worker/ack"))
            .respond_with(move |_: &wiremock::Request| {
                counter_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_json(json!({"status": "ok"}))
            })
            .mount(server)
            .await;
        counter
    }

    #[test]
    fn builder_requires_name_and_reducer() {
        let dummy = CoreClient::new("http://localhost:1", "k").unwrap();
        let err = ProjectionWorker::<Vec<String>>::builder(dummy.clone())
            .reducer(|_, _| Ok(()))
            .build()
            .err()
            .unwrap();
        assert!(matches!(err, Error::Config(_)));

        let err = ProjectionWorker::<Vec<String>>::builder(dummy)
            .name("w")
            .build()
            .err()
            .unwrap();
        assert!(matches!(err, Error::Config(_)));
    }

    #[test]
    fn builder_defaults_checkpoint_interval_to_100() {
        let dummy = CoreClient::new("http://localhost:1", "k").unwrap();
        let worker = ProjectionWorker::<Vec<String>>::builder(dummy)
            .name("w")
            .reducer(|_, _| Ok(()))
            .build()
            .unwrap();
        assert_eq!(worker.checkpoint_interval, 100);
    }

    #[test]
    fn builder_clamps_checkpoint_interval_above_zero() {
        let dummy = CoreClient::new("http://localhost:1", "k").unwrap();
        let worker = ProjectionWorker::<Vec<String>>::builder(dummy)
            .name("w")
            .reducer(|_, _| Ok(()))
            .checkpoint_interval(0)
            .build()
            .unwrap();
        assert_eq!(worker.checkpoint_interval, 1);
    }

    #[tokio::test]
    async fn runs_reducer_once_per_event() {
        let server = MockServer::start().await;
        let _acks = mock_ack_endpoint(&server).await;

        let core = make_core(&server).await;
        let mut worker = ProjectionWorker::<u64>::builder(core)
            .name("test-worker")
            .reducer(|state, _event| {
                *state += 1;
                Ok(())
            })
            .checkpoint_interval(1000) // above event count → no checkpoint fires
            .build()
            .unwrap();

        let stream = stream::iter(vec![
            replay(1, sample_event("a", "x", 1)),
            replay(2, sample_event("b", "x", 1)),
            replay(3, sample_event("c", "x", 1)),
        ]);

        worker.run_with_stream(stream).await.unwrap();
        let count = *worker.state().read().await;
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn checkpoint_fires_at_interval_boundary() {
        let server = MockServer::start().await;
        let acks = mock_ack_endpoint(&server).await;

        let core = make_core(&server).await;
        let mut worker = ProjectionWorker::<u64>::builder(core)
            .name("test-worker")
            .reducer(|state, _event| {
                *state += 1;
                Ok(())
            })
            .checkpoint_interval(5)
            .build()
            .unwrap();

        // Feed 12 replay events — expect checkpoints at 5 and 10 (2 calls).
        let events: Vec<_> = (1..=12)
            .map(|i| replay(i, sample_event(&format!("e{i}"), "x", 1)))
            .collect();
        worker.run_with_stream(stream::iter(events)).await.unwrap();

        let ack_count = acks.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            ack_count, 2,
            "expected 2 interval checkpoints, got {ack_count}"
        );
    }

    #[tokio::test]
    async fn replay_complete_triggers_final_checkpoint() {
        let server = MockServer::start().await;
        let acks = mock_ack_endpoint(&server).await;

        let core = make_core(&server).await;
        let mut worker = ProjectionWorker::<u64>::builder(core)
            .name("test-worker")
            .reducer(|state, _event| {
                *state += 1;
                Ok(())
            })
            .checkpoint_interval(1000) // no interval checkpoints
            .build()
            .unwrap();

        let items = vec![
            replay(1, sample_event("a", "x", 1)),
            replay(2, sample_event("b", "x", 1)),
            Ok(StreamItem::ReplayComplete { replayed: 2 }),
        ];
        worker.run_with_stream(stream::iter(items)).await.unwrap();

        assert_eq!(acks.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn live_events_do_not_checkpoint() {
        let server = MockServer::start().await;
        let acks = mock_ack_endpoint(&server).await;

        let core = make_core(&server).await;
        let mut worker = ProjectionWorker::<u64>::builder(core)
            .name("test-worker")
            .reducer(|state, _event| {
                *state += 1;
                Ok(())
            })
            .checkpoint_interval(1)
            .build()
            .unwrap();

        // Only live events — no position info, no checkpointing.
        let items = vec![
            Ok(StreamItem::ReplayComplete { replayed: 0 }),
            live(sample_event("a", "x", 1)),
            live(sample_event("b", "x", 1)),
            live(sample_event("c", "x", 1)),
        ];
        worker.run_with_stream(stream::iter(items)).await.unwrap();

        // Zero interval-based checkpoints during live (replay_complete had no events to checkpoint).
        assert_eq!(
            acks.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "live events should not trigger checkpoints"
        );
    }

    #[tokio::test]
    async fn deduplicates_events_by_version() {
        let server = MockServer::start().await;
        let _acks = mock_ack_endpoint(&server).await;

        let core = make_core(&server).await;
        let mut worker = ProjectionWorker::<u64>::builder(core)
            .name("test-worker")
            .reducer(|state, _event| {
                *state += 1;
                Ok(())
            })
            .checkpoint_interval(1000)
            .build()
            .unwrap();

        // Same entity, versions 1, 2, 2 (dup), 1 (older dup), 3 → count = 3
        let items = vec![
            replay(1, sample_event("entity-1", "x", 1)),
            replay(2, sample_event("entity-1", "x", 2)),
            replay(3, sample_event("entity-1", "x", 2)), // dup
            replay(4, sample_event("entity-1", "x", 1)), // older dup
            replay(5, sample_event("entity-1", "x", 3)),
        ];
        worker.run_with_stream(stream::iter(items)).await.unwrap();
        assert_eq!(*worker.state().read().await, 3);
    }

    #[tokio::test]
    async fn dedup_is_per_entity() {
        let server = MockServer::start().await;
        let _acks = mock_ack_endpoint(&server).await;

        let core = make_core(&server).await;
        let mut worker = ProjectionWorker::<u64>::builder(core)
            .name("test-worker")
            .reducer(|state, _event| {
                *state += 1;
                Ok(())
            })
            .checkpoint_interval(1000)
            .build()
            .unwrap();

        // Two different entities at version 1 — both counted.
        let items = vec![
            replay(1, sample_event("entity-a", "x", 1)),
            replay(2, sample_event("entity-b", "x", 1)),
            replay(3, sample_event("entity-a", "x", 1)), // dup of a, skip
            replay(4, sample_event("entity-b", "x", 2)), // new
        ];
        worker.run_with_stream(stream::iter(items)).await.unwrap();
        assert_eq!(*worker.state().read().await, 3);
    }

    #[test]
    fn backoff_grows_exponentially_and_clamps() {
        let mut b = ExpBackoff::new();
        let d1 = b.next();
        let d2 = b.next();
        let d3 = b.next();
        assert!(d2 > d1, "d2 ({d2:?}) should exceed d1 ({d1:?})");
        assert!(d3 > d2);
        // Walk forward past the cap.
        for _ in 0..20 {
            let _ = b.next();
        }
        let capped = b.next();
        assert!(capped <= Duration::from_secs(30));
    }

    #[test]
    fn backoff_resets_on_success() {
        let mut b = ExpBackoff::new();
        b.next();
        b.next();
        b.next();
        b.reset();
        let first_after_reset = b.next();
        // Reset must bring us back to the base delay (100ms).
        assert_eq!(first_after_reset, Duration::from_millis(100));
    }

    async fn mock_bulk_endpoint(server: &MockServer) -> Arc<std::sync::atomic::AtomicU64> {
        let counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let counter_clone = Arc::clone(&counter);
        Mock::given(method("POST"))
            .and(path("/api/v1/projections/test-worker/bulk/save"))
            .respond_with(move |_: &wiremock::Request| {
                counter_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_json(json!({"saved": 1}))
            })
            .mount(server)
            .await;
        counter
    }

    #[tokio::test]
    async fn state_flush_fires_at_event_count() {
        let server = MockServer::start().await;
        let _acks = mock_ack_endpoint(&server).await;
        let flushes = mock_bulk_endpoint(&server).await;

        let core = make_core(&server).await;
        let mut worker = ProjectionWorker::<HashMap<String, u64>>::builder(core)
            .name("test-worker")
            .reducer(|state, event| {
                *state.entry(event.entity_id.clone()).or_insert(0) += 1;
                Ok(())
            })
            .checkpoint_interval(1000)
            .state_flush_every(10)
            .state_flush_entities(|state| {
                state.iter().map(|(k, v)| (k.clone(), json!(*v))).collect()
            })
            .build()
            .unwrap();

        // 25 replay events + stream end → flush at 10, 20, and final flush on stream-end.
        let events: Vec<_> = (1..=25)
            .map(|i| replay(i, sample_event(&format!("e{i}"), "x", 1)))
            .collect();
        worker.run_with_stream(stream::iter(events)).await.unwrap();

        let flush_count = flushes.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            flush_count, 3,
            "expected 3 flushes (at 10, 20, end-of-stream), got {flush_count}"
        );
    }

    #[tokio::test]
    async fn state_flush_fires_on_replay_complete() {
        let server = MockServer::start().await;
        let _acks = mock_ack_endpoint(&server).await;
        let flushes = mock_bulk_endpoint(&server).await;

        let core = make_core(&server).await;
        let mut worker = ProjectionWorker::<HashMap<String, u64>>::builder(core)
            .name("test-worker")
            .reducer(|state, event| {
                *state.entry(event.entity_id.clone()).or_insert(0) += 1;
                Ok(())
            })
            .checkpoint_interval(1000)
            .state_flush_every(1000) // never hits the event-count trigger
            .state_flush_entities(|state| {
                state.iter().map(|(k, v)| (k.clone(), json!(*v))).collect()
            })
            .build()
            .unwrap();

        let items = vec![
            replay(1, sample_event("a", "x", 1)),
            replay(2, sample_event("b", "x", 1)),
            Ok(StreamItem::ReplayComplete { replayed: 2 }),
            replay(3, sample_event("c", "x", 1)),
        ];
        worker.run_with_stream(stream::iter(items)).await.unwrap();

        // Flushes: 1 on replay_complete + 1 final on stream end. (Event count
        // didn't trigger flushes because flush_every=1000.)
        let flush_count = flushes.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(flush_count, 2);
    }

    #[tokio::test]
    async fn flush_skipped_when_no_entries() {
        let server = MockServer::start().await;
        let _acks = mock_ack_endpoint(&server).await;
        let flushes = mock_bulk_endpoint(&server).await;

        let core = make_core(&server).await;
        let mut worker = ProjectionWorker::<HashMap<String, u64>>::builder(core)
            .name("test-worker")
            .reducer(|_, _| Ok(())) // never populates state
            .checkpoint_interval(1000)
            .state_flush_every(1)
            .state_flush_entities(|state| {
                state.iter().map(|(k, v)| (k.clone(), json!(*v))).collect()
            })
            .build()
            .unwrap();

        let events = vec![replay(1, sample_event("a", "x", 1))];
        worker.run_with_stream(stream::iter(events)).await.unwrap();

        // Reducer never populates state → flusher returns empty → no bulk PUT.
        assert_eq!(flushes.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn flush_error_does_not_abort_worker() {
        let server = MockServer::start().await;
        let _acks = mock_ack_endpoint(&server).await;
        // No mock for /bulk/save → 404, which the worker should swallow.

        let core = make_core(&server).await;
        let mut worker = ProjectionWorker::<HashMap<String, u64>>::builder(core)
            .name("test-worker")
            .reducer(|state, event| {
                *state.entry(event.entity_id.clone()).or_insert(0) += 1;
                Ok(())
            })
            .checkpoint_interval(1000)
            .state_flush_every(1)
            .state_flush_entities(|state| {
                state.iter().map(|(k, v)| (k.clone(), json!(*v))).collect()
            })
            .build()
            .unwrap();

        // 3 events with flush_every=1 — flush fails on each, but reducer keeps running.
        let events: Vec<_> = (1..=3)
            .map(|i| replay(i, sample_event(&format!("e{i}"), "x", 1)))
            .collect();
        let result = worker.run_with_stream(stream::iter(events)).await;
        assert!(result.is_ok(), "flush failure must not abort the worker");
        assert_eq!(worker.state().read().await.len(), 3);
    }

    #[tokio::test]
    async fn reducer_error_aborts_loop() {
        let server = MockServer::start().await;
        let _acks = mock_ack_endpoint(&server).await;

        let core = make_core(&server).await;
        let mut worker = ProjectionWorker::<u64>::builder(core)
            .name("test-worker")
            .reducer(|_state, event| {
                if event.entity_id == "bad" {
                    Err(Error::Config("nope".into()))
                } else {
                    Ok(())
                }
            })
            .checkpoint_interval(1000)
            .build()
            .unwrap();

        let items = vec![
            replay(1, sample_event("good", "x", 1)),
            replay(2, sample_event("bad", "x", 1)),
            replay(3, sample_event("unreached", "x", 1)),
        ];
        let err = worker
            .run_with_stream(stream::iter(items))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Config(_)));
    }

    /// The worker's WebSocket handshake must carry the same credential the
    /// client's HTTP calls use — a real listener captures the header the
    /// reconnect loop actually sends.
    // tungstenite's handshake callback fixes the Err type of the closure below, so
    // its size is not ours to shrink.
    #[allow(clippy::result_large_err)]
    #[tokio::test]
    async fn stream_handshake_carries_the_client_api_key() {
        use tokio::net::TcpListener;
        use tokio_tungstenite::tungstenite::handshake::server::{
            Request as HandshakeRequest, Response as HandshakeResponse,
        };

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut tx = Some(tx);
            let ws = tokio_tungstenite::accept_hdr_async(
                stream,
                move |req: &HandshakeRequest, resp: HandshakeResponse| {
                    let auth = req
                        .headers()
                        .get("Authorization")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    if let Some(tx) = tx.take() {
                        let _ = tx.send(auth);
                    }
                    Ok(resp)
                },
            )
            .await
            .unwrap();
            // Hold the socket open so the worker stays in its stream loop.
            tokio::time::sleep(Duration::from_secs(5)).await;
            drop(ws);
        });

        let core = CoreClient::new(&format!("http://{addr}"), "secret-key-123").unwrap();
        let worker = ProjectionWorker::<Vec<String>>::builder(core)
            .name("auth-worker")
            .event_types(&["test."])
            .reducer(|_state, _event| Ok(()))
            .build()
            .unwrap();

        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let task = tokio::spawn(run_forever(
            worker,
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicBool::new(false)),
            Arc::clone(&shutdown_flag),
            Arc::new(Notify::new()),
            Arc::new(AtomicBool::new(false)),
        ));

        let auth = tokio::time::timeout(Duration::from_secs(5), rx)
            .await
            .expect("no WebSocket handshake within 5s")
            .expect("handshake header not reported");

        shutdown_flag.store(true, Ordering::SeqCst);
        task.abort();
        server.abort();

        assert_eq!(auth, "Bearer secret-key-123");
    }
}
