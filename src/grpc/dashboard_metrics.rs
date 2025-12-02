//! CYAN FLAME™ Dashboard Metrics Service
//!
//! Provides real-time metrics streaming for the TUI Dashboard.
//! Enables monitoring of connected agents, system health, and network performance.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::{info, warn};

use super::proto::{
    dashboard_metrics_service_server::DashboardMetricsService,
    DashboardMetricsRequest, DashboardMetricsUpdate,
    ConnectedAgentsRequest, ConnectedAgentsResponse, ConnectedAgentSummary,
    SystemSummaryRequest, SystemSummaryResponse,
};

/// Dashboard metrics service implementation
pub struct DashboardMetricsServiceImpl {
    /// Connected agents registry
    connected_agents: Arc<RwLock<HashMap<String, AgentMetrics>>>,
    /// System metrics
    system_metrics: Arc<RwLock<SystemMetricsData>>,
    /// Server start time
    server_start_time: SystemTime,
}

#[derive(Clone, Debug)]
pub struct AgentMetrics {
    pub agent_id: String,
    pub gpu_type: String,
    pub gpu_name: String,
    pub vram_gb: f32,
    pub tflops: f32,
    pub amplification_tier: String,
    pub target_gpu: String,
    pub status: String,
    pub connected_at_ms: i64,
    pub hourly_rate_usd: f64,
}

#[derive(Clone, Debug, Default)]
pub struct SystemMetricsData {
    pub total_connections: u64,
    pub active_connections: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub calibration_requests: u64,
    pub gpu_registrations: u64,
    pub certificates_issued: u64,
    pub certificates_active: u64,
    pub certificates_revoked: u64,
    pub certificates_expired: u64,
}

impl DashboardMetricsServiceImpl {
    pub fn new() -> Self {
        Self {
            connected_agents: Arc::new(RwLock::new(HashMap::new())),
            system_metrics: Arc::new(RwLock::new(SystemMetricsData::default())),
            server_start_time: SystemTime::now(),
        }
    }
    
    /// Register a new agent (called by GPU service)
    pub async fn register_agent(&self, agent: AgentMetrics) {
        let mut agents = self.connected_agents.write().await;
        agents.insert(agent.agent_id.clone(), agent);
    }
    
    /// Unregister an agent
    pub async fn unregister_agent(&self, agent_id: &str) {
        let mut agents = self.connected_agents.write().await;
        agents.remove(agent_id);
    }
    
    /// Update system metrics
    pub async fn update_metrics(&self, updater: impl FnOnce(&mut SystemMetricsData)) {
        let mut metrics = self.system_metrics.write().await;
        updater(&mut metrics);
    }
    
    /// Get current timestamp in milliseconds
    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }
    
    /// Get uptime in seconds
    fn uptime_secs(&self) -> u64 {
        SystemTime::now()
            .duration_since(self.server_start_time)
            .unwrap_or_default()
            .as_secs()
    }
    
    /// Build dashboard metrics update
    async fn build_metrics_update(&self) -> DashboardMetricsUpdate {
        let agents = self.connected_agents.read().await;
        let metrics = self.system_metrics.read().await;
        
        // Count agents by tier
        let mut tier_counts = (0u32, 0u32, 0u32, 0u32); // free, starter, pro, enterprise
        for agent in agents.values() {
            match agent.amplification_tier.as_str() {
                "free" => tier_counts.0 += 1,
                "starter" => tier_counts.1 += 1,
                "pro" => tier_counts.2 += 1,
                "enterprise" => tier_counts.3 += 1,
                _ => {}
            }
        }
        
        // Get top agents (sorted by hourly rate)
        let mut top_agents: Vec<_> = agents.values().cloned().collect();
        top_agents.sort_by(|a, b| b.hourly_rate_usd.partial_cmp(&a.hourly_rate_usd).unwrap());
        let top_agents: Vec<ConnectedAgentSummary> = top_agents
            .into_iter()
            .take(10)
            .map(|a| ConnectedAgentSummary {
                agent_id: a.agent_id,
                gpu_type: a.gpu_type,
                gpu_name: a.gpu_name,
                vram_gb: a.vram_gb,
                tflops: a.tflops,
                amplification_tier: a.amplification_tier,
                target_gpu: a.target_gpu,
                status: a.status,
                connected_at_ms: a.connected_at_ms,
                hourly_rate_usd: a.hourly_rate_usd,
            })
            .collect();

        // Get system CPU/memory (simulated for now, would use sysinfo crate in production)
        let cpu_usage = 15.0 + (Self::now_ms() % 1000) as f32 / 100.0;
        let memory_usage = 45.0 + (Self::now_ms() % 500) as f32 / 100.0;

        DashboardMetricsUpdate {
            timestamp_ms: Self::now_ms(),
            cpu_usage_percent: cpu_usage,
            memory_usage_percent: memory_usage,
            memory_total_gb: 64.0,
            uptime_secs: self.uptime_secs(),
            total_connections: metrics.total_connections,
            active_connections: metrics.active_connections,
            bytes_in: metrics.bytes_in,
            bytes_out: metrics.bytes_out,
            requests_per_sec: (metrics.calibration_requests as f64 / self.uptime_secs().max(1) as f64),
            avg_latency_ms: 2.5,
            calibration_requests_total: metrics.calibration_requests,
            gpu_registrations_total: metrics.gpu_registrations,
            certificates_issued: metrics.certificates_issued,
            certificates_active: metrics.certificates_active,
            certificates_revoked: metrics.certificates_revoked,
            certificates_expired: metrics.certificates_expired,
            total_agents: agents.len() as u32,
            agents_by_tier_free: tier_counts.0,
            agents_by_tier_starter: tier_counts.1,
            agents_by_tier_pro: tier_counts.2,
            agents_by_tier_enterprise: tier_counts.3,
            top_agents,
        }
    }
}

#[tonic::async_trait]
impl DashboardMetricsService for DashboardMetricsServiceImpl {
    type StreamDashboardMetricsStream = ReceiverStream<Result<DashboardMetricsUpdate, Status>>;

    async fn stream_dashboard_metrics(
        &self,
        request: Request<DashboardMetricsRequest>,
    ) -> Result<Response<Self::StreamDashboardMetricsStream>, Status> {
        let req = request.into_inner();
        let refresh_interval = Duration::from_millis(req.refresh_interval_ms.max(100) as u64);

        info!("📊 Dashboard metrics stream started (refresh: {}ms)", req.refresh_interval_ms);

        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let agents = self.connected_agents.clone();
        let metrics = self.system_metrics.clone();
        let start_time = self.server_start_time;

        tokio::spawn(async move {
            loop {
                let update = {
                    let agents_read = agents.read().await;
                    let metrics_read = metrics.read().await;

                    let mut tier_counts = (0u32, 0u32, 0u32, 0u32);
                    for agent in agents_read.values() {
                        match agent.amplification_tier.as_str() {
                            "free" => tier_counts.0 += 1,
                            "starter" => tier_counts.1 += 1,
                            "pro" => tier_counts.2 += 1,
                            "enterprise" => tier_counts.3 += 1,
                            _ => {}
                        }
                    }

                    let uptime = SystemTime::now()
                        .duration_since(start_time)
                        .unwrap_or_default()
                        .as_secs();

                    DashboardMetricsUpdate {
                        timestamp_ms: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as i64,
                        cpu_usage_percent: 15.0,
                        memory_usage_percent: 45.0,
                        memory_total_gb: 64.0,
                        uptime_secs: uptime,
                        total_connections: metrics_read.total_connections,
                        active_connections: metrics_read.active_connections,
                        bytes_in: metrics_read.bytes_in,
                        bytes_out: metrics_read.bytes_out,
                        requests_per_sec: 0.0,
                        avg_latency_ms: 2.5,
                        calibration_requests_total: metrics_read.calibration_requests,
                        gpu_registrations_total: metrics_read.gpu_registrations,
                        certificates_issued: metrics_read.certificates_issued,
                        certificates_active: metrics_read.certificates_active,
                        certificates_revoked: metrics_read.certificates_revoked,
                        certificates_expired: metrics_read.certificates_expired,
                        total_agents: agents_read.len() as u32,
                        agents_by_tier_free: tier_counts.0,
                        agents_by_tier_starter: tier_counts.1,
                        agents_by_tier_pro: tier_counts.2,
                        agents_by_tier_enterprise: tier_counts.3,
                        top_agents: vec![],
                    }
                };

                if tx.send(Ok(update)).await.is_err() {
                    break;
                }
                tokio::time::sleep(refresh_interval).await;
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn get_connected_agents(
        &self,
        request: Request<ConnectedAgentsRequest>,
    ) -> Result<Response<ConnectedAgentsResponse>, Status> {
        let req = request.into_inner();
        let agents = self.connected_agents.read().await;

        let limit = req.limit.min(1000).max(1) as usize;
        let agent_list: Vec<ConnectedAgentSummary> = agents
            .values()
            .filter(|a| {
                (req.filter_tier.is_empty() || a.amplification_tier == req.filter_tier) &&
                (req.filter_gpu_type.is_empty() || a.gpu_type == req.filter_gpu_type)
            })
            .take(limit)
            .map(|a| ConnectedAgentSummary {
                agent_id: a.agent_id.clone(),
                gpu_type: a.gpu_type.clone(),
                gpu_name: a.gpu_name.clone(),
                vram_gb: a.vram_gb,
                tflops: a.tflops,
                amplification_tier: a.amplification_tier.clone(),
                target_gpu: a.target_gpu.clone(),
                status: a.status.clone(),
                connected_at_ms: a.connected_at_ms,
                hourly_rate_usd: a.hourly_rate_usd,
            })
            .collect();

        Ok(Response::new(ConnectedAgentsResponse {
            success: true,
            error_message: String::new(),
            total_count: agents.len() as u32,
            agents: agent_list,
        }))
    }

    async fn get_system_summary(
        &self,
        _request: Request<SystemSummaryRequest>,
    ) -> Result<Response<SystemSummaryResponse>, Status> {
        let agents = self.connected_agents.read().await;

        // Calculate totals
        let total_tflops: f64 = agents.values().map(|a| a.tflops as f64).sum();
        let total_vram: f64 = agents.values().map(|a| a.vram_gb as f64).sum();
        let hourly_revenue: f64 = agents.values().map(|a| a.hourly_rate_usd).sum();

        // Calculate amplified capacity (using average 100× amplification)
        let amplified_tflops = total_tflops * 100.0;
        let amplified_vram = total_vram * 100.0;

        Ok(Response::new(SystemSummaryResponse {
            success: true,
            error_message: String::new(),
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            started_at_ms: self.server_start_time
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64,
            uptime_secs: self.uptime_secs(),
            total_agents_connected: agents.len() as u32,
            total_tflops_capacity: total_tflops,
            total_vram_capacity_gb: total_vram,
            total_amplified_tflops: amplified_tflops,
            total_amplified_vram_gb: amplified_vram,
            hourly_revenue_usd: hourly_revenue,
            daily_revenue_estimate_usd: hourly_revenue * 24.0,
            monthly_revenue_estimate_usd: hourly_revenue * 24.0 * 30.0,
            cluster_health: "healthy".to_string(),
            healthy_nodes: agents.len() as u32,
            unhealthy_nodes: 0,
        }))
    }
}

