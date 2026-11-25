use super::*;
use parking_lot::Mutex;

#[derive(Default)]
struct Recorder {
    seen: Mutex<Vec<Uuid>>,
}

impl Projection for Recorder {
    fn name(&self) -> &'static str {
        "backfill_recorder"
    }
    fn process(&self, event: &Event) -> Result<()> {
        self.seen.lock().push(event.id);
        Ok(())
    }
    fn get_state(&self, _entity_id: &str) -> Option<serde_json::Value> {
        None
    }
    fn clear(&self) {
        self.seen.lock().clear();
    }
}

fn ingest(store: &EventStore, event_type: &str, entity: &str) {
    let event = Event::from_strings(
        event_type.into(),
        entity.into(),
        "default".into(),
        serde_json::json!({}),
        None,
    )
    .unwrap();
    store.ingest(&event).unwrap();
}

fn replay_ordered_ids(store: &EventStore) -> Vec<Uuid> {
    let events = store.events.read();
    let mut ordered: Vec<&Event> = events.iter().collect();
    ordered.sort_by(|a, b| replay_order(a, b));
    ordered.iter().map(|e| e.id).collect()
}

fn position_of(store: &EventStore, id: Uuid) -> BackfillPosition {
    BackfillPosition::of(store.events.read().iter().find(|e| e.id == id).unwrap())
}

fn register(
    store: &EventStore,
    after: Option<BackfillPosition>,
    prefixes: Option<&[&str]>,
) -> Arc<Recorder> {
    let recorder = Arc::new(Recorder::default());
    let projection: Arc<dyn Projection> = recorder.clone();
    match prefixes {
        Some(p) => store
            .register_projection_with_backfill_from_for(&projection, after, p)
            .unwrap(),
        None => store
            .register_projection_with_backfill_from(&projection, after)
            .unwrap(),
    }
    recorder
}

#[test]
fn resume_replays_only_events_after_the_position() {
    let store = EventStore::new();
    for i in 0..5 {
        ingest(&store, "activity.logged", &format!("step-{i}"));
    }
    let ordered = replay_ordered_ids(&store);

    let recorder = register(&store, Some(position_of(&store, ordered[1])), None);

    assert_eq!(*recorder.seen.lock(), ordered[2..]);
}

#[test]
fn resume_from_the_last_event_replays_nothing() {
    let store = EventStore::new();
    for i in 0..3 {
        ingest(&store, "activity.logged", &format!("step-{i}"));
    }
    let last = *replay_ordered_ids(&store).last().unwrap();

    let recorder = register(&store, Some(position_of(&store, last)), None);

    assert!(recorder.seen.lock().is_empty());
}

#[test]
fn no_position_replays_the_whole_history_in_order() {
    let store = EventStore::new();
    for i in 0..4 {
        ingest(&store, "activity.logged", &format!("step-{i}"));
    }

    let recorder = register(&store, None, None);

    assert_eq!(*recorder.seen.lock(), replay_ordered_ids(&store));
}

#[test]
fn position_breaks_ties_inside_one_timestamp_and_version() {
    let store = EventStore::new();
    for i in 0..6 {
        ingest(&store, "activity.logged", &format!("entity-{i}"));
    }
    {
        let mut events = store.events.write();
        let shared = events[0].timestamp;
        for event in events.iter_mut() {
            event.timestamp = shared;
            event.version = 1;
        }
    }
    let ordered = replay_ordered_ids(&store);

    let recorder = register(&store, Some(position_of(&store, ordered[2])), None);

    assert_eq!(*recorder.seen.lock(), ordered[3..]);
}

#[test]
fn replay_order_ignores_physical_pile_order() {
    let store = EventStore::new();
    for i in 0..4 {
        ingest(&store, "activity.logged", &format!("step-{i}"));
    }
    let ordered = replay_ordered_ids(&store);
    store.events.write().reverse();

    let recorder = register(&store, Some(position_of(&store, ordered[0])), None);

    assert_eq!(*recorder.seen.lock(), ordered[1..]);
}

#[test]
fn type_filter_replays_only_matching_events() {
    let store = EventStore::new();
    ingest(&store, "activity.logged", "a");
    ingest(&store, "session.started", "b");
    ingest(&store, "activity.logged", "c");
    let activity: Vec<Uuid> = {
        let events = store.events.read();
        let mut matching: Vec<&Event> = events
            .iter()
            .filter(|e| e.event_type_str() == "activity.logged")
            .collect();
        matching.sort_by(|a, b| replay_order(a, b));
        matching.iter().map(|e| e.id).collect()
    };

    let recorder = register(&store, None, Some(&["activity."]));

    assert_eq!(*recorder.seen.lock(), activity);
}

#[test]
fn type_filter_and_position_combine() {
    let store = EventStore::new();
    ingest(&store, "activity.logged", "a");
    ingest(&store, "session.started", "b");
    ingest(&store, "activity.logged", "c");
    ingest(&store, "session.started", "d");
    ingest(&store, "activity.logged", "e");
    let ordered = replay_ordered_ids(&store);
    let expected: Vec<Uuid> = ordered[2..]
        .iter()
        .copied()
        .filter(|id| {
            store
                .events
                .read()
                .iter()
                .any(|e| e.id == *id && e.event_type_str() == "activity.logged")
        })
        .collect();

    let recorder = register(
        &store,
        Some(position_of(&store, ordered[1])),
        Some(&["activity.logged"]),
    );

    assert_eq!(*recorder.seen.lock(), expected);
}

#[test]
fn events_after_registration_are_delivered_once() {
    let store = Arc::new(EventStore::new());
    for i in 0..3 {
        ingest(&store, "activity.logged", &format!("step-{i}"));
    }
    let position = position_of(&store, replay_ordered_ids(&store)[0]);

    let writer = {
        let store = Arc::clone(&store);
        std::thread::spawn(move || {
            for i in 0..200 {
                ingest(&store, "activity.logged", &format!("live-{i}"));
            }
        })
    };
    let recorder = register(&store, Some(position), None);
    writer.join().unwrap();

    let seen = recorder.seen.lock();
    let mut unique = seen.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), seen.len(), "an event was delivered twice");
    assert_eq!(seen.len(), store.events.read().len() - 1);
}

#[test]
fn position_round_trips_through_json() {
    let store = EventStore::new();
    ingest(&store, "activity.logged", "a");
    let id = replay_ordered_ids(&store)[0];
    let position = position_of(&store, id);

    let json = serde_json::to_string(&position).unwrap();

    assert_eq!(
        serde_json::from_str::<BackfillPosition>(&json).unwrap(),
        position
    );
}
