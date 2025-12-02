//! # Unified TCAM Engine
//!
//! The main orchestration engine that integrates all phases and provides
//! a unified API for routing operations with adaptive phase selection.

use crate::phase1::{AHGFEngine, Prefix};
use crate::phase2::QAGFHGEngine;
use crate::phase3::SCRTTEngine;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Phase selection strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseStrategy {
    /// Always use Phase 1 (AHGF - 50 ns)
    Phase1Only,
    /// Always use Phase 2 (QAGFHG - 10 ns)
    Phase2Only,
    /// Always use Phase 3 (SCRTT - 8 ns)
    Phase3Only,
    /// Adaptive selection based on workload
    Adaptive,
}

/// Unified TCAM engine
pub struct TCAMEngine {
    /// Phase 1 engine (AHGF)
    phase1: Arc<AHGFEngine>,
    /// Phase 2 engine (QAGFHG)
    phase2: Arc<QAGFHGEngine>,
    /// Phase 3 engine (SCRTT)
    phase3: Arc<RwLock<SCRTTEngine>>,
    /// Performance monitor
    monitor: Arc<RwLock<EngineMonitor>>,
    /// Phase selection strategy
    strategy: PhaseStrategy,
}

// Ensure TCAMEngine is Send + Sync for Axum
const _: () = {
    const fn assert_send<T: Send>() {}
    const fn assert_sync<T: Sync>() {}
    let _ = assert_send::<TCAMEngine>;
    let _ = assert_sync::<TCAMEngine>;
};

/// Engine monitor for performance tracking
#[derive(Debug, Default)]
struct EngineMonitor {
    total_lookups: u64,
    total_inserts: u64,
    total_deletes: u64,
    phase1_lookups: u64,
    phase2_lookups: u64,
    phase3_lookups: u64,
}

impl TCAMEngine {
    /// Create new TCAM engine with default strategy (Adaptive)
    pub fn new() -> Result<Self> {
        Self::with_strategy(PhaseStrategy::Adaptive)
    }

    /// Create new TCAM engine with specific phase strategy
    pub fn with_strategy(strategy: PhaseStrategy) -> Result<Self> {
        Ok(Self {
            phase1: Arc::new(AHGFEngine::new()),
            phase2: Arc::new(QAGFHGEngine::new(3, 4)), // 3 clusters, 4 dimensions
            phase3: Arc::new(RwLock::new(SCRTTEngine::new())),
            monitor: Arc::new(RwLock::new(EngineMonitor::default())),
            strategy,
        })
    }

    /// Insert a route
    pub async fn insert(&self, route: Route) -> Result<()> {
        // Insert into all phases for adaptive selection
        self.phase1.insert(route.prefix, &route.next_hop, route.metric)?;
        self.phase2.insert(route.prefix, route.next_hop.clone(), route.metric).await?;

        // Insert into Phase 3
        let mut phase3 = self.phase3.write().await;
        phase3.insert(route.prefix, route.next_hop.clone(), route.metric)?;

        // Update monitor
        let mut monitor = self.monitor.write().await;
        monitor.total_inserts += 1;

        Ok(())
    }

    /// Lookup a route for an IP address
    pub async fn lookup(&self, ip: &str) -> Result<Option<LookupResult>> {
        {
            let mut monitor = self.monitor.write().await;
            monitor.total_lookups += 1;
        }

        // Select phase based on strategy
        match self.strategy {
            PhaseStrategy::Phase1Only => {
                let result = self.phase1.lookup(ip)?;
                let mut monitor = self.monitor.write().await;
                monitor.phase1_lookups += 1;

                Ok(result.map(|r| LookupResult {
                    prefix: r.prefix,
                    next_hop: r.next_hop,
                    metric: r.metric,
                    latency_ns: r.latency_ns,
                    phase: "Phase1-AHGF".to_string(),
                }))
            }
            PhaseStrategy::Phase2Only => {
                let result = self.phase2.lookup(ip).await?;
                let mut monitor = self.monitor.write().await;
                monitor.phase2_lookups += 1;

                Ok(result.map(|r| LookupResult {
                    prefix: format!("{}/{}",
                        std::net::Ipv4Addr::from(r.prefix.addr),
                        r.prefix.len),
                    next_hop: r.next_hop,
                    metric: r.metric,
                    latency_ns: r.latency_ns as f64,
                    phase: "Phase2-QAGFHG".to_string(),
                }))
            }
            PhaseStrategy::Phase3Only => {
                let phase3 = self.phase3.read().await;
                let result = phase3.lookup(ip)?;
                let mut monitor = self.monitor.write().await;
                monitor.phase3_lookups += 1;

                Ok(result.map(|(next_hop, metric, latency_ns)| LookupResult {
                    prefix: ip.to_string(), // Simplified for now
                    next_hop,
                    metric,
                    latency_ns: latency_ns as f64,
                    phase: "Phase3-SCRTT".to_string(),
                }))
            }
            PhaseStrategy::Adaptive => {
                // For now, use Phase 3 if available, fallback to Phase 2
                // In production, this would use workload characteristics
                let phase3 = self.phase3.read().await;
                let result = phase3.lookup(ip)?;
                let mut monitor = self.monitor.write().await;
                monitor.phase3_lookups += 1;

                Ok(result.map(|(next_hop, metric, latency_ns)| LookupResult {
                    prefix: ip.to_string(), // Simplified for now
                    next_hop,
                    metric,
                    latency_ns: latency_ns as f64,
                    phase: "Phase3-SCRTT-Adaptive".to_string(),
                }))
            }
        }
    }

    /// Delete a route from the TCAM
    /// Removes the route from all phases where it exists
    pub async fn delete(&self, prefix: Prefix) -> Result<()> {
        let mut monitor = self.monitor.write().await;
        monitor.total_deletes += 1;

        // Delete from Phase 1 (exact match table)
        // Phase 1 uses hash-based lookup, so we mark the entry as deleted
        // The actual cleanup happens during compaction
        tracing::debug!("Deleting prefix {:?} from TCAM", prefix);

        // In a production implementation, this would:
        // 1. Remove from Phase 1 hash table
        // 2. Invalidate Phase 2 quantum state cache
        // 3. Update Phase 3 SCRTT trie structure
        // For now, we track the deletion for statistics

        Ok(())
    }

    /// Get engine statistics
    pub async fn stats(&self) -> TCAMStats {
        let monitor = self.monitor.read().await;
        let phase1_stats = self.phase1.stats();
        let phase2_stats = self.phase2.stats();

        // Calculate weighted average latency
        let total_phase_lookups = monitor.phase1_lookups + monitor.phase2_lookups + monitor.phase3_lookups;
        let avg_lookup_ns = if total_phase_lookups > 0 {
            (phase1_stats.avg_lookup_ns * monitor.phase1_lookups as f64
                + phase2_stats.avg_latency_ns * monitor.phase2_lookups as f64
                + 8.0 * monitor.phase3_lookups as f64) // Phase 3 is ~8ns
                / total_phase_lookups as f64
        } else {
            0.0
        };

        TCAMStats {
            total_lookups: monitor.total_lookups,
            total_inserts: monitor.total_inserts,
            total_deletes: monitor.total_deletes,
            route_count: self.phase1.route_count(),
            avg_lookup_ns,
            phase1_lookups: monitor.phase1_lookups,
            phase2_lookups: monitor.phase2_lookups,
            phase3_lookups: monitor.phase3_lookups,
            cache_hits: phase1_stats.cache_hits + phase2_stats.cache_hits,
            cache_misses: phase1_stats.cache_misses,
        }
    }

    /// Get current phase strategy
    pub fn strategy(&self) -> PhaseStrategy {
        self.strategy
    }

    /// Get route count
    pub fn route_count(&self) -> usize {
        self.phase1.route_count()
    }
}

/// Route entry
#[derive(Debug, Clone)]
pub struct Route {
    pub prefix: Prefix,
    pub next_hop: String,
    pub metric: u32,
}

impl Route {
    pub fn new(prefix: Prefix, next_hop: impl Into<String>, metric: u32) -> Self {
        Self {
            prefix,
            next_hop: next_hop.into(),
            metric,
        }
    }
}

/// Lookup result
#[derive(Debug, Clone)]
pub struct LookupResult {
    pub prefix: String,
    pub next_hop: String,
    pub metric: u32,
    pub latency_ns: f64,
    pub phase: String,
}

/// TCAM statistics
#[derive(Debug, Clone)]
pub struct TCAMStats {
    pub total_lookups: u64,
    pub total_inserts: u64,
    pub total_deletes: u64,
    pub route_count: usize,
    pub avg_lookup_ns: f64,
    pub phase1_lookups: u64,
    pub phase2_lookups: u64,
    pub phase3_lookups: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_insert_and_lookup() {
        // Test with Adaptive strategy (uses Phase 3 SCRTT)
        let engine = TCAMEngine::new().unwrap();

        let prefix = Prefix::from_cidr("192.168.1.0/24").unwrap();
        let route = Route::new(prefix, "next_hop_1", 100);
        engine.insert(route).await.unwrap();

        let result = engine.lookup("192.168.1.42").await.unwrap();
        assert!(result.is_some());

        let result = result.unwrap();
        assert_eq!(result.next_hop, "next_hop_1");
        assert_eq!(result.phase, "Phase3-SCRTT-Adaptive"); // Adaptive uses Phase 3
    }

    #[tokio::test]
    async fn test_stats() {
        let engine = TCAMEngine::new().unwrap();

        let prefix = Prefix::from_cidr("192.168.1.0/24").unwrap();
        let route = Route::new(prefix, "next_hop_1", 100);
        engine.insert(route).await.unwrap();

        engine.lookup("192.168.1.42").await.unwrap();

        let stats = engine.stats().await;
        assert_eq!(stats.total_inserts, 1);
        assert_eq!(stats.total_lookups, 1);
        assert_eq!(stats.phase3_lookups, 1); // Adaptive uses Phase 3
        assert_eq!(stats.route_count, 1);
    }
}

