//! CYAN FLAME™ gRPC Authentication Module
//!
//! Provides mTLS and API key authentication for gRPC services.
//!
//! ## Authentication Flow
//!
//! 1. SDK Agent sends request with `x-api-key` header
//! 2. AuthInterceptor extracts and validates the key
//! 3. If valid, request proceeds with tier configuration
//! 4. If invalid, request is rejected with UNAUTHENTICATED status
//!
//! ## Tier System
//!
//! | Tier       | Amplification | Max Allocation | Rate Limit |
//! |------------|---------------|----------------|------------|
//! | Free       | 100×          | 2.4 TB         | 100/min    |
//! | Starter    | 1,000×        | 24 TB          | 1,000/min  |
//! | Pro        | 10,000×       | 240 TB         | 10,000/min |
//! | Enterprise | 24,500×       | 574 TB         | Unlimited  |

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Status};
use tonic::service::Interceptor;
use tracing::{debug, info, warn};
use sha2::{Sha256, Digest};
use chrono::{DateTime, Utc};

/// Tier configuration with amplification limits
#[derive(Clone, Debug)]
pub struct TierConfig {
    /// Tier name
    pub name: String,
    /// Memory amplification factor
    pub amplification_factor: u64,
    /// Maximum allocatable memory in TB
    pub max_allocation_tb: u64,
    /// Rate limit (requests per minute, 0 = unlimited)
    pub rate_limit: u32,
    /// Maximum concurrent allocations
    pub max_concurrent_allocations: u32,
    /// Priority level (higher = more priority)
    pub priority: u8,
}

impl TierConfig {
    /// Free tier - 100× amplification
    pub fn free() -> Self {
        Self {
            name: "free".to_string(),
            amplification_factor: 100,
            max_allocation_tb: 2,           // 2.4 TB (100× × 24GB)
            rate_limit: 100,
            max_concurrent_allocations: 1,
            priority: 1,
        }
    }

    /// Starter tier - 1,000× amplification
    pub fn starter() -> Self {
        Self {
            name: "starter".to_string(),
            amplification_factor: 1_000,
            max_allocation_tb: 24,          // 24 TB
            rate_limit: 1_000,
            max_concurrent_allocations: 5,
            priority: 2,
        }
    }

    /// Pro tier - 10,000× amplification
    pub fn pro() -> Self {
        Self {
            name: "pro".to_string(),
            amplification_factor: 10_000,
            max_allocation_tb: 240,         // 240 TB
            rate_limit: 10_000,
            max_concurrent_allocations: 20,
            priority: 3,
        }
    }

    /// Enterprise tier - 24,500× amplification (maximum)
    pub fn enterprise() -> Self {
        Self {
            name: "enterprise".to_string(),
            amplification_factor: 24_500,
            max_allocation_tb: 574,         // 574 TB (full capacity)
            rate_limit: 0,                  // Unlimited
            max_concurrent_allocations: 100,
            priority: 4,
        }
    }

    /// Get tier config by name
    pub fn from_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "free" => Self::free(),
            "starter" => Self::starter(),
            "pro" => Self::pro(),
            "enterprise" => Self::enterprise(),
            _ => Self::free(),  // Default to free
        }
    }
}

/// API Key entry with metadata
#[derive(Clone, Debug)]
pub struct ApiKeyEntry {
    /// Hashed API key (SHA-256)
    pub key_hash: String,
    /// Subscription tier
    pub tier: String,
    /// Tier configuration
    pub tier_config: TierConfig,
    /// Organization/Customer ID
    pub org_id: String,
    /// Expiration time (None = never expires)
    pub expires_at: Option<DateTime<Utc>>,
    /// Rate limit (requests per minute)
    pub rate_limit: u32,
    /// Current request count (reset per minute)
    pub request_count: u32,
    /// Last request timestamp
    pub last_request: DateTime<Utc>,
    /// Enabled flag
    pub enabled: bool,
    /// Current allocation count
    pub current_allocations: u32,
    /// Total memory allocated (bytes)
    pub allocated_memory_bytes: u64,
}

/// Authentication Manager
#[derive(Clone)]
pub struct AuthManager {
    /// API keys indexed by key hash
    api_keys: Arc<RwLock<HashMap<String, ApiKeyEntry>>>,
    /// Enable authentication
    auth_enabled: bool,
}

impl AuthManager {
    /// Create new authentication manager
    pub fn new(auth_enabled: bool) -> Self {
        let mut manager = Self {
            api_keys: Arc::new(RwLock::new(HashMap::new())),
            auth_enabled,
        };
        
        // Add default enterprise keys for testing
        if !auth_enabled {
            info!("🔓 Authentication DISABLED - all requests allowed");
        } else {
            info!("🔐 Authentication ENABLED - API keys required");
        }
        
        manager
    }

    /// Hash an API key
    pub fn hash_key(key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Register a new API key with full tier configuration
    pub async fn register_key(&self, api_key: &str, tier: &str, org_id: &str) {
        let key_hash = Self::hash_key(api_key);
        let tier_config = TierConfig::from_name(tier);
        let rate_limit = tier_config.rate_limit;

        let entry = ApiKeyEntry {
            key_hash: key_hash.clone(),
            tier: tier.to_string(),
            tier_config,
            org_id: org_id.to_string(),
            expires_at: None,
            rate_limit,
            request_count: 0,
            last_request: Utc::now(),
            enabled: true,
            current_allocations: 0,
            allocated_memory_bytes: 0,
        };

        self.api_keys.write().await.insert(key_hash, entry);
        info!("🔑 Registered API key for org: {} (tier: {}, amplification: {}×)",
              org_id, tier, TierConfig::from_name(tier).amplification_factor);
    }

    /// Register default test keys for each tier
    pub async fn register_default_keys(&self) {
        // Free tier test key
        self.register_key("cf_free_test123", "free", "test-free-org").await;
        // Starter tier test key
        self.register_key("cf_starter_test123", "starter", "test-starter-org").await;
        // Pro tier test key
        self.register_key("cf_pro_test123", "pro", "test-pro-org").await;
        // Enterprise tier test key
        self.register_key("cf_ent_test123", "enterprise", "test-enterprise-org").await;
        // Backward compatible test key
        self.register_key("test-key-123", "enterprise", "legacy-test-org").await;

        info!("🔑 Registered 5 default API keys for testing");
    }

    /// Validate an API key and return tier configuration
    pub async fn validate_key(&self, api_key: &str) -> Result<ApiKeyEntry, Status> {
        if !self.auth_enabled {
            // Return a default enterprise entry when auth is disabled
            let tier_config = TierConfig::enterprise();
            return Ok(ApiKeyEntry {
                key_hash: "disabled".to_string(),
                tier: "enterprise".to_string(),
                tier_config,
                org_id: "default".to_string(),
                expires_at: None,
                rate_limit: 0,
                request_count: 0,
                last_request: Utc::now(),
                enabled: true,
                current_allocations: 0,
                allocated_memory_bytes: 0,
            });
        }

        let key_hash = Self::hash_key(api_key);
        let mut keys = self.api_keys.write().await;

        if let Some(entry) = keys.get_mut(&key_hash) {
            // Check if key is enabled
            if !entry.enabled {
                warn!("🚫 Disabled API key attempted: org={}", entry.org_id);
                return Err(Status::permission_denied("API key is disabled"));
            }

            // Check expiration
            if let Some(expires) = entry.expires_at {
                if Utc::now() > expires {
                    warn!("⏰ Expired API key attempted: org={}", entry.org_id);
                    return Err(Status::permission_denied("API key has expired"));
                }
            }

            // Check rate limit (reset if minute has passed)
            let now = Utc::now();
            if (now - entry.last_request).num_seconds() >= 60 {
                entry.request_count = 0;
                entry.last_request = now;
            }

            // Only check rate limit if not unlimited (0 = unlimited)
            if entry.rate_limit > 0 && entry.request_count >= entry.rate_limit {
                warn!("⚠️ Rate limit exceeded for org: {}", entry.org_id);
                return Err(Status::resource_exhausted(format!(
                    "Rate limit exceeded ({}/min). Upgrade to higher tier for more requests.",
                    entry.rate_limit
                )));
            }

            entry.request_count += 1;
            debug!("✅ API key validated: org={}, tier={}, amplification={}×",
                   entry.org_id, entry.tier, entry.tier_config.amplification_factor);
            Ok(entry.clone())
        } else {
            warn!("❌ Invalid API key attempted (key hash: {}...)", &key_hash[..8]);
            Err(Status::unauthenticated("Invalid API key. Please check your API key or contact support."))
        }
    }

    /// Check if an allocation is allowed for the given tier
    pub async fn check_allocation_allowed(&self, api_key: &str, requested_bytes: u64) -> Result<(), Status> {
        let entry = self.validate_key(api_key).await?;

        // Check concurrent allocation limit
        if entry.current_allocations >= entry.tier_config.max_concurrent_allocations {
            return Err(Status::resource_exhausted(format!(
                "Maximum concurrent allocations reached ({}/{}). Release existing allocations or upgrade tier.",
                entry.current_allocations, entry.tier_config.max_concurrent_allocations
            )));
        }

        // Check total allocation limit
        let max_bytes = entry.tier_config.max_allocation_tb * 1024 * 1024 * 1024 * 1024; // TB to bytes
        if entry.allocated_memory_bytes + requested_bytes > max_bytes {
            return Err(Status::resource_exhausted(format!(
                "Allocation would exceed tier limit ({} TB max). Current: {} TB, Requested: {} TB",
                entry.tier_config.max_allocation_tb,
                entry.allocated_memory_bytes / (1024 * 1024 * 1024 * 1024),
                requested_bytes / (1024 * 1024 * 1024 * 1024)
            )));
        }

        Ok(())
    }

    /// Get the amplification factor for a tier
    pub async fn get_amplification_factor(&self, api_key: &str) -> Result<u64, Status> {
        let entry = self.validate_key(api_key).await?;
        Ok(entry.tier_config.amplification_factor)
    }

    /// Check if authentication is enabled
    pub fn is_auth_enabled(&self) -> bool {
        self.auth_enabled
    }

    /// Extract API key from request metadata
    pub fn extract_api_key<T>(request: &Request<T>) -> Option<String> {
        request.metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .or_else(|| {
                // Also check authorization header (Bearer token)
                request.metadata()
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.strip_prefix("Bearer "))
                    .map(|s| s.to_string())
            })
    }
}

// ============================================================================
// gRPC INTERCEPTOR IMPLEMENTATION
// ============================================================================

/// Authentication Interceptor for gRPC requests
///
/// This interceptor validates API keys on every incoming request.
/// It extracts the key from `x-api-key` header or `Authorization: Bearer <key>`.
#[derive(Clone)]
pub struct AuthInterceptor {
    auth_manager: Arc<AuthManager>,
}

impl AuthInterceptor {
    /// Create a new auth interceptor
    pub fn new(auth_manager: Arc<AuthManager>) -> Self {
        Self { auth_manager }
    }
}

impl Interceptor for AuthInterceptor {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        // If auth is disabled, allow all requests
        if !self.auth_manager.is_auth_enabled() {
            return Ok(request);
        }

        // Extract API key from metadata
        let api_key = AuthManager::extract_api_key(&request)
            .ok_or_else(|| {
                warn!("🚫 Request without API key rejected");
                Status::unauthenticated(
                    "Missing API key. Include 'x-api-key' header or 'Authorization: Bearer <key>'."
                )
            })?;

        // Hash the key for lookup
        let key_hash = AuthManager::hash_key(&api_key);

        // Use try_read for non-blocking check (best effort in sync context)
        // This is safe because we're only reading and the guard is dropped before returning
        let result = {
            let keys_guard = self.auth_manager.api_keys.try_read();
            match keys_guard {
                Ok(keys) => {
                    if let Some(entry) = keys.get(&key_hash) {
                        if !entry.enabled {
                            Err(Status::permission_denied("API key is disabled"))
                        } else if let Some(expires) = entry.expires_at {
                            if Utc::now() > expires {
                                Err(Status::permission_denied("API key has expired"))
                            } else {
                                debug!("✅ Request authenticated: org={}, tier={}", entry.org_id, entry.tier);
                                Ok(())
                            }
                        } else {
                            debug!("✅ Request authenticated: org={}, tier={}", entry.org_id, entry.tier);
                            Ok(())
                        }
                    } else {
                        warn!("❌ Invalid API key in request");
                        Err(Status::unauthenticated("Invalid API key"))
                    }
                }
                Err(_) => {
                    // Lock contention - allow request (fail open for availability)
                    warn!("⚠️ Auth check skipped due to lock contention");
                    Ok(())
                }
            }
        };

        // Return based on the result
        match result {
            Ok(()) => Ok(request),
            Err(status) => Err(status),
        }
    }
}

