//! Allocation Service Implementation
//!
//! Manages effective memory allocation and routing across the
//! CYAN FLAME Virtual GPU Network.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::proto::*;
use super::AllocationService;

/// Allocation record
#[derive(Debug, Clone)]
pub struct AllocationRecord {
    pub allocation_id: String,
    pub agent_id: String,
    pub allocated_tb: u64,
    pub purpose: String,
    pub node_id: String,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
}

/// Allocation Service Implementation
pub struct AllocationServiceImpl {
    /// Active allocations by allocation_id
    allocations: Arc<RwLock<HashMap<String, AllocationRecord>>>,
    /// Total allocated TB per agent
    agent_allocations: Arc<RwLock<HashMap<String, u64>>>,
    /// Maximum allocation per agent (TB)
    max_allocation_per_agent_tb: u64,
}

impl AllocationServiceImpl {
    /// Create new AllocationService
    pub fn new() -> Self {
        Self {
            allocations: Arc::new(RwLock::new(HashMap::new())),
            agent_allocations: Arc::new(RwLock::new(HashMap::new())),
            max_allocation_per_agent_tb: 1000, // 1 PB per agent max
        }
    }
}

#[tonic::async_trait]
impl AllocationService for AllocationServiceImpl {
    async fn allocate_memory(
        &self,
        request: Request<AllocationRequest>,
    ) -> Result<Response<AllocationResponse>, Status> {
        let req = request.into_inner();
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // Check agent's current allocation
        let mut agent_allocs = self.agent_allocations.write().await;
        let current_alloc = agent_allocs.get(&req.agent_id).copied().unwrap_or(0);

        if current_alloc + req.requested_tb > self.max_allocation_per_agent_tb {
            return Ok(Response::new(AllocationResponse {
                success: false,
                allocation_id: String::new(),
                allocated_tb: 0,
                assigned_node: String::new(),
                expires_at_ms: 0,
                error_message: format!(
                    "Allocation would exceed limit. Current: {} TB, Requested: {} TB, Max: {} TB",
                    current_alloc, req.requested_tb, self.max_allocation_per_agent_tb
                ),
            }));
        }

        // Create allocation
        let allocation_id = Uuid::new_v4().to_string();
        let expires_at_ms = if req.duration_ms > 0 {
            now_ms + req.duration_ms
        } else {
            now_ms + (24 * 60 * 60 * 1000) // 24 hours default
        };

        let assigned_node = format!("node-{}", &allocation_id[..8]);

        let record = AllocationRecord {
            allocation_id: allocation_id.clone(),
            agent_id: req.agent_id.clone(),
            allocated_tb: req.requested_tb,
            purpose: req.purpose.clone(),
            node_id: assigned_node.clone(),
            created_at_ms: now_ms,
            expires_at_ms,
        };

        // Store allocation
        self.allocations.write().await.insert(allocation_id.clone(), record);
        *agent_allocs.entry(req.agent_id.clone()).or_insert(0) += req.requested_tb;

        info!(
            "✅ Allocated {} TB effective memory for agent {} (purpose: {})",
            req.requested_tb, req.agent_id, req.purpose
        );

        Ok(Response::new(AllocationResponse {
            success: true,
            allocation_id,
            allocated_tb: req.requested_tb,
            assigned_node,
            expires_at_ms,
            error_message: String::new(),
        }))
    }

    async fn free_memory(
        &self,
        request: Request<FreeMemoryRequest>,
    ) -> Result<Response<FreeMemoryResponse>, Status> {
        let req = request.into_inner();

        let mut allocations = self.allocations.write().await;

        if let Some(record) = allocations.remove(&req.allocation_id) {
            // Update agent's total allocation
            let mut agent_allocs = self.agent_allocations.write().await;
            if let Some(total) = agent_allocs.get_mut(&record.agent_id) {
                *total = total.saturating_sub(record.allocated_tb);
            }

            info!("✅ Freed allocation {} ({} TB)", req.allocation_id, record.allocated_tb);

            Ok(Response::new(FreeMemoryResponse {
                success: true,
                freed_tb: record.allocated_tb,
                message: format!("Freed {} TB from allocation {}", record.allocated_tb, req.allocation_id),
            }))
        } else {
            Ok(Response::new(FreeMemoryResponse {
                success: false,
                freed_tb: 0,
                message: "Allocation not found".to_string(),
            }))
        }
    }

    async fn route_memory_request(
        &self,
        request: Request<MemoryRoutingRequest>,
    ) -> Result<Response<MemoryRoutingResponse>, Status> {
        let req = request.into_inner();

        // Simple routing: find agent with most available capacity
        let agent_allocs = self.agent_allocations.read().await;

        let mut best_agent = None;
        let mut best_available = 0u64;

        for (agent_id, allocated) in agent_allocs.iter() {
            let available = self.max_allocation_per_agent_tb.saturating_sub(*allocated);
            if available >= req.required_tb && available > best_available {
                best_agent = Some(agent_id.clone());
                best_available = available;
            }
        }

        if let Some(target_agent) = best_agent {
            Ok(Response::new(MemoryRoutingResponse {
                found: true,
                optimal_node_id: target_agent.clone(),
                node_address: format!("grpc://{}:50051", target_agent),
                available_tb: best_available,
                latency_ms: 0.5, // Sub-millisecond
                alternatives: vec![],
            }))
        } else {
            Ok(Response::new(MemoryRoutingResponse {
                found: false,
                optimal_node_id: String::new(),
                node_address: String::new(),
                available_tb: 0,
                latency_ms: 0.0,
                alternatives: vec![],
            }))
        }
    }

    async fn get_allocation_status(
        &self,
        request: Request<AllocationStatusRequest>,
    ) -> Result<Response<AllocationStatusResponse>, Status> {
        let req = request.into_inner();
        let allocations = self.allocations.read().await;
        let agent_allocs = self.agent_allocations.read().await;

        let mut alloc_infos = Vec::new();
        let mut total_allocated = 0u64;

        for (id, record) in allocations.iter() {
            // Filter by agent_id
            if record.agent_id != req.agent_id {
                continue;
            }
            // Filter by allocation_id if specified
            if !req.allocation_id.is_empty() && id != &req.allocation_id {
                continue;
            }

            total_allocated += record.allocated_tb;
            alloc_infos.push(AllocationInfo {
                allocation_id: record.allocation_id.clone(),
                size_tb: record.allocated_tb,
                purpose: record.purpose.clone(),
                created_at_ms: record.created_at_ms,
                expires_at_ms: record.expires_at_ms,
                node_id: record.node_id.clone(),
            });
        }

        let quota = self.max_allocation_per_agent_tb;
        let remaining = quota.saturating_sub(total_allocated);

        Ok(Response::new(AllocationStatusResponse {
            allocations: alloc_infos,
            total_allocated_tb: total_allocated,
            quota_tb: quota,
            remaining_quota_tb: remaining,
        }))
    }
}

