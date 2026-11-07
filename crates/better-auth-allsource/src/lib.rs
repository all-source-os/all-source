//! Allsource `DatabaseAdapter` for [better-auth-rs](https://crates.io/crates/better-auth-core).
//!
//! Uses Allsource Core for event appending and Query Service for reads,
//! eliminating the need for a separate SQL database for authentication.
//!
//! # Usage
//!
//! ```rust,ignore
//! use better_auth_allsource::AllsourceAuthAdapter;
//!
//! let adapter = AllsourceAuthAdapter::new(
//!     "http://localhost:3900",   // Allsource Core
//!     "http://localhost:3902",   // Allsource Query Service
//!     "my-tenant-id",
//!     "ask_my-api-key",
//! );
//!
//! let auth = BetterAuth::builder()
//!     .database(adapter)
//!     .build();
//! ```

mod adapter;
mod client;
mod credentials;
mod error;
mod user_record;

pub use adapter::AllsourceAuthAdapter;
pub use credentials::CredentialProvider;
pub use error::AllsourceAuthError;
