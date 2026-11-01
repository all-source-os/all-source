#[cfg(feature = "server")]
use crate::infrastructure::repositories::EventSourcedAuthRepository;
use crate::{
    application::services::consumer::ConsumerRegistry,
    domain::{entities::TenantQuotas, repositories::TenantRepository, value_objects::TenantId},
    error::Result,
    infrastructure::{
        persistence::SystemMetadataStore,
        repositories::{
            EventSourcedAuditRepository, EventSourcedConfigRepository, EventSourcedTenantRepository,
        },
    },
};
use std::{path::PathBuf, sync::Arc};

/// Holds all event-sourced repositories for system metadata.
///
/// This is the result of a successful system bootstrap. All repositories
/// share a single `SystemMetadataStore` for storage, ensuring atomic
/// write-ahead logging and coordinated recovery.
pub struct SystemRepositories {
    /// The underlying durable store
    pub system_store: Arc<SystemMetadataStore>,

    /// Event-sourced tenant repository
    pub tenant_repository: Arc<EventSourcedTenantRepository>,

    /// Event-sourced audit repository
    pub audit_repository: Arc<EventSourcedAuditRepository>,

    /// Event-sourced config repository
    pub config_repository: Arc<EventSourcedConfigRepository>,

    /// Event-sourced auth repository (API keys)
    #[cfg(feature = "server")]
    pub auth_repository: Arc<EventSourcedAuthRepository>,

    /// Durable consumer registry for subscription cursor tracking
    pub consumer_registry: Arc<ConsumerRegistry>,
}

/// Staged system initialization with static stability.
///
/// Follows the AWS data plane / control plane separation pattern:
///
/// ```text
/// Stage 1: Initialize SystemMetadataStore from local storage
/// Stage 2: Replay _system/* streams → populate in-memory caches
/// Stage 3: If empty, run first-boot bootstrap (create default tenant)
/// Stage 4: Return SystemRepositories → start accepting traffic
/// ```
///
/// # Static Stability
///
/// After bootstrap, all metadata reads go through in-memory caches (DashMap).
/// If a system stream write fails, the error is logged but does NOT block
/// the data plane. User event ingestion and queries continue with cached
/// metadata until the system store recovers.
pub struct SystemBootstrap;

impl SystemBootstrap {
    /// Execute the staged initialization sequence.
    ///
    /// # Arguments
    /// * `system_data_dir` — Path to the system metadata storage directory
    /// * `bootstrap_tenant` — Optional default tenant name for first-boot
    ///
    /// # Returns
    /// `SystemRepositories` with all event-sourced repos initialized and caches populated.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub async fn initialize(
        system_data_dir: PathBuf,
        bootstrap_tenant: Option<String>,
    ) -> Result<SystemRepositories> {
        // Stage 1: Initialize SystemMetadataStore
        tracing::info!(
            "Stage 1: Initializing system metadata store at {}",
            system_data_dir.display()
        );
        let system_store = Arc::new(SystemMetadataStore::new(&system_data_dir)?);
        tracing::info!(
            "System store initialized: {} events recovered",
            system_store.total_events()
        );

        // Stage 2: Replay system streams → populate in-memory caches
        tracing::info!("Stage 2: Replaying system streams into repository caches");
        let timed = |name: &str, started: std::time::Instant| {
            tracing::info!("Stage 2: {name} cache rebuilt in {:?}", started.elapsed());
        };
        let started = std::time::Instant::now();
        let tenant_repository = Arc::new(EventSourcedTenantRepository::new(system_store.clone()));
        timed("tenant", started);
        let started = std::time::Instant::now();
        let audit_repository = Arc::new(EventSourcedAuditRepository::new(system_store.clone()));
        timed("audit", started);
        let started = std::time::Instant::now();
        let config_repository = Arc::new(EventSourcedConfigRepository::new(system_store.clone()));
        timed("config", started);
        #[cfg(feature = "server")]
        let auth_repository = {
            let started = std::time::Instant::now();
            let repository = Arc::new(EventSourcedAuthRepository::new(system_store.clone()));
            timed("auth", started);
            repository
        };
        let started = std::time::Instant::now();
        let consumer_registry = Arc::new(ConsumerRegistry::new_durable(system_store.clone()));
        timed("consumer", started);

        let tenant_count = tenant_repository.count().await.unwrap_or(0);
        #[cfg(feature = "server")]
        tracing::info!(
            "System caches populated: {} tenants, {} config entries, {} API keys ({} active), {} consumers",
            tenant_count,
            config_repository.count(),
            auth_repository.count(),
            auth_repository.active_count(),
            consumer_registry.count()
        );
        #[cfg(not(feature = "server"))]
        tracing::info!(
            "System caches populated: {} tenants, {} config entries, {} consumers",
            tenant_count,
            config_repository.count(),
            consumer_registry.count()
        );

        // Stage 3: First-boot bootstrap (if needed)
        if tenant_count == 0 {
            if let Some(ref tenant_name) = bootstrap_tenant {
                tracing::info!(
                    "Stage 3: First boot detected — creating default tenant '{}'",
                    tenant_name
                );

                // Use the tenant name as the tenant ID (lowercase, hyphens)
                let tenant_id_str = tenant_name
                    .to_lowercase()
                    .replace(' ', "-")
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                    .collect::<String>();

                match TenantId::new(tenant_id_str) {
                    Ok(tenant_id) => {
                        match tenant_repository
                            .create(tenant_id, tenant_name.clone(), TenantQuotas::unlimited())
                            .await
                        {
                            Ok(_) => {
                                tracing::info!("Default tenant '{}' created", tenant_name);
                            }
                            Err(e) => {
                                tracing::warn!("Failed to create default tenant: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Invalid default tenant ID: {}", e);
                    }
                }
            } else {
                tracing::info!(
                    "Stage 3: No bootstrap tenant configured, skipping first-boot setup"
                );
            }
        } else {
            tracing::info!("Stage 3: Skipped — {} existing tenants found", tenant_count);
        }

        // Stage 4: Ready
        tracing::info!("Stage 4: System metadata initialized — ready to accept traffic");

        Ok(SystemRepositories {
            system_store,
            tenant_repository,
            audit_repository,
            config_repository,
            #[cfg(feature = "server")]
            auth_repository,
            consumer_registry,
        })
    }

    /// Try to initialize system repositories, falling back to None if the
    /// system data directory is not configured.
    ///
    /// This implements the backward-compatibility path: if no system_data_dir
    /// is set, the system continues with in-memory repositories.
    pub async fn try_initialize(
        system_data_dir: Option<PathBuf>,
        bootstrap_tenant: Option<String>,
    ) -> Option<SystemRepositories> {
        if let Some(dir) = system_data_dir {
            match Self::initialize(dir, bootstrap_tenant).await {
                Ok(repos) => Some(repos),
                Err(e) => {
                    tracing::error!(
                        "Failed to initialize system metadata store: {}. \
                     Falling back to in-memory repositories.",
                        e
                    );
                    None
                }
            }
        } else {
            tracing::info!(
                "No system_data_dir configured. Using in-memory repositories for metadata."
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_staged_initialization_empty() {
        let temp_dir = TempDir::new().unwrap();
        let system_dir = temp_dir.path().join("__system");

        let repos = SystemBootstrap::initialize(system_dir, None).await.unwrap();

        assert_eq!(repos.tenant_repository.count().await.unwrap(), 0);
        assert_eq!(repos.config_repository.count(), 0);
    }

    #[tokio::test]
    async fn test_staged_initialization_with_bootstrap_tenant() {
        let temp_dir = TempDir::new().unwrap();
        let system_dir = temp_dir.path().join("__system");

        let repos = SystemBootstrap::initialize(system_dir, Some("Default Tenant".to_string()))
            .await
            .unwrap();

        assert_eq!(repos.tenant_repository.count().await.unwrap(), 1);

        let tenant_id = TenantId::new("default-tenant".to_string()).unwrap();
        let tenant = repos
            .tenant_repository
            .find_by_id(&tenant_id)
            .await
            .unwrap();
        assert!(tenant.is_some());
        assert_eq!(tenant.unwrap().name(), "Default Tenant");
    }

    #[tokio::test]
    async fn test_staged_initialization_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let system_dir = temp_dir.path().join("__system");

        // First boot
        {
            let repos =
                SystemBootstrap::initialize(system_dir.clone(), Some("ACME Corp".to_string()))
                    .await
                    .unwrap();

            assert_eq!(repos.tenant_repository.count().await.unwrap(), 1);

            // Add some config
            repos
                .config_repository
                .set("feature_flag", serde_json::json!(true), None)
                .unwrap();
        }

        // Restart — should recover everything
        {
            let repos = SystemBootstrap::initialize(system_dir, None).await.unwrap();

            // Tenant should survive
            assert_eq!(repos.tenant_repository.count().await.unwrap(), 1);

            // Config should survive
            assert_eq!(repos.config_repository.count(), 1);
            assert_eq!(
                repos.config_repository.get_value("feature_flag").unwrap(),
                serde_json::json!(true)
            );
        }
    }

    #[tokio::test]
    async fn test_try_initialize_none() {
        let result = SystemBootstrap::try_initialize(None, None).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_try_initialize_some() {
        let temp_dir = TempDir::new().unwrap();
        let system_dir = temp_dir.path().join("__system");

        let result = SystemBootstrap::try_initialize(Some(system_dir), None).await;
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_no_duplicate_bootstrap_on_restart() {
        let temp_dir = TempDir::new().unwrap();
        let system_dir = temp_dir.path().join("__system");

        // First boot with bootstrap
        SystemBootstrap::initialize(system_dir.clone(), Some("ACME".to_string()))
            .await
            .unwrap();

        // Second boot with same bootstrap config — should NOT create duplicate
        let repos = SystemBootstrap::initialize(system_dir, Some("ACME".to_string()))
            .await
            .unwrap();

        assert_eq!(repos.tenant_repository.count().await.unwrap(), 1);
    }
}
