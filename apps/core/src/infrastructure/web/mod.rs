// Web layer - HTTP handlers, WebSocket, and API routes
// Clean Architecture: Infrastructure Layer

pub mod api;
pub mod api_v1;
mod archive_work;
pub mod audit_api;
pub mod auth_api;
pub mod config_api;
pub mod demo_api;
pub mod handlers;
#[cfg(feature = "prime")]
pub mod prime_api;
mod retained_query;
#[cfg(feature = "multi-tenant")]
pub mod tenant_api;
#[cfg(feature = "multi-tenant")]
pub mod tenant_query_usage_api;
pub mod websocket;

// Public API re-exports
pub use api::serve;
pub use websocket::{EventFilters, WebSocketConfig, WebSocketManager, WebSocketStats};

// Handler state re-exports for convenience
pub use handlers::{
    ArticleHandlerState, CreatorHandlerState, ForkHandlerState, PaymentHandlerState,
};
