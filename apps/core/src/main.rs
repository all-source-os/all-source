#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "replication")]
use allsource_core::replication::{ReplicationMode, WalReceiver, WalShipper};
use allsource_core::{
    api_v1,
    api_v1::NodeRole,
    auth::AuthManager,
    config::ServerConfig,
    domain::repositories::TenantRepository,
    infrastructure::{
        cluster::{
            ClusterManager, ClusterMember, GeoReplicationConfig, GeoReplicationManager, MemberRole,
        },
        di::ContainerBuilder,
        persistence::SystemBootstrap,
        repositories::InMemoryTenantRepository,
    },
    rate_limit::{RateLimitConfig, RateLimiter},
    resp::RespServer,
    store::EventStore,
};
use anyhow::Result;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[cfg_attr(feature = "hotpath", hotpath::main)]
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "allsource_core=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Detect node role from environment
    let role = NodeRole::from_env();

    tracing::info!(
        "🌟 AllSource Core v{} starting...",
        env!("CARGO_PKG_VERSION")
    );
    match role {
        NodeRole::Leader => tracing::info!("   Starting as LEADER"),
        NodeRole::Follower => tracing::info!("   Starting as FOLLOWER (read-only)"),
    }
    tracing::info!("   Production-ready event store with authentication & multi-tenancy");

    // Initialize components — read persistence config from environment
    let (config, mode) = allsource_core::store::EventStoreConfig::from_env();
    if mode == "in-memory" {
        tracing::warn!(
            "   Persistence: NONE (in-memory only) — set ALLSOURCE_DATA_DIR for durability"
        );
    } else {
        tracing::info!(
            "   Persistence: {} (storage_dir={:?}, wal_dir={:?})",
            mode,
            config.storage_dir,
            config.wal_dir
        );
    }
    let mut store = EventStore::with_config(config);

    // Initialize WAL replication if this is a leader with replication enabled
    #[cfg(feature = "replication")]
    let replication_enabled =
        std::env::var("ALLSOURCE_REPLICATION_ENABLED").is_ok_and(|v| v == "true");
    #[cfg(feature = "replication")]
    let replication_port: u16 = std::env::var("ALLSOURCE_REPLICATION_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3910);
    #[cfg(not(feature = "replication"))]
    let replication_port: u16 = 3910;

    // Read replication mode and ACK timeout
    #[cfg(feature = "replication")]
    let replication_mode = std::env::var("ALLSOURCE_REPLICATION_MODE")
        .map_or(ReplicationMode::Async, |v| {
            ReplicationMode::from_str_value(&v)
        });
    #[cfg(feature = "replication")]
    let ack_timeout_ms: u64 = std::env::var("ALLSOURCE_REPLICATION_ACK_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5000);

    // Create WAL shipper (but don't spawn yet — needs store Arc first for catch-up)
    #[cfg(feature = "replication")]
    let wal_shipper_raw = if replication_enabled && role == NodeRole::Leader {
        let (mut shipper, tx) = WalShipper::new();
        shipper.set_replication_mode(
            replication_mode,
            std::time::Duration::from_millis(ack_timeout_ms),
        );
        store.enable_wal_replication(tx);
        Some(shipper)
    } else {
        if replication_enabled && role == NodeRole::Follower {
            tracing::info!("   Replication enabled but this is a follower — shipper not started");
        }
        None
    };

    // Initialize system metadata (event-sourced repositories) BEFORE Arc-wrapping the store,
    // so we can set the durable consumer registry via &mut self.
    let system_data_dir = std::env::var("ALLSOURCE_SYSTEM_DATA_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var("ALLSOURCE_DATA_DIR")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|d| std::path::PathBuf::from(d).join("__system"))
        });
    let bootstrap_tenant = std::env::var("ALLSOURCE_BOOTSTRAP_TENANT")
        .ok()
        .filter(|s| !s.is_empty());
    let system_repos = SystemBootstrap::try_initialize(system_data_dir, bootstrap_tenant).await;

    // Wire durable consumer registry into EventStore before Arc wrapping
    if let Some(ref repos) = system_repos {
        store.set_consumer_registry(repos.consumer_registry.clone());
    }

    let store = Arc::new(store);

    // Populate the on-disk storage-size gauges once at boot so the dashboard's
    // "storage" card reflects reality immediately, rather than reading 0 until the
    // first checkpoint tick (or staying 0 forever if checkpointing is disabled).
    {
        let store_for_metrics = Arc::clone(&store);
        tokio::task::spawn_blocking(move || store_for_metrics.refresh_storage_metrics_now());
    }

    // Step 6: spawn the background checkpoint loop if a cadence is
    // configured. Each tick flushes pending Parquet batches and, on
    // success, truncates the WAL — bounding dirty-restart replay to
    // one interval of writes regardless of total dataset size.
    if let Some(interval) = store.checkpoint_interval() {
        let store_for_checkpoint = Arc::clone(&store);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.tick().await; // first tick fires immediately — skip it
            loop {
                ticker.tick().await;
                let store = Arc::clone(&store_for_checkpoint);
                let result = tokio::task::spawn_blocking(move || store.checkpoint()).await;
                match result {
                    Ok(Ok(())) => tracing::debug!("✅ Background checkpoint complete"),
                    Ok(Err(e)) => tracing::warn!("Background checkpoint failed: {e}"),
                    Err(e) => tracing::warn!("Background checkpoint task panicked: {e}"),
                }
            }
        });
        tracing::info!(
            "✅ Background WAL checkpoint loop started (interval: {:?})",
            interval
        );
    }

    // Step 4: the only thing that drives the per-tenant compaction sweep,
    // and so the only thing that applies retention — a tenant's TTL is
    // enforced inside the sweep, not on its own cadence (gh#321).
    if let Some(manager) = store.compaction_manager() {
        let interval = std::time::Duration::from_secs(
            manager
                .config()
                .compaction_interval_seconds
                .clamp(60, 86_400),
        );
        // Derived from the last sweep's recorded time, not from boot. Re-arming
        // the full interval on every start meant a process restarting more
        // often than the interval never reached its first tick (gh#321).
        let first_delay = manager.delay_until_next_sweep(std::time::Duration::from_secs(120));
        tracing::info!(
            last_compaction = ?manager.last_compaction(),
            first_sweep_in_secs = first_delay.as_secs(),
            "Compaction sweep scheduled"
        );
        tokio::spawn(async move {
            tokio::time::sleep(first_delay).await;
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                let manager = Arc::clone(&manager);
                // One sweep at a time. `spawn_blocking` is awaited before the
                // next tick is read, and `tokio::time::interval` drops missed
                // ticks rather than queueing them, so a sweep that outlives
                // its interval cannot overlap the next.
                let result = tokio::task::spawn_blocking(move || {
                    if !manager.should_compact() {
                        return None;
                    }
                    Some(manager.compact())
                })
                .await;
                match result {
                    Ok(None) => {}
                    Ok(Some(Ok(r))) if r.files_compacted > 0 => tracing::info!(
                        files_compacted = r.files_compacted,
                        events_compacted = r.events_compacted,
                        bytes_before = r.bytes_before,
                        bytes_after = r.bytes_after,
                        duration_ms = r.duration_ms,
                        "✅ Background compaction sweep complete"
                    ),
                    Ok(Some(Ok(_))) => {
                        tracing::debug!("Background compaction sweep found nothing to compact");
                    }
                    Ok(Some(Err(e))) => tracing::warn!("Background compaction failed: {e}"),
                    Err(e) => tracing::warn!("Background compaction task panicked: {e}"),
                }
            }
        });
        tracing::info!(
            "✅ Background compaction loop started (interval: {:?})",
            interval
        );
    }

    // Now that store is in Arc, attach it to the shipper and spawn
    #[cfg(feature = "replication")]
    let wal_shipper = if let Some(mut shipper) = wal_shipper_raw {
        shipper.set_store(Arc::clone(&store));
        shipper.set_metrics(store.metrics());
        let shipper = Arc::new(shipper);
        let shipper_clone = Arc::clone(&shipper);
        tokio::spawn(async move {
            if let Err(e) = shipper_clone.serve(replication_port).await {
                tracing::error!("Replication server error: {}", e);
            }
        });
        tracing::info!(
            "✅ WAL replication enabled on port {} (mode: {}, ack_timeout: {}ms)",
            replication_port,
            replication_mode,
            ack_timeout_ms,
        );
        Some(shipper)
    } else {
        None
    };
    #[cfg(not(feature = "replication"))]
    let wal_shipper: Option<Arc<allsource_core::replication::WalShipper>> = None;

    // Initialize WAL receiver if this is a follower with a leader URL configured
    #[cfg(feature = "replication")]
    let wal_receiver = if role == NodeRole::Follower {
        if let Ok(leader_url) = std::env::var("ALLSOURCE_LEADER_URL")
            && !leader_url.is_empty()
        {
            let follower_wal_dir = std::env::var("ALLSOURCE_DATA_DIR")
                .unwrap_or_else(|_| "/tmp/allsource".to_string());
            let wal_dir = std::path::PathBuf::from(&follower_wal_dir).join("follower-wal");

            match WalReceiver::new(leader_url.clone(), &wal_dir, Arc::clone(&store)) {
                Ok(mut receiver) => {
                    receiver.set_metrics(store.metrics());
                    let receiver = Arc::new(receiver);
                    let receiver_clone = Arc::clone(&receiver);
                    tokio::spawn(async move {
                        receiver_clone.run().await;
                    });
                    tracing::info!(
                        "✅ WAL receiver started, connecting to leader at {}",
                        leader_url,
                    );
                    Some(receiver)
                }
                Err(e) => {
                    tracing::error!("Failed to initialize WAL receiver: {}", e);
                    None
                }
            }
        } else {
            tracing::warn!("Follower mode but ALLSOURCE_LEADER_URL not set — replication disabled");
            None
        }
    } else {
        None
    };
    #[cfg(not(feature = "replication"))]
    let wal_receiver: Option<Arc<allsource_core::replication::WalReceiver>> = None;

    let auth_manager = Arc::new({
        let mut mgr = match std::env::var("ALLSOURCE_JWT_SECRET") {
            Ok(secret) if !secret.is_empty() => {
                tracing::info!("Using ALLSOURCE_JWT_SECRET for JWT validation");
                AuthManager::new(&secret)
            }
            _ => {
                tracing::warn!(
                    "ALLSOURCE_JWT_SECRET not set — using random secret (tokens from other services will fail)"
                );
                AuthManager::default()
            }
        };

        // Attach event-sourced auth repository for durable API key storage
        if let Some(ref repos) = system_repos {
            mgr = mgr.with_auth_repository(repos.auth_repository.clone());
        }

        mgr
    });
    let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig::professional()));

    // Use event-sourced tenant repository when system repositories are available,
    // otherwise fall back to in-memory only (dev/test mode).
    let tenant_repo: Arc<dyn TenantRepository> = if let Some(ref repos) = system_repos {
        repos.tenant_repository.clone()
    } else {
        Arc::new(InMemoryTenantRepository::new())
    };

    // Initialize DI container for paywall domain + system repositories
    let mut builder = ContainerBuilder::new().with_in_memory_repositories();
    if let Some(repos) = system_repos {
        builder = builder.with_system_repositories(repos);
    }
    let service_container = builder.build();

    // Start webhook delivery worker (v0.11 feature)
    {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        store.set_webhook_tx(tx);
        let webhook_registry = store.webhook_registry();
        tokio::spawn(allsource_core::webhook_worker::run_webhook_delivery_worker(
            rx,
            webhook_registry,
        ));
    }

    tracing::info!("✅ Event store initialized");
    tracing::info!("✅ Authentication manager initialized");
    tracing::info!("✅ Tenant repository initialized");
    tracing::info!("✅ Rate limiter initialized (professional tier defaults)");
    if service_container.has_system_repositories() {
        tracing::info!("✅ Service container initialized (event-sourced system repositories)");
    } else {
        tracing::info!("✅ Service container initialized (in-memory repositories)");
    }

    // Register bootstrap API key if configured
    if let Ok(bootstrap_key) = std::env::var("ALLSOURCE_BOOTSTRAP_API_KEY")
        && !bootstrap_key.is_empty()
    {
        let bootstrap_tenant = std::env::var("ALLSOURCE_BOOTSTRAP_TENANT_ID")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "default".to_string());
        auth_manager.register_bootstrap_api_key(&bootstrap_key, &bootstrap_tenant);
        tracing::info!(
            "✅ Bootstrap API key configured (tenant: {})",
            bootstrap_tenant
        );
    }

    // Start RESP3 (Redis wire protocol) server if configured
    if let Ok(resp_port_str) = std::env::var("ALLSOURCE_RESP_PORT")
        && let Ok(resp_port) = resp_port_str.parse::<u16>()
    {
        let resp_server = Arc::new(RespServer::new(Arc::clone(&store)));
        tokio::spawn(async move {
            if let Err(e) = resp_server.serve(resp_port).await {
                tracing::error!("RESP3 server error: {}", e);
            }
        });
        tracing::info!("✅ RESP3 server enabled on port {}", resp_port);
    }

    // Initialize cluster manager if cluster mode is enabled
    let cluster_manager = if std::env::var("ALLSOURCE_CLUSTER_ENABLED").is_ok_and(|v| v == "true") {
        let self_node_id: u32 = std::env::var("ALLSOURCE_NODE_ID")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let partition_count: u32 = std::env::var("ALLSOURCE_PARTITION_COUNT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(32);

        let cm = ClusterManager::new(self_node_id, partition_count);

        // Self-register this node
        let api_port: u16 = std::env::var("ALLSOURCE_PORT")
            .or_else(|_| std::env::var("PORT"))
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3900);
        let host = std::env::var("ALLSOURCE_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());

        let member_role = match role {
            NodeRole::Leader => MemberRole::Leader,
            NodeRole::Follower => MemberRole::Follower,
        };

        let self_member = ClusterMember {
            node_id: self_node_id,
            api_address: format!("{host}:{api_port}"),
            replication_address: format!("{host}:{replication_port}"),
            role: member_role,
            last_wal_offset: 0,
            last_heartbeat_ms: 0,
            healthy: true,
        };

        // Use block_on to run the async add_member from sync context
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(cm.add_member(self_member));
        });

        tracing::info!(
            "✅ Cluster mode enabled (node_id={}, partitions={})",
            self_node_id,
            partition_count,
        );
        Some(Arc::new(cm))
    } else {
        None
    };

    // Initialize geo-replication if configured
    let geo_replication = GeoReplicationConfig::from_env().map(|config| {
        let peer_count = config.peers.len();
        let region_id = config.region_id.clone();
        let mgr = Arc::new(GeoReplicationManager::new(config));
        tracing::info!(
            "✅ Geo-replication enabled (region='{}', peers={})",
            region_id,
            peer_count,
        );
        mgr
    });

    // Start API server (v1.0 with auth & rate limiting)
    // Use Config::from_env() to pick up ALLSOURCE_HOST / ALLSOURCE_PORT / PORT env vars
    // (Critical on Fly.io where ALLSOURCE_HOST="::" enables IPv6 6PN internal networking)
    let config = ServerConfig::from_env();
    let addr = if config.host.contains(':') {
        format!("[{}]:{}", config.host, config.port)
    } else {
        format!("{}:{}", config.host, config.port)
    };
    tracing::info!("🚀 AllSource Core listening on {}", addr);
    tracing::info!("📝 API: /health, /api/v1/events, /api/v1/events/query");
    tracing::info!(
        "🔌 WebSocket: ws://{}:{}/api/v1/events/stream",
        config.host,
        config.port
    );
    tracing::info!("🔒 Features: Auth, Multi-tenancy, Rate Limiting");

    api_v1::serve_v1(
        store,
        auth_manager,
        tenant_repo,
        rate_limiter,
        service_container,
        &addr,
        role,
        wal_shipper,
        wal_receiver,
        replication_port,
        cluster_manager,
        geo_replication,
    )
    .await?;

    Ok(())
}
