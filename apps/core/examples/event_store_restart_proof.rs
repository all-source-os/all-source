//! A synthetic inventory history, replayed across two separate processes.
//! This proves graceful restart, not crash recovery or distributed durability.
use allsource_core::embedded::{Config, EmbeddedCore, IngestEvent, Query};
use serde_json::json;
use std::{env, process::Command};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() == 1 {
        let directory = tempfile::tempdir()?;
        for phase in ["append", "replay"] {
            let status = Command::new(env::current_exe()?)
                .arg(phase)
                .arg(directory.path())
                .status()?;
            if !status.success() {
                return Err(format!("{phase} failed: {status}").into());
            }
        }
        println!("PASS: separate-process restart preserved all three events and rebuilt stock=8.");
        return Ok(());
    }
    if args.len() != 3 || !["append", "replay"].contains(&args[1].as_str()) {
        return Err("Run without arguments; the parent creates isolated temporary storage.".into());
    }
    let core = EmbeddedCore::open(Config::builder().data_dir(&args[2]).build()?).await?;
    if args[1] == "append" {
        assert!(
            core.query(Query::new().entity_id("sample-stock"))
                .await?
                .is_empty()
        );
        for (sequence, event_type, delta) in [
            (1, "stock.received", 10),
            (2, "stock.reserved", -3),
            (3, "stock.released", 1),
        ] {
            core.ingest(IngestEvent {
                entity_id: "sample-stock",
                event_type,
                payload: json!({"sequence": sequence, "delta": delta}),
                metadata: Some(json!({"synthetic": true})),
                tenant_id: None,
            })
            .await?;
        }
    }
    let mut events = core.query(Query::new().entity_id("sample-stock")).await?;
    // The application supplies sequence for this one-writer example. This is
    // not a claim about ordering concurrent writes or global database order.
    events.sort_by_key(|event| event.payload["sequence"].as_i64().unwrap());
    assert_eq!(events.len(), 3);
    let expected = [
        ("stock.received", 10),
        ("stock.reserved", -3),
        ("stock.released", 1),
    ];
    let mut stock = 0;
    for (index, event) in events.iter().enumerate() {
        assert_eq!(event.payload["sequence"].as_i64(), Some(index as i64 + 1));
        assert_eq!(event.event_type, expected[index].0);
        assert_eq!(event.payload["delta"].as_i64(), Some(expected[index].1));
        stock += event.payload["delta"].as_i64().unwrap();
        println!("{}: {} -> stock={stock}", args[1], event.event_type);
        if index == 1 {
            assert_eq!(stock, 7);
        }
    }
    assert_eq!(stock, 8);
    // Discarding this local accumulator and folding again demonstrates an
    // application-owned projection rebuild, not a server projection endpoint.
    let rebuilt: i64 = events
        .iter()
        .map(|e| e.payload["delta"].as_i64().unwrap())
        .sum();
    assert_eq!(rebuilt, stock);
    println!(
        "{}: rebuilt projection={rebuilt}; historical stock after event 2=7",
        args[1]
    );
    core.shutdown().await?;
    Ok(())
}
