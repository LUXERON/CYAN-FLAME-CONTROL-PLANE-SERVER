//! Operations Service Implementation
//!
//! Provides operational commands for SDK agents including
//! health checks, system info, and agent management.

use std::collections::HashMap;
use std::pin::Pin;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio_stream::Stream;
use tonic::{Request, Response, Status};
use tracing::{debug, info, warn};

use super::proto::*;
use super::OperationsService;

/// Server start time for uptime calculation
static START_TIME: std::sync::OnceLock<SystemTime> = std::sync::OnceLock::new();

/// Operations Service Implementation
pub struct OperationsServiceImpl {
    /// Server version
    version: String,
}

impl OperationsServiceImpl {
    /// Create new OperationsService
    pub fn new() -> Self {
        START_TIME.get_or_init(SystemTime::now);
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

#[tonic::async_trait]
impl OperationsService for OperationsServiceImpl {
    type UpgradeAgentStream = Pin<Box<dyn Stream<Item = Result<UpgradeProgress, Status>> + Send>>;

    async fn health_check(
        &self,
        request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        let req = request.into_inner();
        debug!("Health check from agent: {}", req.agent_id);

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let mut details = HashMap::new();
        details.insert("version".to_string(), self.version.clone());
        details.insert("service".to_string(), "CYAN FLAME Control Plane".to_string());

        Ok(Response::new(HealthCheckResponse {
            healthy: true,
            status: "SERVING".to_string(),
            timestamp_ms: now_ms,
            details,
        }))
    }

    async fn upgrade_agent(
        &self,
        request: Request<UpgradeRequest>,
    ) -> Result<Response<Self::UpgradeAgentStream>, Status> {
        let req = request.into_inner();
        info!("Upgrade request for agent: {} to version {}", req.agent_id, req.target_version);

        let output_stream = async_stream::stream! {
            // Stage 1: Downloading
            yield Ok(UpgradeProgress {
                stage: "DOWNLOADING".to_string(),
                progress_percent: 0.0,
                message: "Starting download...".to_string(),
                success: false,
                error: String::new(),
            });

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            yield Ok(UpgradeProgress {
                stage: "DOWNLOADING".to_string(),
                progress_percent: 50.0,
                message: "Download in progress...".to_string(),
                success: false,
                error: String::new(),
            });

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            // Stage 2: Verifying
            yield Ok(UpgradeProgress {
                stage: "VERIFYING".to_string(),
                progress_percent: 100.0,
                message: "Verifying checksum...".to_string(),
                success: false,
                error: String::new(),
            });

            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

            // Stage 3: Complete
            yield Ok(UpgradeProgress {
                stage: "COMPLETE".to_string(),
                progress_percent: 100.0,
                message: "Upgrade simulation complete".to_string(),
                success: true,
                error: String::new(),
            });
        };

        Ok(Response::new(Box::pin(output_stream)))
    }

    async fn get_system_info(
        &self,
        request: Request<SystemInfoRequest>,
    ) -> Result<Response<SystemInfoResponse>, Status> {
        let req = request.into_inner();
        info!("System info request from agent: {}", req.agent_id);

        let start = START_TIME.get().unwrap();
        let uptime_secs = start.elapsed().map(|d| d.as_secs()).unwrap_or(0);
        let started_at_ms = start
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // Get system info
        let sys = sysinfo::System::new_all();
        let total_memory_gb = sys.total_memory() / (1024 * 1024 * 1024);
        let cpu_count = sys.cpus().len() as u32;

        Ok(Response::new(SystemInfoResponse {
            agent_version: self.version.clone(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            cpu_cores: cpu_count,
            total_memory_gb,
            gpu: None, // Control plane doesn't have GPU info
            started_at_ms,
            uptime_seconds: uptime_secs,
        }))
    }

    async fn restart_agent(
        &self,
        request: Request<RestartRequest>,
    ) -> Result<Response<RestartResponse>, Status> {
        let req = request.into_inner();
        info!("Restart request for agent: {} (graceful: {}, delay: {}s)",
            req.agent_id, req.graceful, req.delay_seconds);

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let restart_at_ms = now_ms + (req.delay_seconds as i64 * 1000);

        Ok(Response::new(RestartResponse {
            accepted: true,
            message: format!(
                "Restart {} scheduled for agent {}",
                if req.graceful { "gracefully" } else { "immediately" },
                req.agent_id
            ),
            restart_at_ms,
        }))
    }
}

