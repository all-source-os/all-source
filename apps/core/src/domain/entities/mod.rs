// Core event sourcing entities
pub mod audit_event;
pub mod event;
pub mod event_store_fork;
pub mod event_stream;
pub mod projection;
pub mod query_usage;
pub mod schema;
pub mod tenant;

// Paywall domain entities
pub mod access_token;
pub mod creator;
pub mod paywall_article;
pub mod transaction;

// Core re-exports
pub use audit_event::{Actor, AuditAction, AuditCategory, AuditEvent, AuditEventId, AuditOutcome};
pub use event::Event;
pub use event_store_fork::{EventStoreFork, ForkIsolationLevel, ForkStatus};
pub use event_stream::EventStream;
pub use projection::{
    Projection, ProjectionConfig, ProjectionStats, ProjectionStatus, ProjectionType,
};
pub use schema::{CompatibilityMode, Schema};
pub use tenant::{QuotaResource, SchemaEnforcement, Tenant, TenantQuotas, TenantUsage, UsageMeter};

// Paywall re-exports
pub use access_token::{AccessMethod, AccessToken, AccessTokenId};
pub use creator::{Creator, CreatorSettings, CreatorStatus, CreatorTier};
pub use paywall_article::{ArticleStats, ArticleStatus, PaywallArticle};
pub use transaction::{Blockchain, Transaction, TransactionStatus};
