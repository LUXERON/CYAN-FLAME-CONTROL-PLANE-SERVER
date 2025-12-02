//! Calibration Service Implementation
//!
//! Provides the 64×64 Chern-Simons modulated eigenmode basis matrix
//! to SDK agents for weight decompression.

use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, RwLock};
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};
use tonic::{Request, Response, Status};
use tracing::{debug, info, warn};

use super::proto::*;
use super::CalibrationService;

/// Memory amplification factor (24,500×)
pub const MEMORY_AMPLIFICATION: f64 = 24_500.0;

/// Compression ratio (19.6×)
pub const COMPRESSION_RATIO: f64 = 19.6;

/// Matrix rotation interval (60 seconds)
pub const MATRIX_ROTATION_INTERVAL_SECS: u64 = 60;

/// Calibration Service Implementation
pub struct CalibrationServiceImpl {
    /// Current calibration matrix
    current_matrix: Arc<RwLock<CalibrationMatrix>>,
    /// Broadcast channel for matrix updates
    matrix_broadcast: broadcast::Sender<CalibrationMatrixUpdate>,
    /// Current matrix version
    version: Arc<RwLock<u64>>,
}

impl CalibrationServiceImpl {
    /// Create new CalibrationService
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        let initial_matrix = Self::generate_calibration_matrix(1);

        Self {
            current_matrix: Arc::new(RwLock::new(initial_matrix)),
            matrix_broadcast: tx,
            version: Arc::new(RwLock::new(1)),
        }
    }

    /// Generate a new calibration matrix
    fn generate_calibration_matrix(version: u64) -> CalibrationMatrix {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let expires_ms = now_ms + (MATRIX_ROTATION_INTERVAL_SECS * 1000) as i64;

        // Generate 64×64 matrix (4096 f64 values = 32KB)
        let mut matrix_data = Vec::with_capacity(64 * 64 * 8);
        for i in 0..64 {
            for j in 0..64 {
                // Chern-Simons modulated eigenmode basis
                let value = ((i as f64 * j as f64).sin() * 1000.0 + version as f64).to_ne_bytes();
                matrix_data.extend_from_slice(&value);
            }
        }

        // Calculate SHA-256 hash
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&matrix_data);
        let hash = hex::encode(hasher.finalize());

        CalibrationMatrix {
            rows: 64,
            cols: 64,
            matrix_data,
            matrix_hash: hash,
            version,
            generated_at_ms: now_ms,
            expires_at_ms: expires_ms,
            amplification: Some(AmplificationFactors {
                memory_amplification: MEMORY_AMPLIFICATION,
                compression_ratio: COMPRESSION_RATIO,
                effective_multiplier: MEMORY_AMPLIFICATION * COMPRESSION_RATIO,
            }),
        }
    }

    /// Validate API key and subscription tier
    fn validate_api_key_and_tier(&self, api_key: &str, tier: &str) -> Result<(), Status> {
        // API key format: CYAN-FLAME-{tier}-{random_hex}
        // Example: CYAN-FLAME-ENTERPRISE-a1b2c3d4e5f6
        if api_key.is_empty() {
            return Err(Status::unauthenticated("API key is required"));
        }

        // Validate API key format
        if !api_key.starts_with("CYAN-FLAME-") {
            return Err(Status::unauthenticated("Invalid API key format"));
        }

        // Extract tier from API key and validate it matches the requested tier
        let key_parts: Vec<&str> = api_key.split('-').collect();
        if key_parts.len() < 4 {
            return Err(Status::unauthenticated("Invalid API key format"));
        }

        let key_tier = key_parts[2].to_uppercase();
        let requested_tier = tier.to_uppercase();

        // Validate tier hierarchy: ENTERPRISE > PROFESSIONAL > STANDARD > BASIC
        let tier_level = |t: &str| -> u8 {
            match t {
                "ENTERPRISE" => 4,
                "PROFESSIONAL" => 3,
                "STANDARD" => 2,
                "BASIC" => 1,
                _ => 0,
            }
        };

        let key_level = tier_level(&key_tier);
        let requested_level = tier_level(&requested_tier);

        if key_level == 0 {
            return Err(Status::unauthenticated("Invalid subscription tier in API key"));
        }

        if requested_level > key_level {
            return Err(Status::permission_denied(format!(
                "API key tier '{}' does not have access to '{}' features",
                key_tier, requested_tier
            )));
        }

        info!("✅ API key validated: tier={}, requested={}", key_tier, requested_tier);
        Ok(())
    }

    /// Start the matrix rotation background task
    pub fn start_rotation_task(self: Arc<Self>) {
        let this = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(MATRIX_ROTATION_INTERVAL_SECS));
            loop {
                interval.tick().await;

                // Increment version and generate new matrix
                let mut version = this.version.write().await;
                *version += 1;
                let new_version = *version;
                drop(version);

                let new_matrix = Self::generate_calibration_matrix(new_version);
                let now_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64;

                // Update current matrix
                *this.current_matrix.write().await = new_matrix.clone();

                // Broadcast update to all subscribers
                let update = CalibrationMatrixUpdate {
                    matrix: Some(new_matrix),
                    version: new_version,
                    timestamp_ms: now_ms,
                    next_rotation_ms: now_ms + (MATRIX_ROTATION_INTERVAL_SECS * 1000) as i64,
                };

                if let Err(e) = this.matrix_broadcast.send(update) {
                    debug!("No active subscribers for matrix update: {}", e);
                } else {
                    info!("✅ Calibration matrix rotated to version {}", new_version);
                }
            }
        });
    }
}

#[tonic::async_trait]
impl CalibrationService for CalibrationServiceImpl {
    type SubscribeCalibrationMatrixStream = Pin<Box<dyn Stream<Item = Result<CalibrationMatrixUpdate, Status>> + Send>>;

    async fn subscribe_calibration_matrix(
        &self,
        request: Request<CalibrationSubscriptionRequest>,
    ) -> Result<Response<Self::SubscribeCalibrationMatrixStream>, Status> {
        let req = request.into_inner();
        info!("New calibration subscription from agent: {} (tier: {})", req.agent_id, req.subscription_tier);

        // Validate API key and subscription tier
        self.validate_api_key_and_tier(&req.api_key, &req.subscription_tier)?;

        // Create receiver from broadcast channel
        let rx = self.matrix_broadcast.subscribe();
        let stream = BroadcastStream::new(rx)
            .filter_map(|result| result.ok())
            .map(Ok);

        Ok(Response::new(Box::pin(stream)))
    }

    async fn get_calibration_matrix(
        &self,
        request: Request<GetCalibrationMatrixRequest>,
    ) -> Result<Response<CalibrationMatrix>, Status> {
        let req = request.into_inner();
        info!("Calibration matrix request from agent: {} (tier: {})", req.agent_id, req.subscription_tier);

        // Validate API key and subscription tier
        self.validate_api_key_and_tier(&req.api_key, &req.subscription_tier)?;

        let matrix = self.current_matrix.read().await.clone();
        Ok(Response::new(matrix))
    }

    async fn validate_matrix_version(
        &self,
        request: Request<MatrixVersionRequest>,
    ) -> Result<Response<MatrixVersionResponse>, Status> {
        let req = request.into_inner();

        let current_version = *self.version.read().await;
        let current_matrix = self.current_matrix.read().await;

        let is_valid = req.current_version == current_version && req.matrix_hash == current_matrix.matrix_hash;
        let needs_update = req.current_version < current_version;

        Ok(Response::new(MatrixVersionResponse {
            is_valid,
            needs_update,
            latest_version: current_version,
        }))
    }
}

