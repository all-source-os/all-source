//! Strict archive I/O must not occupy Tokio's request workers. Admission
//! stays owned by the blocking closure, including after its caller disconnects.

use crate::{
    domain::entities::Event,
    error::{AllSourceError, Result},
    store::{EventStore, ReadScope},
};
use std::{
    sync::{
        Arc, LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::Semaphore;

static SLOTS: LazyLock<Arc<WorkPool>> = LazyLock::new(|| Arc::new(WorkPool::new(2, 16)));
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
const ADMISSION_TIMEOUT: Duration = Duration::from_millis(100);

struct WorkPool {
    active: Arc<Semaphore>,
    waiting: Arc<Semaphore>,
}

impl WorkPool {
    fn new(active: usize, waiting: usize) -> Self {
        Self {
            active: Arc::new(Semaphore::new(active)),
            waiting: Arc::new(Semaphore::new(waiting)),
        }
    }
}

pub(super) async fn append(
    store: Arc<EventStore>,
    event: Event,
    expected: Option<u64>,
) -> Result<u64> {
    if expected.is_none() {
        return store.ingest_with_expected_version(&event, expected);
    }
    run(Arc::clone(&SLOTS), RESPONSE_TIMEOUT, move |cancellation| {
        store.prepare_http_append(&event, &cancellation)?;
        store.ingest_with_expected_version_cancellable(&event, expected, Some(&cancellation))
    })
    .await
}

pub(super) async fn retained_query(
    store: Arc<EventStore>,
    tenant: String,
    entity: String,
    limit: usize,
) -> Result<(Vec<Event>, usize)> {
    run(Arc::clone(&SLOTS), RESPONSE_TIMEOUT, move |cancellation| {
        store.prepare_http_archive(&tenant, &cancellation)?;
        // Core is an internal service. Its gateway supplies the authoritative
        // tenant; entity-level customer grants are enforced before this call.
        store.query_retained_entity_cancellable(
            &tenant,
            &entity,
            limit,
            &ReadScope::unrestricted(),
            Some(&cancellation),
        )
    })
    .await
}

struct CancelOnDrop(Arc<AtomicBool>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

async fn run<T, F>(slots: Arc<WorkPool>, timeout: Duration, work: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(Arc<AtomicBool>) -> Result<T> + Send + 'static,
{
    let deadline = tokio::time::Instant::now() + timeout;
    let permit = if let Ok(permit) = Arc::clone(&slots.active).try_acquire_owned() {
        permit
    } else {
        let _waiting = Arc::clone(&slots.waiting)
            .try_acquire_owned()
            .map_err(|_| {
                AllSourceError::QueueFull("Conditional archive admission capacity exhausted".into())
            })?;
        tokio::time::timeout_at(
            deadline.min(tokio::time::Instant::now() + ADMISSION_TIMEOUT),
            Arc::clone(&slots.active).acquire_owned(),
        )
        .await
        .map_err(|_| AllSourceError::QueueFull("Conditional archive admission timed out".into()))?
        .map_err(|_| AllSourceError::QueueFull("Conditional archive workers unavailable".into()))?
    };
    let cancellation = Arc::new(AtomicBool::new(false));
    let _cancel = CancelOnDrop(Arc::clone(&cancellation));
    let worker = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work(cancellation)
    });
    match tokio::time::timeout_at(deadline, worker).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(AllSourceError::InternalError(
            "Conditional archive worker failed".into(),
        )),
        Err(_) => Err(AllSourceError::QueueFull(
            "Strict archive response deadline exceeded; conditional append outcome may be uncertain"
                .into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn admission_queue_has_bounded_capacity_and_wait() {
        let slots = Arc::new(WorkPool::new(0, 1));
        let queued = tokio::spawn(run(Arc::clone(&slots), Duration::from_secs(1), |_| {
            panic!("no worker is available")
        }));
        // test-hang-allow: bounded observation of the queued request taking its admission slot.
        tokio::time::timeout(Duration::from_secs(1), async {
            while slots.waiting.available_permits() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let denied = run(Arc::clone(&slots), Duration::from_secs(1), |_| Ok(()))
            .await
            .unwrap_err();
        assert!(denied.to_string().contains("admission capacity exhausted"));
        let timed_out: Result<()> = queued.await.unwrap();
        assert!(
            timed_out
                .unwrap_err()
                .to_string()
                .contains("admission timed out")
        );
        assert_eq!(slots.waiting.available_permits(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn dropping_a_queued_caller_releases_only_its_queue_slot() {
        let slots = Arc::new(WorkPool::new(0, 1));
        let queued = tokio::spawn(run(Arc::clone(&slots), Duration::from_secs(1), |_| Ok(())));
        // test-hang-allow: bounded observation before cancellation.
        tokio::time::timeout(Duration::from_secs(1), async {
            while slots.waiting.available_permits() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        queued.abort();
        assert!(queued.await.unwrap_err().is_cancelled());
        assert_eq!(slots.waiting.available_permits(), 1);
        assert_eq!(slots.active.available_permits(), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn deadline_cancels_work_but_does_not_release_its_capacity() {
        let slots = Arc::new(WorkPool::new(1, 0));
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let worker = tokio::spawn(run(
            Arc::clone(&slots),
            Duration::from_millis(50),
            move |cancel| {
                started_tx.send(cancel).unwrap();
                // test-hang-allow: bounded stand-in for a kernel call that outlives HTTP.
                release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                Ok(())
            },
        ));
        let cancellation = started_rx.await.unwrap();
        let error = worker.await.unwrap().unwrap_err();
        assert!(matches!(error, AllSourceError::QueueFull(_)));
        assert!(error.to_string().contains("outcome may be uncertain"));
        assert!(cancellation.load(Ordering::Acquire));
        assert_eq!(slots.active.available_permits(), 0);
        release_tx.send(()).unwrap();
        // test-hang-allow: bounded observation of the blocking closure releasing its lease.
        let _permit = tokio::time::timeout(
            Duration::from_secs(1),
            Arc::clone(&slots.active).acquire_owned(),
        )
        .await
        .unwrap()
        .unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn blocked_archive_work_leaves_the_request_runtime_available() {
        let slots = Arc::new(WorkPool::new(1, 0));
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let worker = tokio::spawn(run(slots, Duration::from_secs(3), move |_| {
            started_tx.send(()).unwrap();
            // test-hang-allow: synthetic blocking I/O, bounded even if assertion fails.
            release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            Ok(())
        }));
        started_rx.await.unwrap();
        assert!(!worker.is_finished());
        tokio::task::yield_now().await;
        release_tx.send(()).unwrap();
        worker.await.unwrap().unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn caller_cancellation_keeps_admission_until_blocking_work_exits() {
        let slots = Arc::new(WorkPool::new(1, 0));
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let worker = tokio::spawn(run(
            Arc::clone(&slots),
            Duration::from_secs(3),
            move |cancel| {
                started_tx.send(cancel).unwrap();
                // test-hang-allow: simulate an in-flight kernel operation that cannot be cancelled.
                release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                Ok(())
            },
        ));
        let cancellation = started_rx.await.unwrap();
        worker.abort();
        assert!(worker.await.unwrap_err().is_cancelled());
        assert!(cancellation.load(Ordering::Acquire));
        assert_eq!(slots.active.available_permits(), 0);
        assert!(matches!(
            run(Arc::clone(&slots), Duration::from_secs(1), |_| Ok(())).await,
            Err(AllSourceError::QueueFull(_))
        ));
        release_tx.send(()).unwrap();
        // test-hang-allow: bounded observation of the owned blocking worker's exit.
        tokio::time::timeout(Duration::from_secs(1), async {
            while slots.active.available_permits() == 0 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
    }
}
