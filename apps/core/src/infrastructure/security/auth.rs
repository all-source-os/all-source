use crate::{
    error::{AllSourceError, Result},
    infrastructure::hash_state::{FastDashMap, fast_dashmap},
};
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// User role for RBAC
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,          // Full system access
    Developer,      // Read/write events, manage schemas
    ReadOnly,       // Read-only access to events
    ServiceAccount, // Programmatic access for services
}

impl Role {
    /// Check if role has specific permission
    pub fn has_permission(&self, permission: Permission) -> bool {
        match self {
            Role::Admin => true, // Admin has all permissions
            Role::Developer => matches!(
                permission,
                Permission::Read
                    | Permission::Write
                    | Permission::Metrics
                    | Permission::ManageSchemas
                    | Permission::ManagePipelines
            ),
            Role::ReadOnly => matches!(permission, Permission::Read | Permission::Metrics),
            Role::ServiceAccount => {
                matches!(permission, Permission::Read | Permission::Write)
            }
        }
    }

    /// Canonical role preset for MCP tokens issued to Pro-tier subscribers.
    /// Pro unlocks MCP server access at a read-only scope — consumers can
    /// query events, sample streams, and read metrics, but cannot mutate
    /// state. Call this from the API-key provisioning path when a Pro
    /// tenant requests an MCP key so the mapping stays centralized here
    /// rather than hardcoded at call sites.
    pub fn mcp_readonly_preset() -> Self {
        Role::ReadOnly
    }
}

/// Permission types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Read,
    Write,
    Admin,
    Metrics,
    ManageSchemas,
    ManagePipelines,
    ManageTenants,
}

/// JWT Claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID or API key ID)
    pub sub: String,
    /// Tenant ID
    pub tenant_id: String,
    /// User role
    pub role: Role,
    /// Expiration time (UNIX timestamp)
    pub exp: i64,
    /// Issued at time (UNIX timestamp)
    pub iat: i64,
    /// Issuer
    pub iss: String,
}

impl Claims {
    /// Create new claims for a user
    pub fn new(user_id: String, tenant_id: String, role: Role, expires_in: Duration) -> Self {
        let now = Utc::now();
        Self {
            sub: user_id,
            tenant_id,
            role,
            iat: now.timestamp(),
            exp: (now + expires_in).timestamp(),
            iss: "allsource".to_string(),
        }
    }

    /// Check if claims are expired
    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() > self.exp
    }

    /// Check if user has permission
    pub fn has_permission(&self, permission: Permission) -> bool {
        self.role.has_permission(permission)
    }
}

/// User account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub role: Role,
    pub tenant_id: String,
    pub created_at: chrono::DateTime<Utc>,
    pub active: bool,
}

impl User {
    /// Create a new user with hashed password
    pub fn new(
        username: String,
        email: String,
        password: &str,
        role: Role,
        tenant_id: String,
    ) -> Result<Self> {
        let password_hash = hash_password(password)?;

        Ok(Self {
            id: Uuid::new_v4(),
            username,
            email,
            password_hash,
            role,
            tenant_id,
            created_at: Utc::now(),
            active: true,
        })
    }

    /// Verify password
    pub fn verify_password(&self, password: &str) -> Result<bool> {
        verify_password(password, &self.password_hash)
    }
}

/// API Key for programmatic access
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: Uuid,
    pub name: String,
    pub tenant_id: String,
    pub role: Role,
    #[serde(skip_serializing)]
    pub key_hash: String,
    pub created_at: chrono::DateTime<Utc>,
    pub expires_at: Option<chrono::DateTime<Utc>>,
    pub active: bool,
    pub last_used: Option<chrono::DateTime<Utc>>,
}

impl ApiKey {
    /// Create new API key
    pub fn new(
        name: String,
        tenant_id: String,
        role: Role,
        expires_at: Option<chrono::DateTime<Utc>>,
    ) -> (Self, String) {
        let key = generate_api_key();
        let key_hash = hash_api_key(&key);

        let api_key = Self {
            id: Uuid::new_v4(),
            name,
            tenant_id,
            role,
            key_hash,
            created_at: Utc::now(),
            expires_at,
            active: true,
            last_used: None,
        };

        (api_key, key)
    }

    /// Verify API key
    pub fn verify(&self, key: &str) -> bool {
        if !self.active {
            return false;
        }

        if let Some(expires_at) = self.expires_at
            && Utc::now() > expires_at
        {
            return false;
        }

        hash_api_key(key) == self.key_hash
    }

    /// Check if expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }
}

/// Authentication manager
pub struct AuthManager {
    /// JWT encoding key
    encoding_key: EncodingKey,
    /// JWT decoding key
    decoding_key: DecodingKey,
    /// JWT validation rules
    validation: Validation,
    /// Users storage (in production, use database)
    users: Arc<FastDashMap<Uuid, User>>,
    /// API keys storage (in production, use database)
    api_keys: Arc<FastDashMap<Uuid, ApiKey>>,
    /// Username to user ID mapping
    username_index: Arc<FastDashMap<String, Uuid>>,
    /// Optional event-sourced auth repository for durable key storage.
    /// When set, API key operations are persisted to the system WAL.
    auth_repo: Option<Arc<crate::infrastructure::repositories::EventSourcedAuthRepository>>,
}

impl AuthManager {
    /// Create new authentication manager
    pub fn new(jwt_secret: &str) -> Self {
        let encoding_key = EncodingKey::from_secret(jwt_secret.as_bytes());
        let decoding_key = DecodingKey::from_secret(jwt_secret.as_bytes());

        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&["allsource"]);

        Self {
            encoding_key,
            decoding_key,
            validation,
            users: Arc::new(fast_dashmap()),
            api_keys: Arc::new(fast_dashmap()),
            username_index: Arc::new(fast_dashmap()),
            auth_repo: None,
        }
    }

    /// Attach an event-sourced auth repository for durable API key storage.
    ///
    /// Loads all previously persisted keys into the in-memory cache.
    /// Subsequent `create_api_key` and `revoke_api_key` calls will also
    /// write events to the system WAL.
    pub fn with_auth_repository(
        mut self,
        repo: Arc<crate::infrastructure::repositories::EventSourcedAuthRepository>,
    ) -> Self {
        // Load persisted keys into the in-memory DashMap
        for key in repo.all_keys() {
            self.api_keys.insert(key.id, key);
        }
        let loaded = self.api_keys.len();
        if loaded > 0 {
            tracing::info!("Loaded {} API keys from durable storage", loaded);
        }
        self.auth_repo = Some(repo);
        self
    }

    /// Register a new user
    pub fn register_user(
        &self,
        username: String,
        email: String,
        password: &str,
        role: Role,
        tenant_id: String,
    ) -> Result<User> {
        // Check if username already exists
        if self.username_index.contains_key(&username) {
            return Err(AllSourceError::ValidationError(
                "Username already exists".to_string(),
            ));
        }

        let user = User::new(username.clone(), email, password, role, tenant_id)?;

        self.users.insert(user.id, user.clone());
        self.username_index.insert(username, user.id);

        Ok(user)
    }

    /// Authenticate user with username and password
    pub fn authenticate(&self, username: &str, password: &str) -> Result<String> {
        let user_id = self
            .username_index
            .get(username)
            .ok_or_else(|| AllSourceError::ValidationError("Invalid credentials".to_string()))?;

        let user = self
            .users
            .get(&user_id)
            .ok_or_else(|| AllSourceError::ValidationError("User not found".to_string()))?;

        if !user.active {
            return Err(AllSourceError::ValidationError(
                "User account is inactive".to_string(),
            ));
        }

        if !user.verify_password(password)? {
            return Err(AllSourceError::ValidationError(
                "Invalid credentials".to_string(),
            ));
        }

        // Generate JWT token
        let claims = Claims::new(
            user.id.to_string(),
            user.tenant_id.clone(),
            user.role.clone(),
            Duration::hours(24), // Token expires in 24 hours
        );

        let token = encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| AllSourceError::ValidationError(format!("Failed to create token: {e}")))?;

        Ok(token)
    }

    /// Validate JWT token
    pub fn validate_token(&self, token: &str) -> Result<Claims> {
        let token_data = decode::<Claims>(token, &self.decoding_key, &self.validation)
            .map_err(|e| AllSourceError::ValidationError(format!("Invalid token: {e}")))?;

        if token_data.claims.is_expired() {
            return Err(AllSourceError::ValidationError("Token expired".to_string()));
        }

        Ok(token_data.claims)
    }

    /// Create API key
    ///
    /// When an auth repository is attached, the key is persisted to the
    /// system WAL and survives restarts.
    pub fn create_api_key(
        &self,
        name: String,
        tenant_id: String,
        role: Role,
        expires_at: Option<chrono::DateTime<Utc>>,
    ) -> (ApiKey, String) {
        let (api_key, key) = ApiKey::new(name, tenant_id, role, expires_at);
        self.api_keys.insert(api_key.id, api_key.clone());

        // Persist to durable storage if available
        if let Some(ref repo) = self.auth_repo
            && let Err(e) = repo.persist_api_key(&api_key)
        {
            tracing::error!("Failed to persist API key to system store: {e}");
        }

        (api_key, key)
    }

    /// Validate API key
    pub fn validate_api_key(&self, key: &str) -> Result<Claims> {
        // First, find the matching key and extract the data we need
        // We must complete iteration before calling get_mut to avoid deadlock
        let found_key = self.api_keys.iter().find_map(|entry| {
            let api_key = entry.value();
            if api_key.verify(key) {
                Some((api_key.id, api_key.tenant_id.clone(), api_key.role.clone()))
            } else {
                None
            }
        });

        if let Some((key_id, tenant_id, role)) = found_key {
            // Now safe to acquire write lock since iteration is complete
            if let Some(mut key_mut) = self.api_keys.get_mut(&key_id) {
                key_mut.last_used = Some(Utc::now());
            }

            let claims = Claims::new(key_id.to_string(), tenant_id, role, Duration::hours(24));

            return Ok(claims);
        }

        Err(AllSourceError::ValidationError(
            "Invalid API key".to_string(),
        ))
    }

    /// Get user by ID
    pub fn get_user(&self, user_id: &Uuid) -> Option<User> {
        self.users.get(user_id).map(|u| u.clone())
    }

    /// List all users (admin only)
    pub fn list_users(&self) -> Vec<User> {
        self.users
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Delete user
    pub fn delete_user(&self, user_id: &Uuid) -> Result<()> {
        if let Some((_, user)) = self.users.remove(user_id) {
            self.username_index.remove(&user.username);
            Ok(())
        } else {
            Err(AllSourceError::ValidationError(
                "User not found".to_string(),
            ))
        }
    }

    /// Revoke API key
    ///
    /// When an auth repository is attached, the revocation is persisted
    /// to the system WAL and survives restarts.
    pub fn revoke_api_key(&self, key_id: &Uuid) -> Result<()> {
        if let Some(mut api_key) = self.api_keys.get_mut(key_id) {
            api_key.active = false;

            // Persist revocation to durable storage
            if let Some(ref repo) = self.auth_repo
                && let Err(e) = repo.persist_key_revocation(key_id)
            {
                tracing::error!("Failed to persist key revocation to system store: {e}");
            }

            Ok(())
        } else {
            Err(AllSourceError::ValidationError(
                "API key not found".to_string(),
            ))
        }
    }

    /// List API keys for a tenant
    pub fn list_api_keys(&self, tenant_id: &str) -> Vec<ApiKey> {
        self.api_keys
            .iter()
            .filter(|entry| entry.value().tenant_id == tenant_id)
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Register a bootstrap API key with a specific key value.
    ///
    /// This is used on startup to create a pre-configured API key from
    /// the `ALLSOURCE_BOOTSTRAP_API_KEY` environment variable.
    ///
    /// If an auth repository is attached, the key is persisted to the
    /// system WAL. On subsequent restarts, the key is loaded from storage
    /// automatically — the bootstrap only creates the key if it doesn't
    /// already exist (idempotent).
    pub fn register_bootstrap_api_key(&self, key: &str, tenant_id: &str) -> ApiKey {
        let key_hash = hash_api_key(key);

        // Check if this exact key already exists (idempotent bootstrap)
        let existing = self.api_keys.iter().find_map(|entry| {
            if entry.value().key_hash == key_hash && entry.value().active {
                Some(entry.value().clone())
            } else {
                None
            }
        });

        if let Some(existing_key) = existing {
            tracing::debug!("Bootstrap API key already exists (idempotent)");
            return existing_key;
        }

        let api_key = ApiKey {
            id: Uuid::new_v4(),
            name: "bootstrap".to_string(),
            tenant_id: tenant_id.to_string(),
            role: Role::Admin,
            key_hash,
            created_at: Utc::now(),
            expires_at: None,
            active: true,
            last_used: None,
        };

        self.api_keys.insert(api_key.id, api_key.clone());

        // Persist to durable storage if available
        if let Some(ref repo) = self.auth_repo
            && let Err(e) = repo.persist_api_key(&api_key)
        {
            tracing::error!("Failed to persist bootstrap API key: {e}");
        }

        api_key
    }
}

impl Default for AuthManager {
    fn default() -> Self {
        use base64::{Engine as _, engine::general_purpose};
        let secret = general_purpose::STANDARD.encode(rand::random::<[u8; 32]>());
        Self::new(&secret)
    }
}

/// Hash password using Argon2
fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| AllSourceError::ValidationError(format!("Password hashing failed: {e}")))?;

    Ok(hash.to_string())
}

/// Verify password against hash
fn verify_password(password: &str, hash: &str) -> Result<bool> {
    let parsed_hash = PasswordHash::new(hash)
        .map_err(|e| AllSourceError::ValidationError(format!("Invalid password hash: {e}")))?;

    let argon2 = Argon2::default();
    Ok(argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

/// Generate API key
fn generate_api_key() -> String {
    use base64::{Engine as _, engine::general_purpose};
    let random_bytes: [u8; 32] = rand::random();
    format!(
        "ask_{}",
        general_purpose::URL_SAFE_NO_PAD.encode(random_bytes)
    )
}

/// Hash API key for storage
fn hash_api_key(key: &str) -> String {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };

    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_creation_and_verification() {
        let user = User::new(
            "testuser".to_string(),
            "test@example.com".to_string(),
            "password123",
            Role::Developer,
            "tenant1".to_string(),
        )
        .unwrap();

        assert!(user.verify_password("password123").unwrap());
        assert!(!user.verify_password("wrongpassword").unwrap());
    }

    #[test]
    fn test_role_permissions() {
        assert!(Role::Admin.has_permission(Permission::Admin));
        assert!(Role::Developer.has_permission(Permission::Write));
        assert!(!Role::ReadOnly.has_permission(Permission::Write));
        assert!(Role::ReadOnly.has_permission(Permission::Read));
    }

    #[test]
    fn test_auth_manager() {
        let auth = AuthManager::new("test_secret");

        // Register user
        let user = auth
            .register_user(
                "alice".to_string(),
                "alice@example.com".to_string(),
                "password123",
                Role::Developer,
                "tenant1".to_string(),
            )
            .unwrap();

        // Authenticate
        let token = auth.authenticate("alice", "password123").unwrap();
        assert!(!token.is_empty());

        // Validate token
        let claims = auth.validate_token(&token).unwrap();
        assert_eq!(claims.tenant_id, "tenant1");
        assert_eq!(claims.role, Role::Developer);
    }

    #[test]
    fn test_api_key() {
        let auth = AuthManager::new("test_secret");

        let (api_key, key) = auth.create_api_key(
            "test-key".to_string(),
            "tenant1".to_string(),
            Role::ServiceAccount,
            None,
        );

        // Validate key
        let claims = auth.validate_api_key(&key).unwrap();
        assert_eq!(claims.tenant_id, "tenant1");
        assert_eq!(claims.role, Role::ServiceAccount);

        // Revoke key
        auth.revoke_api_key(&api_key.id).unwrap();

        // Should fail after revocation
        assert!(auth.validate_api_key(&key).is_err());
    }

    #[test]
    fn test_claims_expiration() {
        let claims = Claims::new(
            "user1".to_string(),
            "tenant1".to_string(),
            Role::Developer,
            Duration::seconds(-1), // Expired 1 second ago
        );

        assert!(claims.is_expired());
    }
}
