//! Telemetry Service Implementation
//!
//! Receives GPU status telemetry from SDK agents and provides
//! aggregated metrics for monitoring and capacity planning.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, RwLock};
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};
use tracing::{debug, info, warn};

use super::proto::*;
use super::TelemetryService;

/// Telemetry Service Implementation
pub struct TelemetryServiceImpl {
    /// Connected agents and their latest memory status
    agents: Arc<RwLock<HashMap<String, MemoryStatusUpdate>>>,
    /// Network capacity broadcast channel
    capacity_broadcast: broadcast::Sender<NetworkCapacityUpdate>,
}

impl TelemetryServiceImpl {
    /// Create new TelemetryService
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            capacity_broadcast: tx,
        }
    }

    /// Calculate network capacity from all agents
    async fn calculate_network_capacity(&self) -> NetworkCapacityUpdate {
        let agents = self.agents.read().await;
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let mut total_physical = 0u64;
        let mut total_effective = 0u64;
        let mut allocated_effective = 0u64;
        let mut nodes = Vec::new();

        for (agent_id, status) in agents.iter() {
            if let Some(gpu) = &status.gpu_status {
                total_physical += gpu.total_mb / 1024; // Convert to GB
            }
            if let Some(eff) = &status.effective_status {
                total_effective += eff.total_tb;
                allocated_effective += eff.allocated_tb;
                nodes.push(NodeCapacity {
                    node_id: agent_id.clone(),
                    node_address: String::new(),
                    physical_gb: status.gpu_status.as_ref().map(|g| g.total_mb / 1024).unwrap_or(0),
                    effective_tb: eff.total_tb,
                    allocated_tb: eff.allocated_tb,
                    is_healthy: true,
                });
            }
        }

        let utilization = if total_effective > 0 {
            (allocated_effective as f64 / total_effective as f64) * 100.0
        } else {
            0.0
        };

        NetworkCapacityUpdate {
            timestamp_ms: now_ms,
            total_nodes: agents.len() as u32,
            active_nodes: agents.len() as u32,
            metrics: Some(NetworkCapacityMetrics {
                total_physical_gb: total_physical,
                total_effective_tb: total_effective,
                allocated_effective_tb: allocated_effective,
                available_effective_tb: total_effective.saturating_sub(allocated_effective),
                network_utilization_percent: utilization,
            }),
            nodes,
        }
    }
}

#[tonic::async_trait]
impl TelemetryService for TelemetryServiceImpl {
    type StreamMemoryStatusStream = Pin<Box<dyn Stream<Item = Result<TelemetryAck, Status>> + Send>>;
    type SubscribeNetworkCapacityStream = Pin<Box<dyn Stream<Item = Result<NetworkCapacityUpdate, Status>> + Send>>;
    type ReportHealthStream = Pin<Box<dyn Stream<Item = Result<HealthAck, Status>> + Send>>;

    async fn stream_memory_status(
        &self,
        request: Request<Streaming<MemoryStatusUpdate>>,
    ) -> Result<Response<Self::StreamMemoryStatusStream>, Status> {
        let mut stream = request.into_inner();
        let agents = self.agents.clone();
        let capacity_tx = self.capacity_broadcast.clone();
        let this_agents = self.agents.clone();

        let output_stream = async_stream::stream! {
            while let Ok(Some(status)) = stream.message().await {
                let agent_id = status.agent_id.clone();

                // Store latest status
                agents.write().await.insert(agent_id.clone(), status);

                // Broadcast capacity update
                let capacity = {
                    let agents = this_agents.read().await;
                    let now_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as i64;

                    NetworkCapacityUpdate {
                        timestamp_ms: now_ms,
                        total_nodes: agents.len() as u32,
                        active_nodes: agents.len() as u32,
                        metrics: None,
                        nodes: vec![],
                    }
                };
                let _ = capacity_tx.send(capacity);

                yield Ok(TelemetryAck {
                    received: true,
                    timestamp_ms: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as i64,
                    message: format!("Received status from {}", agent_id),
                });
            }
        };

        Ok(Response::new(Box::pin(output_stream)))
    }

    async fn subscribe_network_capacity(
        &self,
        request: Request<NetworkCapacityRequest>,
    ) -> Result<Response<Self::SubscribeNetworkCapacityStream>, Status> {
        let req = request.into_inner();
        info!("Network capacity subscription from agent: {}", req.agent_id);

        let rx = self.capacity_broadcast.subscribe();
        let stream = BroadcastStream::new(rx)
            .filter_map(|result| result.ok())
            .map(Ok);

        Ok(Response::new(Box::pin(stream)))
    }

    async fn report_health(
        &self,
        request: Request<Streaming<HealthMetrics>>,
    ) -> Result<Response<Self::ReportHealthStream>, Status> {
        let mut stream = request.into_inner();

        let output_stream = async_stream::stream! {
            while let Ok(Some(metrics)) = stream.message().await {
                debug!("Health metrics from {}: CPU {}%, Memory {}%",
                    metrics.agent_id, metrics.cpu_percent, metrics.memory_percent);

                yield Ok(HealthAck {
                    received: true,
                    timestamp_ms: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as i64,
                });
            }
        };

        Ok(Response::new(Box::pin(output_stream)))
    }
}

impl Clone for TelemetryServiceImpl {
    fn clone(&self) -> Self {
        Self {
            agents: self.agents.clone(),
            capacity_broadcast: self.capacity_broadcast.clone(),
        }
    }
}

