# Security Documentation

## Overview

AllSource Core implements a comprehensive, production-ready security architecture with multiple layers of protection designed for multi-tenant event sourcing systems. This document describes the security features, best practices, and configuration options.

## Table of Contents

1. [Security Architecture](#security-architecture)
2. [Authentication](#authentication)
3. [Authorization (RBAC)](#authorization-rbac)
4. [Tenant Isolation](#tenant-isolation)
5. [Rate Limiting](#rate-limiting)
6. [Audit Logging](#audit-logging)
7. [Network Security](#network-security)
8. [Security Headers](#security-headers)
9. [Security Best Practices](#security-best-practices)
10. [Vulnerability Reporting](#vulnerability-reporting)

---

## Security Architecture

AllSource Core uses a defense-in-depth approach with multiple security layers:

```
┌─────────────────────────────────────────────┐
│         Network Layer                        │
│  - IP Filtering (Global + Per-Tenant)       │
│  - TLS Termination                          │
└─────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────┐
│         Middleware Stack                     │
│  - Request ID Generation                    │
│  - Security Headers (HSTS, CSP, etc.)      │
│  - Rate Limiting                            │
└─────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────┐
│         Authentication Layer                 │
│  - JWT Token Validation                     │
│  - API Key Verification                     │
│  - Password Authentication                  │
└─────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────┐
│         Authorization Layer (RBAC)           │
│  - Permission Checks                        │
│  - Role-Based Access Control                │
└─────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────┐
│         Tenant Isolation Layer               │
│  - Tenant Context Validation                │
│  - Cross-Tenant Access Prevention           │
└─────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────┐
│         Data Access Layer                    │
│  - Repository-Level Isolation               │
│  - Query Filtering by Tenant                │
└─────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────┐
│         Audit Layer                          │
│  - All Security Events Logged               │
│  - Immutable Audit Trail                    │
└─────────────────────────────────────────────┘
```

---

## Authentication

AllSource Core supports three authentication methods:

### 1. JWT Token Authentication

**Use Case**: Web applications, SPAs, mobile apps

**How it works**:
```rust
use allsource_core::auth::{AuthManager, Role};
use chrono::Duration;

// Create auth manager
let auth = AuthManager::new("your-secret-key");

// Register user
let user = auth.register_user(
    "username".to_string(),
    "user@example.com".to_string(),
    "SecurePassword123!",
    Role::Developer,
    "tenant-id".to_string(),
)?;

// Authenticate and get JWT token
let token = auth.authenticate("username", "SecurePassword123!")?;

// Validate token
let claims = auth.validate_token(&token)?;
```

**Security Features**:
- HS256 algorithm for token signing
- Configurable token expiration (default: 24 hours)
- Token includes: user_id, tenant_id, role, expiration
- Automatic expiration checking
- Secret key must be at least 32 characters in production

**Best Practices**:
```rust
// ✅ DO: Use strong, randomly generated secrets
let secret = generate_random_string(64);
let auth = AuthManager::new(&secret);

// ✅ DO: Store tokens securely (httpOnly cookies)
// ✅ DO: Use short expiration times for sensitive operations
// ✅ DO: Implement token refresh mechanism

// ❌ DON'T: Use weak or predictable secrets
// ❌ DON'T: Store tokens in localStorage (XSS risk)
// ❌ DON'T: Use overly long expiration times
```

### 2. API Key Authentication

**Use Case**: Service-to-service communication, CLI tools, scripts

**How it works**:
```rust
// Create API key
let (api_key, raw_key) = auth.create_api_key(
    "Production API Key".to_string(),
    "tenant-id".to_string(),
    Role::Developer,
    Some(Utc::now() + Duration::days(90)), // Optional expiration
);

// Store raw_key securely - it cannot be retrieved later
// The raw key format is: ask_{base64_encoded_data}

// Validate API key
let claims = auth.validate_api_key(&raw_key)?;
```

**Security Features**:
- Prefix-based key format (`ask_`) for easy identification
- Keys are hashed using Argon2 before storage
- Optional expiration dates
- Last-used timestamp tracking
- Per-key role assignment

**Best Practices**:
```rust
// ✅ DO: Store raw keys securely (environment variables, secret managers)
// ✅ DO: Use different keys for different environments
// ✅ DO: Rotate keys regularly (90-180 days)
// ✅ DO: Set expiration dates for all keys
// ✅ DO: Revoke unused keys immediately

// ❌ DON'T: Commit API keys to version control
// ❌ DON'T: Share API keys between services
// ❌ DON'T: Use keys without expiration in production
```

### 3. Password Authentication

**Security Features**:
- Argon2id password hashing (memory-hard, resistant to GPU attacks)
- Automatic password strength validation
- Protection against timing attacks
- Salted hashes (automatic with Argon2)

**Password Requirements**:
- Minimum 8 characters
- Recommended: Mix of uppercase, lowercase, numbers, symbols

---

## Authorization (RBAC)

AllSource Core implements Role-Based Access Control with four predefined roles:

### Roles and Permissions

| Role | Read | Write | Admin | ManageTenants |
|------|------|-------|-------|---------------|
| **Admin** | ✅ | ✅ | ✅ | ✅ |
| **Developer** | ✅ | ✅ | ❌ | ❌ |
| **ReadOnly** | ✅ | ❌ | ❌ | ❌ |
| **ServiceAccount** | ✅ | ✅ | ❌ | ❌ |

### Permission Checking

```rust
use allsource_core::auth::{Permission, Role};
use allsource_core::middleware::AuthContext;

// In middleware/handlers
fn handle_request(auth_ctx: &AuthContext) -> Result<()> {
    // Check single permission
    auth_ctx.require_permission(Permission::Write)?;

    // Check role
    if auth_ctx.role() == Role::Admin {
        // Admin-only logic
    }

    Ok(())
}
```

### Custom Permission Logic

```rust
use allsource_core::auth::Claims;

impl Claims {
    pub fn has_permission(&self, permission: Permission) -> bool {
        match self.role {
            Role::Admin => true, // Admins have all permissions
            Role::Developer => matches!(permission,
                Permission::Read | Permission::Write
            ),
            Role::ReadOnly => matches!(permission, Permission::Read),
        }
    }
}
```

---

## Tenant Isolation

Strict multi-tenancy with complete data isolation between tenants.

### Architecture

1. **Repository Level**: All repositories filter by tenant_id
2. **Middleware Level**: TenantContext validates tenant ownership
3. **Domain Level**: Event streams validate tenant consistency

### Implementation

```rust
use allsource_core::domain::value_objects::TenantId;
use allsource_core::middleware::TenantContext;

// Middleware automatically injects TenantContext
pub async fn tenant_isolation_middleware(
    // ... extracts tenant from auth context
    // ... validates tenant is active
    // ... injects TenantContext into request
) -> Result<Response> {
    // Tenant validation happens here
}

// In your handlers
async fn handle_query(tenant_ctx: Extension<TenantContext>) -> Result<Json<Events>> {
    let tenant = &tenant_ctx.tenant;

    // All queries automatically scoped to this tenant
    let events = event_repo.get_streams_by_tenant(tenant.id()).await?;

    Ok(Json(events))
}
```

### Cross-Tenant Protection

```rust
// ✅ This is automatically prevented
let tenant1_id = TenantId::new("tenant-1")?;
let tenant2_id = TenantId::new("tenant-2")?;

// Repository methods are tenant-scoped
let events = repo.get_streams_by_tenant(&tenant1_id).await?;
// Returns only tenant-1's events, even if tenant-2's data exists

// ❌ Direct cross-tenant access is impossible
// The middleware validates the authenticated user's tenant matches requested resources
```

### Security Guarantees

- **No Shared Tables**: Each tenant's data is logically separated
- **Query Filtering**: All queries include tenant_id in WHERE clauses
- **Validation at Every Layer**: Middleware, application, and domain layers all validate
- **Audit Trail**: All cross-tenant access attempts are logged

---

## Rate Limiting

Token bucket algorithm with per-tenant rate limits.

### Configuration

```rust
use allsource_core::rate_limit::{RateLimiter, RateLimitConfig};

// Create rate limiter with default config
let rate_limiter = RateLimiter::new(RateLimitConfig::professional());

// Set custom config for specific tenant
rate_limiter.set_config("tenant-id", RateLimitConfig {
    requests_per_minute: 1000,
    burst_size: 100,
});

// Check rate limit
let result = rate_limiter.check_rate_limit("tenant-id");
if !result.allowed {
    return Err(Error::RateLimitExceeded {
        retry_after: result.retry_after_seconds,
    });
}
```

### Predefined Tiers

```rust
// Free tier: 60 req/min
let config = RateLimitConfig::free_tier();

// Professional: 600 req/min
let config = RateLimitConfig::professional();

// Unlimited: 10,000 req/min
let config = RateLimitConfig::unlimited();

// Development: 100,000 req/min
let config = RateLimitConfig::dev_mode();
```

### Rate Limiting Strategy

- **Per-Tenant**: Each tenant has independent rate limits
- **Token Bucket**: Allows bursts while maintaining average rate
- **Graceful Degradation**: Returns retry-after headers
- **Cost-Based**: Expensive operations can consume multiple tokens

### Best Practices

```rust
// ✅ DO: Set appropriate limits based on tier
// ✅ DO: Return 429 status with Retry-After header
// ✅ DO: Log rate limit violations
// ✅ DO: Consider implementing adaptive rate limiting

// ❌ DON'T: Use same limits for all tenants
// ❌ DON'T: Set limits too low (causes poor UX)
// ❌ DON'T: Set limits too high (enables abuse)
```

---

## Audit Logging

Immutable audit trail of all security-relevant events.

### What Gets Logged

1. **Authentication Events**:
   - Login attempts (success/failure)
   - Logout
   - Password changes
   - API key usage

2. **Authorization Events**:
   - Permission denied
   - Role changes
   - Access control violations

3. **Data Access Events**:
   - Event ingestion
   - Query execution
   - Schema modifications
   - Projection updates

4. **Administrative Events**:
   - Tenant creation/modification
   - User management
   - Configuration changes

### Usage

```rust
use allsource_core::domain::entities::{AuditEvent, AuditAction, AuditOutcome, Actor};
use allsource_core::domain::repositories::AuditEventRepository;

// Create audit event
let actor = Actor::User {
    user_id: user.id.to_string(),
    username: user.username.clone(),
};

let event = AuditEvent::new(
    tenant_id.clone(),
    AuditAction::Login,
    actor,
    AuditOutcome::Success,
);

// Append to audit log
audit_repo.append(event).await?;

// Query audit log (tenant-isolated)
let events = audit_repo.get_by_tenant(&tenant_id, 100, 0).await?;
```

### Audit Log Structure

```rust
pub struct AuditEvent {
    id: Uuid,
    tenant_id: TenantId,
    timestamp: DateTime<Utc>,
    action: AuditAction,      // What happened
    actor: Actor,             // Who did it
    outcome: AuditOutcome,    // Success/Failure
    metadata: serde_json::Value,
}

pub enum AuditAction {
    Login,
    Logout,
    IngestEvent,
    QueryEvents,
    CreateSchema,
    UpdateProjection,
    ModifyTenant,
    ChangePermissions,
}

pub enum AuditOutcome {
    Success,
    Failure,
    PartialSuccess,
}
```

### Compliance

Audit logs support compliance with:
- **SOC 2**: Complete audit trail
- **GDPR**: Data access logging
- **HIPAA**: Security event logging
- **PCI-DSS**: Access control monitoring

---

## Network Security

### IP Filtering

Global and per-tenant IP allowlists/blocklists.

```rust
use allsource_core::infrastructure::security::IpFilter;
use std::net::IpAddr;

let ip_filter = IpFilter::new();

// Global allowlist (applies to all tenants)
ip_filter.add_to_global_allowlist(
    "10.0.0.0/8".parse()?
);

// Global blocklist (highest priority)
ip_filter.add_to_global_blocklist(
    "192.168.1.100".parse()?
);

// Per-tenant allowlist
ip_filter.add_to_tenant_allowlist(
    &tenant_id,
    "203.0.113.0/24".parse()?
);

// Check if IP is allowed
let result = ip_filter.is_allowed_for_tenant(&tenant_id, &client_ip);
if !result.allowed {
    return Err(Error::IpBlocked { reason: result.reason });
}
```

### Filter Priority

1. **Global Blocklist** (highest priority)
2. **Tenant Blocklist**
3. **Tenant Allowlist**
4. **Global Allowlist**
5. **Default Action** (allow or block)

### Use Cases

```rust
// Office IP restriction
ip_filter.add_to_tenant_allowlist(&tenant, "203.0.113.0/24".parse()?);

// Block malicious IPs globally
ip_filter.add_to_global_blocklist("198.51.100.50".parse()?);

// VPN-only access
let vpn_ips = vec!["10.0.0.0/8", "172.16.0.0/12"];
for ip in vpn_ips {
    ip_filter.add_to_global_allowlist(ip.parse()?);
}
```

---

## Security Headers

Comprehensive security headers for defense-in-depth.

### Configuration

```rust
use allsource_core::middleware::{SecurityConfig, FrameOptions};

let config = SecurityConfig {
    // HTTP Strict Transport Security
    enable_hsts: true,
    hsts_max_age: 31536000, // 1 year

    // Frame Options (Clickjacking protection)
    enable_frame_options: true,
    frame_options: FrameOptions::Deny,

    // Content Type Sniffing protection
    enable_content_type_options: true,

    // XSS Protection
    enable_xss_protection: true,

    // Content Security Policy
    csp: Some("default-src 'self'; script-src 'self' 'unsafe-inline'".to_string()),

    // CORS configuration
    cors_origins: vec!["https://app.example.com".to_string()],
    cors_methods: vec!["GET".to_string(), "POST".to_string()],
    cors_headers: vec!["Content-Type".to_string(), "Authorization".to_string()],
    cors_max_age: 3600,
};
```

### Headers Applied

| Header | Value | Purpose |
|--------|-------|---------|
| `Strict-Transport-Security` | `max-age=31536000` | Force HTTPS |
| `X-Frame-Options` | `DENY` | Prevent clickjacking |
| `X-Content-Type-Options` | `nosniff` | Prevent MIME sniffing |
| `X-XSS-Protection` | `1; mode=block` | XSS protection |
| `Content-Security-Policy` | Custom | Control resource loading |
| `X-Request-ID` | UUID | Request tracing |

### Content Security Policy

```rust
// Strict CSP (recommended for production)
let csp = "default-src 'none'; \
           script-src 'self'; \
           style-src 'self'; \
           img-src 'self' data:; \
           font-src 'self'; \
           connect-src 'self'; \
           frame-ancestors 'none'";

config.csp = Some(csp.to_string());
```

---

## Security Best Practices

### 1. Authentication

- **Use HTTPS only** in production
- **Rotate JWT secrets** every 90 days
- **Implement MFA** for admin accounts
- **Use short-lived tokens** (1-24 hours)
- **Implement token refresh** mechanism
- **Store secrets in environment variables** or secret managers

### 2. Authorization

- **Principle of least privilege**: Grant minimum required permissions
- **Regular permission audits**: Review and revoke unused permissions
- **Separate admin accounts**: Don't use admin for daily operations
- **Log all permission changes**: Track who changed what

### 3. Data Protection

- **Encrypt data at rest**: Use database encryption
- **Encrypt data in transit**: TLS 1.3 minimum
- **Secure key management**: Use KMS or HSM for production
- **Regular backups**: Automated, encrypted, tested
- **Data retention policies**: Delete old data per compliance requirements

### 4. Operational Security

- **Security scanning**: Regular vulnerability scans
- **Dependency updates**: Keep dependencies up to date
- **Security monitoring**: Real-time alerts for suspicious activity
- **Incident response plan**: Document and test
- **Regular penetration testing**: Annual or after major changes

### 5. Development Security

```rust
// ✅ DO: Use parameterized queries
let events = query!("SELECT * FROM events WHERE tenant_id = $1", tenant_id);

// ✅ DO: Validate all inputs
let tenant_id = TenantId::new(input)?; // Returns error if invalid

// ✅ DO: Use type-safe APIs
let event = Event::from_strings(event_type, entity_id, tenant_id, data, metadata)?;

// ❌ DON'T: Concatenate SQL queries
// ❌ DON'T: Trust user input
// ❌ DON'T: Expose internal errors to clients
```

### 6. Deployment Security

```bash
# Use environment variables for secrets
export JWT_SECRET=$(openssl rand -base64 64)
export DATABASE_URL="postgresql://..."

# Don't commit secrets
echo ".env" >> .gitignore
echo "secrets/" >> .gitignore

# Use secure configurations
export RUST_LOG=warn  # Don't log sensitive data in production
export ENABLE_DEBUG=false
```

---

## Vulnerability Reporting

### Reporting a Security Vulnerability

If you discover a security vulnerability, please follow responsible disclosure:

1. **DO NOT** create a public GitHub issue
2. **Email**: security@example.com (replace with actual address)
3. **Include**:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

### Response Timeline

- **24 hours**: Initial acknowledgment
- **7 days**: Preliminary assessment
- **30 days**: Fix developed and tested
- **90 days**: Public disclosure (coordinated)

### Security Updates

- Security patches are released immediately
- Update notifications via:
  - GitHub Security Advisories
  - Email (if registered)
  - Release notes

---

## Security Testing

AllSource Core includes comprehensive security tests:

```bash
# Run security integration tests
cargo test --lib security_integration_tests

# Tests cover:
# - Authentication (JWT, API Keys, Passwords)
# - Authorization (RBAC, Permissions)
# - Tenant Isolation (Repository, Streams, Cross-tenant)
# - Rate Limiting (Per-tenant, Multi-tenant)
# - Audit Logging (Event recording, Tenant isolation)
# - IP Filtering (Global, Per-tenant)
# - Security Headers (Configuration, Application)
```

---

## Advanced Security Features

AllSource Core includes enterprise-grade advanced security capabilities for proactive threat detection, data protection, and automated security operations.

### 1. ML-Based Anomaly Detection

**Purpose**: Automatically detect suspicious patterns and security threats in audit logs using statistical analysis and behavioral baselining.

**Features**:
- Brute force attack detection (5+ failed logins in 15 minutes)
- Unusual access pattern detection (outside typical hours, actions)
- Privilege escalation attempts (unauthorized sensitive operations)
- Data exfiltration patterns (5x normal query rate)
- Velocity anomalies (impossibly fast actions)

**Configuration**:
```rust
use allsource_core::security::{AnomalyDetector, AnomalyDetectionConfig};

let config = AnomalyDetectionConfig {
    enabled: true,
    enable_brute_force_detection: true,
    enable_unusual_access_detection: true,
    enable_privilege_escalation_detection: true,
    enable_data_exfiltration_detection: true,
    enable_velocity_detection: true,
    anomaly_threshold: 0.7,  // 0.0-1.0
};

let detector = AnomalyDetector::new(config);

// Analyze events
let result = detector.analyze_event(&audit_event)?;

if result.is_anomalous {
    match result.recommended_action {
        RecommendedAction::RevokeAccess => /* Immediate block */,
        RecommendedAction::Block => /* Block user */,
        RecommendedAction::RequireMFA => /* Force MFA */,
        RecommendedAction::Alert => /* Send alert */,
        RecommendedAction::Monitor => /* Log for review */,
    }
}
```

### 2. Field-Level Encryption

**Purpose**: Transparent encryption/decryption of sensitive data fields with automatic key rotation support.

**Features**:
- AES-256-GCM encryption
- Per-field encryption keys
- Automatic key rotation without downtime
- Support for multiple key versions
- Envelope encryption pattern

**Usage**:
```rust
use allsource_core::security::{FieldEncryption, EncryptionConfig};

let config = EncryptionConfig {
    enabled: true,
    key_rotation_days: 90,
    algorithm: EncryptionAlgorithm::Aes256Gcm,
};

let encryption = FieldEncryption::new(config)?;

// Encrypt sensitive data
let encrypted = encryption.encrypt_string("sensitive-value", "ssn")?;

// Store encrypted data (can serialize to JSON)
let json = serde_json::to_string(&encrypted)?;

// Later, decrypt
let decrypted = encryption.decrypt_string(&encrypted)?;

// Key rotation (existing encrypted data remains readable)
encryption.rotate_keys()?;
```

### 3. HSM/KMS Integration

**Purpose**: External key management for enterprise-grade key security and compliance.

**Supported Providers**:
- AWS KMS
- Google Cloud KMS
- Azure Key Vault
- HashiCorp Vault
- PKCS#11 HSM
- Local (testing only)

**Configuration**:
```rust
use allsource_core::security::{KmsManager, KmsConfig, KmsProvider};

let config = KmsConfig {
    provider: KmsProvider::AwsKms,
    endpoint: Some("https://kms.us-east-1.amazonaws.com".to_string()),
    region: Some("us-east-1".to_string()),
    credentials_path: Some("/path/to/credentials".to_string()),
};

let kms = KmsManager::new(config)?;

// Create master key
let key = kms.create_key("master-key", KeyPurpose::Encryption, KeyAlgorithm::Aes256Gcm).await?;

// Envelope encryption
let encrypted = kms.envelope_encrypt(&key.key_id, b"sensitive data").await?;

// Envelope decryption
let decrypted = kms.envelope_decrypt(&encrypted).await?;

// Key rotation
kms.rotate_key(&key.key_id).await?;
```

### 4. Adaptive Rate Limiting

**Purpose**: ML-based automatic rate limit adjustment based on learned usage patterns, system load, and anomaly detection.

**Features**:
- Learning-based adjustment (increase/decrease based on utilization)
- Anomaly-based throttling (3x normal rate triggers reduction)
- Load-based adjustment (reduces on high CPU/memory)
- Pattern prediction (proactive increases at peak times)
- Safety limits (min/max bounds)

**Configuration**:
```rust
use allsource_core::security::{AdaptiveRateLimiter, AdaptiveRateLimitConfig, SystemLoad};

let config = AdaptiveRateLimitConfig {
    enabled: true,
    min_rate_limit: 10,
    max_rate_limit: 10_000,
    learning_window_hours: 24 * 7,  // 1 week
    adjustment_factor: 0.3,
    enable_anomaly_throttling: true,
    enable_load_based_adjustment: true,
    enable_pattern_prediction: true,
};

let limiter = AdaptiveRateLimiter::new(config);

// Check rate limit
let result = limiter.check_adaptive_limit("tenant-123")?;
if !result.allowed {
    return Err(RateLimitExceeded);
}

// Record system load for adjustments
limiter.record_system_load(SystemLoad {
    cpu_usage: 0.65,
    memory_usage: 0.72,
    active_connections: 1500,
    queue_depth: 250,
});

// Update limits periodically (e.g., every 5 minutes)
limiter.update_adaptive_limits()?;

// Get statistics
let stats = limiter.get_tenant_stats("tenant-123")?;
println!("Current limit: {}, Utilization: {:.1}%",
    stats.current_limit, stats.utilization * 100.0);
```

### 5. Security Automation & CI/CD Scanning

**Purpose**: Automated security scanning for continuous security validation in CI/CD pipelines.

**Scanners**:
- Dependency vulnerabilities (cargo audit)
- Secret detection (hardcoded credentials, API keys)
- SAST (static code analysis with clippy)
- License compliance

**Usage**:
```rust
use allsource_core::security::{SecurityScanner, SecurityScanConfig, CiCdIntegration};

let config = SecurityScanConfig {
    enabled: true,
    scan_frequency_hours: 24,
    enable_dependency_scan: true,
    enable_secrets_scan: true,
    enable_sast: true,
    enable_license_check: true,
    fail_on_high_severity: true,
    fail_on_medium_severity: false,
};

let mut scanner = SecurityScanner::new(config);

// Run full security scan
let result = scanner.run_full_scan()?;

match result.status {
    ScanStatus::Pass => println!("All security checks passed"),
    ScanStatus::Warning => println!("Found {} warnings", result.summary.medium + result.summary.low),
    ScanStatus::Fail => {
        println!("Security scan failed!");
        for (category, findings) in &result.findings {
            for finding in findings {
                if finding.severity == Severity::Critical || finding.severity == Severity::High {
                    println!("[{}] {}: {}", finding.severity, finding.title, finding.description);
                }
            }
        }
        std::process::exit(1);
    },
    ScanStatus::Error => println!("Scan encountered errors"),
}

// Generate CI/CD workflows
let github_workflow = CiCdIntegration::generate_github_actions_workflow();
std::fs::write(".github/workflows/security.yml", github_workflow)?;
```

**GitHub Actions Integration**:
```yaml
# .github/workflows/security.yml (auto-generated)
name: Security Scan

on:
  push:
    branches: [ main, develop ]
  pull_request:
    branches: [ main ]
  schedule:
    - cron: '0 0 * * *'  # Daily

jobs:
  security-scan:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          components: clippy
      - name: Install cargo-audit
        run: cargo install cargo-audit
      - name: Dependency Audit
        run: cargo audit
      - name: Security Clippy
        run: cargo clippy -- -D warnings
      - name: Run Security Tests
        run: cargo test --lib security
```

### Best Practices for Advanced Security

1. **Anomaly Detection**:
   - Review anomaly alerts daily
   - Tune thresholds based on your environment
   - Build user profiles over at least 1 week
   - Integrate with SIEM/alerting systems

2. **Encryption**:
   - Rotate keys every 90 days
   - Store master keys in HSM/KMS
   - Never commit encryption keys to version control
   - Use envelope encryption for large data

3. **KMS Integration**:
   - Use managed KMS in production (AWS/GCP/Azure)
   - Enable key audit logging
   - Implement key access policies
   - Test disaster recovery procedures

4. **Adaptive Rate Limiting**:
   - Set conservative min/max limits initially
   - Monitor adjustment patterns
   - Combine with static rate limits
   - Alert on aggressive throttling

5. **Security Automation**:
   - Run scans on every PR
   - Block merges on high-severity findings
   - Keep cargo audit database updated
   - Review false positives regularly

---

## Compliance and Standards

AllSource Core security architecture supports:

- **OWASP Top 10**: Protection against common vulnerabilities
- **CWE Top 25**: Most dangerous software weaknesses addressed
- **NIST Cybersecurity Framework**: Comprehensive security controls
- **SOC 2 Type II**: Security, availability, confidentiality
- **GDPR**: Data protection and privacy
- **HIPAA**: Healthcare data security
- **PCI-DSS**: Payment card security

---

## Local Development Mode

For local development and testing, AllSource Core supports a development mode that bypasses authentication and rate limiting. This allows developers to quickly iterate without needing to set up a full authentication flow.

### Enabling Development Mode

Set the `ALLSOURCE_DEV_MODE` environment variable:

```bash
# Enable development mode
export ALLSOURCE_DEV_MODE=true

# Or using inline
ALLSOURCE_DEV_MODE=true cargo run
```

### What Development Mode Does

When enabled, development mode:

1. **Bypasses Authentication**: All requests are automatically authenticated with a dev user context:
   - User ID: `dev-user`
   - Tenant ID: `dev-tenant`
   - Role: `Admin` (full permissions)

2. **Disables Rate Limiting**: Rate limits are not enforced, allowing unlimited requests.

3. **Logs a Warning**: On startup, a warning is logged to remind you that security is disabled:
   ```
   ⚠️  ALLSOURCE_DEV_MODE is enabled - authentication and rate limiting are DISABLED
   ⚠️  This should NEVER be used in production!
   ```

### When to Use Development Mode

| Use Case | Recommended |
|----------|-------------|
| Local development | ✅ Yes |
| Unit/integration testing | ✅ Yes |
| CI/CD testing | ⚠️ With caution |
| Staging environment | ❌ No |
| Production environment | ❌ NEVER |

### Alternative: Bootstrap API Key

If you need authentication but want a quick setup, use the bootstrap API key instead of development mode:

```bash
# Set the bootstrap key, then start the server normally (no dev mode)
export ALLSOURCE_BOOTSTRAP_API_KEY="<your-bootstrap-key>"
cargo run

# Authenticate using bootstrap key
curl -H "Authorization: Bearer $ALLSOURCE_BOOTSTRAP_API_KEY" \
  http://localhost:3900/api/v1/events
```

The bootstrap API key provides authenticated access while maintaining the full security stack.

### Security Comparison

| Feature | Dev Mode | Bootstrap Key | Full Auth |
|---------|----------|---------------|-----------|
| Authentication | Bypassed | Validated | Validated |
| Rate Limiting | Disabled | Enforced | Enforced |
| Audit Logging | Enabled | Enabled | Enabled |
| Tenant Isolation | `dev-tenant` | Bootstrap tenant | Per-user tenant |
| Permissions | Admin | Admin | Per-role |
| Production Safe | ❌ No | ⚠️ No | ✅ Yes |

### Best Practices

```bash
# ✅ DO: Use dev mode only locally
ALLSOURCE_DEV_MODE=true cargo run --bin allsource-core

# ✅ DO: Keep it out of CI environment variables
# (Unless specifically for development/test workflows)

# ✅ DO: Use proper auth in staging
export JWT_SECRET="production-secret-here"
cargo run --release

# ❌ DON'T: Commit .env files with dev mode enabled
# ❌ DON'T: Deploy to any shared environment with dev mode
# ❌ DON'T: Use dev mode when testing security features
```

### Verifying Development Mode Status

Check if development mode is enabled via the `/health` endpoint:

```bash
curl http://localhost:3900/health
# Response includes dev_mode: true/false
```

Or check the server logs on startup for the warning message.

---

## Security Checklist for Production

- [ ] **Ensure `ALLSOURCE_DEV_MODE` is NOT set**
- [ ] Change all default secrets and keys
- [ ] Enable HTTPS with valid TLS certificates
- [ ] Configure rate limiting for all tenants
- [ ] Set up IP filtering rules
- [ ] Enable audit logging
- [ ] Configure security headers
- [ ] Set up monitoring and alerting
- [ ] Implement backup and disaster recovery
- [ ] Document incident response procedures
- [ ] Conduct security assessment
- [ ] Train team on security practices
- [ ] Set up automated security scanning
- [ ] Configure log aggregation and analysis
- [ ] Implement secrets management solution

---

## Additional Resources

- [Authentication Chain](../../../docs/current/AUTH_CHAIN.md)
- [Multi-Tenancy Architecture](../../../docs/current/TENANT_ARCHITECTURE.md)
- [Docker Deployment](../../../docs/deployment/DOCKER.md)

---

**Last Updated**: 2026-11-23
**Version**: 0.30.0
**Maintained By**: AllSource Core Security Team
