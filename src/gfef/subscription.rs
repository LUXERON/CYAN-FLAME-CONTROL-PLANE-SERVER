//! Subscription Tier Management
//! 
//! Controls access to GFEF indices and calibration matrices based on subscription level.

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Subscription tiers with corresponding amplification benefits
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubscriptionTier {
    /// Free trial - No GFEF index, 1× compression (no benefit)
    Trial,
    /// Developer tier - Cached index (24h), 100× compression
    Developer,
    /// Professional tier - Real-time index, 1,250× compression
    Professional,
    /// Enterprise tier - Custom models, 10,000× compression
    Enterprise,
}

impl SubscriptionTier {
    /// Get the amplification factor for this tier
    pub fn amplification_factor(&self) -> f64 {
        match self {
            Self::Trial => 1.0,
            Self::Developer => 100.0,
            Self::Professional => 1_250.0,
            Self::Enterprise => 10_000.0,
        }
    }
    
    /// Get GFEF index access level
    pub fn gfef_access(&self) -> GFEFAccess {
        match self {
            Self::Trial => GFEFAccess::None,
            Self::Developer => GFEFAccess::Cached { ttl_hours: 24 },
            Self::Professional => GFEFAccess::RealTime,
            Self::Enterprise => GFEFAccess::CustomModels,
        }
    }
    
    /// Get monthly prediction quota
    pub fn monthly_predictions(&self) -> u64 {
        match self {
            Self::Trial => 1_000,
            Self::Developer => 100_000,
            Self::Professional => 1_000_000,
            Self::Enterprise => u64::MAX, // Unlimited
        }
    }
    
    /// Get calibration matrix rotation interval (seconds)
    pub fn calibration_rotation_secs(&self) -> u64 {
        match self {
            Self::Trial => 0, // No rotation (1× only)
            Self::Developer => 3600, // 1 hour
            Self::Professional => 60, // 60 seconds
            Self::Enterprise => 10, // 10 seconds (highest security)
        }
    }
}

/// GFEF Index access levels
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GFEFAccess {
    /// No index access
    None,
    /// Cached index with TTL
    Cached { ttl_hours: u32 },
    /// Real-time predictions
    RealTime,
    /// Custom model index generation
    CustomModels,
}

/// Customer subscription record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub customer_id: Uuid,
    pub tier: SubscriptionTier,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub predictions_used: u64,
    pub predictions_quota: u64,
    pub models_registered: Vec<String>,
}

impl Subscription {
    pub fn new(customer_id: Uuid, tier: SubscriptionTier) -> Self {
        let now = Utc::now();
        Self {
            customer_id,
            tier,
            created_at: now,
            expires_at: now + chrono::Duration::days(30),
            predictions_used: 0,
            predictions_quota: tier.monthly_predictions(),
            models_registered: Vec::new(),
        }
    }
    
    /// Check if subscription is active
    pub fn is_active(&self) -> bool {
        Utc::now() < self.expires_at
    }
    
    /// Check if predictions quota is available
    pub fn has_quota(&self) -> bool {
        self.predictions_used < self.predictions_quota
    }
    
    /// Consume one prediction from quota
    pub fn consume_prediction(&mut self) -> bool {
        if self.has_quota() {
            self.predictions_used += 1;
            true
        } else {
            false
        }
    }
}

/// Subscription manager
pub struct SubscriptionManager {
    // In production, this would use PostgreSQL
    subscriptions: std::collections::HashMap<Uuid, Subscription>,
}

impl SubscriptionManager {
    pub fn new() -> Self {
        Self {
            subscriptions: std::collections::HashMap::new(),
        }
    }
    
    pub fn create_subscription(&mut self, customer_id: Uuid, tier: SubscriptionTier) -> Subscription {
        let sub = Subscription::new(customer_id, tier);
        self.subscriptions.insert(customer_id, sub.clone());
        sub
    }
    
    pub fn get_subscription(&self, customer_id: &Uuid) -> Option<&Subscription> {
        self.subscriptions.get(customer_id)
    }
    
    pub fn validate_access(&self, customer_id: &Uuid) -> Result<&Subscription, SubscriptionError> {
        let sub = self.subscriptions.get(customer_id)
            .ok_or(SubscriptionError::NotFound)?;
        
        if !sub.is_active() {
            return Err(SubscriptionError::Expired);
        }
        
        if !sub.has_quota() {
            return Err(SubscriptionError::QuotaExceeded);
        }
        
        Ok(sub)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SubscriptionError {
    #[error("Subscription not found")]
    NotFound,
    #[error("Subscription expired")]
    Expired,
    #[error("Prediction quota exceeded")]
    QuotaExceeded,
}

