// Copyright 2024-2025 AllSource Team
// Licensed under the Business Source License 1.1 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     See LICENSE-BSL in the repository root
//
// Change Date: 2029-03-01
// Change License: Apache License, Version 2.0

//! WAL Shipper — streams WAL entries from the leader to connected followers.
//!
//! Opens a TCP listener on the replication port and accepts follower connections.
//! Each follower sends a `Subscribe` message with its last known WAL offset, then
//! receives a stream of `WalEntry` messages as the leader processes writes.
//!
//! ## Replication modes
//!
//! - **Async** (default): leader returns 200 immediately after local WAL write.
//! - **SemiSync**: leader waits for at least 1 follower ACK before returning 200.
//!   If the ACK timeout expires, the write still succeeds (degraded mode with warning).
//! - **Sync**: leader waits for ALL followers to ACK before returning 200.
//!
//! ## Catch-up protocol
//!
//! When a follower subscribes with an offset older than the leader's oldest WAL
//! entry, the shipper initiates a Parquet snapshot transfer:
//! 1. Send `SnapshotStart` with the list of Parquet files
//! 2. Stream each file as `SnapshotChunk` messages (base64-encoded, 512KB chunks)
//! 3. Send `SnapshotEnd` with the WAL offset to resume from
//! 4. Resume normal WAL streaming from that offset
//!
//! If the follower is only slightly behind (within WAL range), catch-up happens
//! via normal WAL streaming without a snapshot transfer.

use crate::{
    infrastructure::{
        observability::metrics::MetricsRegistry,
        persistence::wal::WALEntry,
        replication::protocol::{FollowerMessage, LeaderMessage},
    },
    store::EventStore,
};
use dashmap::DashMap;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::{Notify, broadcast},
};
use uuid::Uuid;

/// Replication mode controlling write acknowledgement behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplicationMode {
    /// Leader returns 200 immediately after local WAL write (default).
    Async,
    /// Leader waits for at least 1 follower ACK before returning 200.
    /// Falls back to async (with warning) if ACK timeout expires.
    SemiSync,
    /// Leader waits for ALL followers to ACK before returning 200.
    Sync,
}

impl ReplicationMode {
    /// Parse from environment variable value.
    pub fn from_str_value(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "semi-sync" | "semi_sync" | "semisync" => ReplicationMode::SemiSync,
            "sync" => ReplicationMode::Sync,
            _ => ReplicationMode::Async,
        }
    }
}

impl std::fmt::Display for ReplicationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplicationMode::Async => write!(f, "async"),
            ReplicationMode::SemiSync => write!(f, "semi-sync"),
            ReplicationMode::Sync => write!(f, "sync"),
        }
    }
}

/// Size of each Parquet file chunk for snapshot transfer (512 KB).
const SNAPSHOT_CHUNK_SIZE: usize = 512 * 1024;

/// Tracks a connected follower's state.
struct FollowerState {
    /// Last WAL offset acknowledged by this follower.
    acked_offset: u64,
    /// When this follower connected.
    connected_at: Instant,
}

/// Shared replication status exposed via the health endpoint.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReplicationStatus {
    pub followers: usize,
    pub min_lag_ms: u64,
    pub max_lag_ms: u64,
    pub replication_mode: ReplicationMode,
}

/// WAL Shipper manages the replication TCP server and follower connections.
pub struct WalShipper {
    /// Broadcast sender for new WAL entries — the WAL pushes entries here.
    entry_tx: broadcast::Sender<WALEntry>,
    /// Connected followers keyed by a unique ID.
    followers: Arc<DashMap<Uuid, FollowerState>>,
    /// Current leader WAL sequence (updated on each broadcast).
    leader_offset: Arc<std::sync::atomic::AtomicU64>,
    /// Reference to the EventStore for catch-up (Parquet files + WAL oldest offset).
    store: Option<Arc<EventStore>>,
    /// Prometheus metrics registry for replication counters/gauges.
    metrics: Option<Arc<MetricsRegistry>>,
    /// Replication mode (async, semi-sync, sync).
    replication_mode: ReplicationMode,
    /// Timeout for waiting for follower ACKs in semi-sync/sync modes.
    ack_timeout: Duration,
    /// Notification channel for ACK arrivals — wakes up `wait_for_ack` callers.
    ack_notify: Arc<Notify>,
}

impl WalShipper {
    /// Create a new WAL shipper. Returns the shipper and a broadcast sender
    /// that should be given to the WAL for publishing new entries.
    pub fn new() -> (Self, broadcast::Sender<WALEntry>) {
        let (entry_tx, _) = broadcast::channel(4096);
        let tx_clone = entry_tx.clone();
        (
            Self {
                entry_tx,
                followers: Arc::new(DashMap::new()),
                leader_offset: Arc::new(std::sync::atomic::AtomicU64::new(0)),
                store: None,
                metrics: None,
                replication_mode: ReplicationMode::Async,
                ack_timeout: Duration::from_secs(5),
                ack_notify: Arc::new(Notify::new()),
            },
            tx_clone,
        )
    }

    /// Set the replication mode and ACK timeout.
    pub fn set_replication_mode(&mut self, mode: ReplicationMode, ack_timeout: Duration) {
        self.replication_mode = mode;
        self.ack_timeout = ack_timeout;
    }

    /// Get the current replication mode.
    pub fn replication_mode(&self) -> ReplicationMode {
        self.replication_mode
    }

    /// Get the current leader WAL offset.
    pub fn current_leader_offset(&self) -> u64 {
        self.leader_offset
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Wait for follower ACK(s) up to the given WAL offset.
    ///
    /// Behavior depends on replication mode:
    /// - **Async**: returns immediately.
    /// - **SemiSync**: waits until at least 1 follower has ACKed `target_offset`,
    ///   or the timeout expires (returns `true` on success, `false` on timeout).
    /// - **Sync**: waits until ALL connected followers have ACKed `target_offset`,
    ///   or the timeout expires.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub async fn wait_for_ack(&self, target_offset: u64) -> bool {
        match self.replication_mode {
            ReplicationMode::Async => true,
            ReplicationMode::SemiSync => self.wait_for_ack_inner(target_offset, false).await,
            ReplicationMode::Sync => self.wait_for_ack_inner(target_offset, true).await,
        }
    }

    /// Inner ACK wait loop. If `all_followers` is true, waits for ALL followers;
    /// otherwise waits for at least 1.
    async fn wait_for_ack_inner(&self, target_offset: u64, all_followers: bool) -> bool {
        let start = Instant::now();
        let timeout = self.ack_timeout;

        loop {
            // Check current ACK state
            let follower_count = self.followers.len();
            if follower_count == 0 {
                // No followers connected — can't wait for ACKs
                return false;
            }

            if all_followers {
                // Sync: all followers must have ACKed
                let all_acked = self
                    .followers
                    .iter()
                    .all(|entry| entry.value().acked_offset >= target_offset);
                if all_acked {
                    return true;
                }
            } else {
                // Semi-sync: at least 1 follower must have ACKed
                let any_acked = self
                    .followers
                    .iter()
                    .any(|entry| entry.value().acked_offset >= target_offset);
                if any_acked {
                    return true;
                }
            }

            // Check timeout
            let elapsed = start.elapsed();
            if elapsed >= timeout {
                return false;
            }

            // Wait for next ACK notification, with remaining timeout.
            // `elapsed < timeout` is guaranteed by the early return above (and
            // `elapsed` is the captured local, not a fresh reading), so this can
            // never underflow. `saturating_sub` states that structurally instead
            // of relying on a panic-on-underflow `-` in the replication ACK path.
            let remaining = timeout.saturating_sub(elapsed);
            if tokio::time::timeout(remaining, self.ack_notify.notified())
                .await
                .is_err()
            {
                return false;
            }
        }
    }

    /// Set the Prometheus metrics registry for replication metrics.
    pub fn set_metrics(&mut self, metrics: Arc<MetricsRegistry>) {
        self.metrics = Some(metrics);
    }

    /// Attach the EventStore reference for catch-up protocol support.
    ///
    /// When set, the shipper can stream Parquet snapshot files to followers
    /// that are too far behind for WAL-only catch-up. Must be called before
    /// `serve()`.
    pub fn set_store(&mut self, store: Arc<EventStore>) {
        self.store = Some(store);
    }

    /// Get current replication status for the health endpoint.
    pub fn status(&self) -> ReplicationStatus {
        let leader_offset = self
            .leader_offset
            .load(std::sync::atomic::Ordering::Relaxed);
        let mut min_lag_ms = u64::MAX;
        let mut max_lag_ms = 0u64;

        for entry in self.followers.iter() {
            let follower = entry.value();
            let lag = leader_offset.saturating_sub(follower.acked_offset);
            min_lag_ms = min_lag_ms.min(lag);
            max_lag_ms = max_lag_ms.max(lag);
        }

        let follower_count = self.followers.len();
        if follower_count == 0 {
            min_lag_ms = 0;
        }

        ReplicationStatus {
            followers: follower_count,
            min_lag_ms,
            max_lag_ms,
            replication_mode: self.replication_mode,
        }
    }

    /// Start the replication TCP server. This runs until the process shuts down.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub async fn serve(self: Arc<Self>, port: u16) -> anyhow::Result<()> {
        let addr = format!("0.0.0.0:{port}");
        let listener = TcpListener::bind(&addr).await?;

        tracing::info!(
            "Replication server listening on {} (followers can connect)",
            addr
        );

        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    tracing::info!("Follower connected from {}", peer_addr);
                    let shipper = Arc::clone(&self);
                    tokio::spawn(async move {
                        if let Err(e) = shipper.handle_follower(stream).await {
                            tracing::warn!("Follower {} disconnected: {}", peer_addr, e);
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Failed to accept follower connection: {}", e);
                }
            }
        }
    }

    /// Determine whether a follower needs a Parquet snapshot catch-up.
    ///
    /// Returns `true` if the follower's last_offset is behind the oldest
    /// available WAL entry (meaning WAL-only catch-up is impossible).
    fn needs_snapshot_catchup(&self, last_offset: u64) -> bool {
        // A follower with offset 0 is brand new — needs snapshot if we have data
        if last_offset == 0 {
            if let Some(ref store) = self.store
                && let Some(wal) = store.wal()
            {
                return wal.current_sequence() > 0;
            }
            return false;
        }

        if let Some(ref store) = self.store
            && let Some(wal) = store.wal()
            && let Some(oldest) = wal.oldest_sequence()
        {
            return last_offset < oldest;
        }
        false
    }

    /// Stream Parquet snapshot files to a follower for catch-up.
    ///
    /// This is called when a follower is too far behind for WAL-only catch-up.
    /// After the snapshot transfer, the follower loads the Parquet files and
    /// resumes normal WAL streaming from the offset after the snapshot.
    async fn send_snapshot(
        &self,
        writer: &mut tokio::net::tcp::OwnedWriteHalf,
        peer: std::net::SocketAddr,
    ) -> anyhow::Result<u64> {
        let store = self
            .store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No store available for snapshot catch-up"))?;

        // Flush any pending Parquet batches so the snapshot is complete
        if let Err(e) = store.flush_storage() {
            tracing::warn!("Failed to flush storage before snapshot: {}", e);
        }

        let storage = store.parquet_storage().ok_or_else(|| {
            anyhow::anyhow!("No Parquet storage configured for snapshot catch-up")
        })?;

        // Collect file paths while holding the read guard, then drop it before any await
        let parquet_files = {
            let storage_guard = storage.read();
            storage_guard.list_parquet_files()?
        };

        if parquet_files.is_empty() {
            tracing::info!("No Parquet files to send for snapshot catch-up to {}", peer);
            let current_offset = self
                .leader_offset
                .load(std::sync::atomic::Ordering::Relaxed);
            return Ok(current_offset);
        }

        // Collect filenames
        let filenames: Vec<String> = parquet_files
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .collect();

        tracing::info!(
            "Sending Parquet snapshot to {} ({} files: {:?})",
            peer,
            filenames.len(),
            filenames,
        );

        // Step 1: Send SnapshotStart
        let start_msg = LeaderMessage::SnapshotStart {
            parquet_files: filenames,
        };
        send_message(writer, &start_msg).await?;

        // Step 2: Stream each Parquet file as chunks
        for file_path in &parquet_files {
            let filename = file_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let file_data = tokio::fs::read(file_path).await.map_err(|e| {
                anyhow::anyhow!("Failed to read Parquet file {}: {}", file_path.display(), e)
            })?;

            let total_size = file_data.len();
            let mut offset: usize = 0;

            while offset < total_size {
                let end = (offset + SNAPSHOT_CHUNK_SIZE).min(total_size);
                let chunk = &file_data[offset..end];
                let is_last = end >= total_size;

                use base64::Engine;
                let encoded = base64::engine::general_purpose::STANDARD.encode(chunk);

                let chunk_msg = LeaderMessage::SnapshotChunk {
                    filename: filename.clone(),
                    data: encoded,
                    chunk_offset: offset as u64,
                    is_last,
                };
                send_message(writer, &chunk_msg).await?;

                offset = end;
            }

            tracing::debug!(
                "Sent Parquet file {} ({} bytes) to {}",
                filename,
                total_size,
                peer,
            );
        }

        // The WAL offset to resume from: current leader offset at time of snapshot
        let wal_offset_after_snapshot = self
            .leader_offset
            .load(std::sync::atomic::Ordering::Relaxed);

        // Step 3: Send SnapshotEnd
        let end_msg = LeaderMessage::SnapshotEnd {
            wal_offset_after_snapshot,
        };
        send_message(writer, &end_msg).await?;

        tracing::info!(
            "Snapshot transfer complete to {}, resuming WAL from offset {}",
            peer,
            wal_offset_after_snapshot,
        );

        Ok(wal_offset_after_snapshot)
    }

    /// Handle a single follower connection.
    async fn handle_follower(self: &Arc<Self>, stream: TcpStream) -> anyhow::Result<()> {
        let peer = stream.peer_addr()?;
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        // Step 1: Read the subscribe message from the follower.
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        let subscribe_msg: FollowerMessage = serde_json::from_str(line.trim())?;
        let FollowerMessage::Subscribe { last_offset } = subscribe_msg else {
            anyhow::bail!("Expected Subscribe message, got: {subscribe_msg:?}");
        };

        tracing::info!(
            "Follower {} subscribed with last_offset={}",
            peer,
            last_offset
        );

        // Register follower
        let follower_id = Uuid::new_v4();
        self.followers.insert(
            follower_id,
            FollowerState {
                acked_offset: last_offset,
                connected_at: Instant::now(),
            },
        );

        // Update follower count metric
        if let Some(ref m) = self.metrics {
            m.replication_followers_connected
                .set(self.followers.len() as i64);
        }

        // Subscribe to the broadcast channel for new WAL entries.
        let mut entry_rx = self.entry_tx.subscribe();

        // Step 2: Determine catch-up strategy.
        let resume_offset = if self.needs_snapshot_catchup(last_offset) {
            // Follower is too far behind — send Parquet snapshot first.
            tracing::info!(
                "Follower {} needs snapshot catch-up (last_offset={}, behind WAL range)",
                peer,
                last_offset,
            );
            match self.send_snapshot(&mut writer, peer).await {
                Ok(offset) => offset,
                Err(e) => {
                    tracing::error!("Failed to send snapshot to {}: {}", peer, e);
                    self.followers.remove(&follower_id);
                    return Err(e);
                }
            }
        } else {
            // Follower is within WAL range — send CaughtUp and stream from live.
            last_offset
        };

        // Send CaughtUp to signal the follower can start processing live WAL.
        let current_offset = self
            .leader_offset
            .load(std::sync::atomic::Ordering::Relaxed);
        let caught_up = LeaderMessage::CaughtUp { current_offset };
        send_message(&mut writer, &caught_up).await?;

        // Step 3: Stream WAL entries and listen for ACKs concurrently.
        let followers = Arc::clone(&self.followers);
        let leader_offset = Arc::clone(&self.leader_offset);

        // Spawn ACK reader task
        let followers_ack = Arc::clone(&followers);
        let ack_metrics = self.metrics.clone();
        let ack_leader_offset = Arc::clone(&leader_offset);
        let ack_follower_id_str = follower_id.to_string();
        let ack_notify = Arc::clone(&self.ack_notify);
        let ack_task = tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // Connection closed
                    Ok(_) => {
                        if let Ok(FollowerMessage::Ack { offset }) =
                            serde_json::from_str(line.trim())
                            && let Some(mut f) = followers_ack.get_mut(&follower_id)
                        {
                            f.acked_offset = offset;
                            // Notify any waiters (semi-sync/sync mode)
                            ack_notify.notify_waiters();
                            if let Some(ref m) = ack_metrics {
                                m.replication_acks_total.inc();
                                let leader_off =
                                    ack_leader_offset.load(std::sync::atomic::Ordering::Relaxed);
                                let lag = leader_off.saturating_sub(offset);
                                m.replication_follower_lag_seconds
                                    .with_label_values(&[&ack_follower_id_str])
                                    .set(lag as i64);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::debug!("Error reading ACK from follower: {}", e);
                        break;
                    }
                }
            }
        });

        // Stream WAL entries to follower
        let ship_metrics = self.metrics.clone();
        let stream_result: anyhow::Result<()> = async {
            loop {
                match entry_rx.recv().await {
                    Ok(wal_entry) => {
                        let offset = wal_entry.sequence;
                        // Only send entries the follower hasn't seen
                        if offset > resume_offset {
                            leader_offset.store(offset, std::sync::atomic::Ordering::Relaxed);
                            let msg = LeaderMessage::WalEntry {
                                offset,
                                data: wal_entry,
                            };
                            let json = serde_json::to_string(&msg)?;
                            if let Some(ref m) = ship_metrics {
                                m.replication_wal_shipped_total.inc();
                                m.replication_wal_shipped_bytes_total
                                    .inc_by(json.len() as u64);
                            }
                            send_message_raw(&mut writer, json).await?;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            "Follower {} lagged by {} entries, some may be missed",
                            peer,
                            n
                        );
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!(
                            "Broadcast channel closed, stopping replication to {}",
                            peer
                        );
                        break;
                    }
                }
            }
            Ok(())
        }
        .await;

        // Cleanup
        ack_task.abort();
        self.followers.remove(&follower_id);
        if let Some(ref m) = self.metrics {
            m.replication_followers_connected
                .set(self.followers.len() as i64);
        }
        tracing::info!("Follower {} removed from active set", peer);

        stream_result
    }
}

/// Send a newline-delimited JSON message over a TCP stream.
async fn send_message(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    msg: &LeaderMessage,
) -> anyhow::Result<()> {
    let json = serde_json::to_string(msg)?;
    send_message_raw(writer, json).await
}

/// Send a pre-serialized JSON string as a newline-delimited message.
async fn send_message_raw(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    mut json: String,
) -> anyhow::Result<()> {
    json.push('\n');
    writer.write_all(json.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wal_shipper_creation() {
        let (shipper, _tx) = WalShipper::new();
        let status = shipper.status();
        assert_eq!(status.followers, 0);
        assert_eq!(status.min_lag_ms, 0);
        assert_eq!(status.max_lag_ms, 0);
    }

    #[test]
    fn test_replication_status_serialization() {
        let status = ReplicationStatus {
            followers: 2,
            min_lag_ms: 12,
            max_lag_ms: 45,
            replication_mode: ReplicationMode::Async,
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["followers"], 2);
        assert_eq!(json["min_lag_ms"], 12);
        assert_eq!(json["max_lag_ms"], 45);
        assert_eq!(json["replication_mode"], "async");
    }

    #[test]
    fn test_replication_mode_from_str() {
        assert_eq!(
            ReplicationMode::from_str_value("async"),
            ReplicationMode::Async
        );
        assert_eq!(
            ReplicationMode::from_str_value("semi-sync"),
            ReplicationMode::SemiSync
        );
        assert_eq!(
            ReplicationMode::from_str_value("semi_sync"),
            ReplicationMode::SemiSync
        );
        assert_eq!(
            ReplicationMode::from_str_value("semisync"),
            ReplicationMode::SemiSync
        );
        assert_eq!(
            ReplicationMode::from_str_value("sync"),
            ReplicationMode::Sync
        );
        assert_eq!(
            ReplicationMode::from_str_value("unknown"),
            ReplicationMode::Async
        );
    }

    #[test]
    fn test_replication_mode_display() {
        assert_eq!(ReplicationMode::Async.to_string(), "async");
        assert_eq!(ReplicationMode::SemiSync.to_string(), "semi-sync");
        assert_eq!(ReplicationMode::Sync.to_string(), "sync");
    }

    #[test]
    fn test_replication_mode_serialization() {
        let json = serde_json::to_value(ReplicationMode::SemiSync).unwrap();
        assert_eq!(json, "semi_sync");
        let json = serde_json::to_value(ReplicationMode::Sync).unwrap();
        assert_eq!(json, "sync");
        let json = serde_json::to_value(ReplicationMode::Async).unwrap();
        assert_eq!(json, "async");
    }

    #[tokio::test]
    async fn test_wait_for_ack_async_mode() {
        let (shipper, _tx) = WalShipper::new();
        // Async mode returns immediately
        assert!(shipper.wait_for_ack(100).await);
    }

    #[tokio::test]
    async fn test_wait_for_ack_semi_sync_no_followers() {
        let (mut shipper, _tx) = WalShipper::new();
        shipper.set_replication_mode(ReplicationMode::SemiSync, Duration::from_millis(100));
        // No followers → returns false (timeout)
        assert!(!shipper.wait_for_ack(1).await);
    }

    #[tokio::test]
    async fn test_broadcast_channel_delivery() {
        let (shipper, tx) = WalShipper::new();
        let mut rx = shipper.entry_tx.subscribe();

        // Create a test WAL entry
        let event = crate::test_utils::test_event("test-entity", "test.event");
        let entry = WALEntry::new(1, event);

        // Broadcast
        tx.send(entry.clone()).unwrap();

        // Receive
        let received = rx.recv().await.unwrap();
        assert_eq!(received.sequence, 1);
    }

    #[test]
    fn test_needs_snapshot_catchup_no_store() {
        let (shipper, _tx) = WalShipper::new();
        // Without a store, catch-up is never needed
        assert!(!shipper.needs_snapshot_catchup(0));
        assert!(!shipper.needs_snapshot_catchup(100));
    }

    #[test]
    fn test_needs_snapshot_catchup_with_empty_store() {
        let (mut shipper, _tx) = WalShipper::new();
        let store = Arc::new(EventStore::new());
        shipper.set_store(store);

        // Empty store with no WAL — no catch-up needed
        assert!(!shipper.needs_snapshot_catchup(0));
    }
}
