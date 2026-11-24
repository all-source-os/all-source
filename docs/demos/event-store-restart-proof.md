# Rebuild inventory from events after restarting AllSource

Status: executable example; synthetic workload, not customer evidence.

An inventory row says eight units remain. The event history explains why:
ten arrived, three were reserved, and one reservation was released.

This example stores those three facts in AllSource Core, rebuilds an
application-owned projection, shuts the writer down, and starts a separate
process against the same local data directory. The second process queries the
events and rebuilds the projection without receiving the first process's state.

## Run it

From the repository root, with the Rust toolchain installed:

```console
cargo run -p allsource-core --no-default-features --features embedded --example event_store_restart_proof
```

The parent creates an isolated temporary directory and removes it when done.
It does not connect to a hosted account or touch an existing database.
The example checks the recovered event count and sequence, historical stock
after the second event (seven), and final stock (eight). Any failed assertion
or child process makes the command fail.

## What this demonstrates

The stored events are the history; the running stock figure is a derived view.
The example intentionally discards that view and calculates it again. Replaying
only the first two events reconstructs an earlier state instead of guessing
from the final balance.

The projection here is an explicit Rust fold over queried events. It is not a
demonstration of a managed projection API. A single writer supplies the sequence
field; this does not establish concurrent ordering guarantees. The restart is
graceful and invokes shutdown. It does not test power loss, kill -9, replication,
hosted recovery objectives, exactly-once delivery, throughput or production load.

## Screen-recording script

1. Show the three event tuples in the source and label the workload synthetic.
2. Run the command. Keep the command and complete output visible.
3. Highlight the append-process projection: eight units.
4. Highlight replay-process history: ten, seven, eight. Explain that this is a
   second process reading persisted events, not a retained variable.
5. End on the PASS line only if this run produced it. Show the graceful-restart
   limitation and link the runnable source. Do not splice failed and passed runs.

Narration: “Ten units arrived. Three were reserved. One was released. AllSource
stores each change. This process rebuilds stock from those events, then exits.
A second process reads the persisted history and reconstructs the same eight
units. Stop replay after event two and stock is seven. This example uses a
graceful restart and an application-owned projection; it is not a crash test.”

