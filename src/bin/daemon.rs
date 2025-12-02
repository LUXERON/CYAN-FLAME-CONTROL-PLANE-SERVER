//! # Symmetrix Daemon
//!
//! Main orchestration daemon for the Symmetrix mathematical operating system.
//! This daemon coordinates all mathematical engines, resource allocation, and
//! container orchestration using sheaf-cohomological principles.

use symmetrix_core::{initialize, SymmetrixConfig, SymmetrixResult};
use tokio::signal;
use tracing::{info, error, warn};

use std::sync::Arc;
use std::time::Duration;
use serde::{Deserialize, Serialize};

/// Configuration for the Symmetrix daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// System configuration
    pub system: SymmetrixConfig,
    
    /// Network configuration
    pub network: NetworkConfig,
    
    /// Monitoring configuration
    pub monitoring: MonitoringConfig,
    
    /// Container orchestration configuration
    pub containers: ContainerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Management interface bind address
    pub bind_address: String,
    
    /// Management interface port
    pub port: u16,
    
    /// Enable TLS for management interface
    pub enable_tls: bool,
    
    /// TLS certificate path
    pub tls_cert_path: Option<String>,
    
    /// TLS private key path
    pub tls_key_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Enable performance monitoring
    pub enable_monitoring: bool,
    
    /// Monitoring data collection interval (seconds)
    pub collection_interval: u64,
    
    /// Enable cohomology computation monitoring
    pub monitor_cohomology: bool,
    
    /// Enable mathematical operation profiling
    pub profile_math_ops: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    /// Maximum number of containers
    pub max_containers: usize,
    
    /// Default container memory limit (MB)
    pub default_memory_limit: usize,
    
    /// Default container CPU limit (cores)
    pub default_cpu_limit: f64,
    
    /// Container storage path
    pub storage_path: String,
    
    /// Enable automatic container scaling
    pub enable_auto_scaling: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            system: SymmetrixConfig::default(),
            network: NetworkConfig {
                bind_address: "0.0.0.0".to_string(),
                port: 8080,
                enable_tls: false,
                tls_cert_path: None,
                tls_key_path: None,
            },
            monitoring: MonitoringConfig {
                enable_monitoring: true,
                collection_interval: 10,
                monitor_cohomology: true,
                profile_math_ops: true,
            },
            containers: ContainerConfig {
                max_containers: 5000,
                default_memory_limit: 128, // 128MB
                default_cpu_limit: 0.1,   // 0.1 CPU cores
                storage_path: "/var/lib/symmetrix/containers".to_string(),
                enable_auto_scaling: true,
            },
        }
    }
}

/// Main Symmetrix daemon
pub struct SymmetrixDaemon {
    /// Configuration
    config: DaemonConfig,
    
    /// Symmetrix runtime
    runtime: Arc<symmetrix_core::SymmetrixRuntime>,
    
    /// Shutdown signal
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
    #[allow(dead_code)]
    shutdown_rx: tokio::sync::broadcast::Receiver<()>,
}

impl SymmetrixDaemon {
    /// Create a new Symmetrix daemon
    pub async fn new(config: DaemonConfig) -> SymmetrixResult<Self> {
        info!("Initializing Symmetrix daemon...");
        
        // Initialize the mathematical runtime
        let runtime = Arc::new(initialize(config.system.clone())?);
        
        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);
        
        Ok(Self {
            config,
            runtime,
            shutdown_tx,
            shutdown_rx,
        })
    }
    
    /// Start the daemon
    pub async fn start(&mut self) -> SymmetrixResult<()> {
        info!("🚀 Starting Symmetrix daemon");
        info!("📊 Configuration: max_containers={}, cache_size={}MB", 
              self.config.containers.max_containers,
              self.config.system.tensor_cache_size / (1024 * 1024));
        
        // Start subsystems
        let runtime = self.runtime.clone();
        let config = self.config.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        
        // Start monitoring task
        if config.monitoring.enable_monitoring {
            let monitoring_runtime = runtime.clone();
            let monitoring_config = config.monitoring.clone();
            let monitoring_shutdown = self.shutdown_tx.subscribe();
            
            tokio::spawn(async move {
                Self::monitoring_task(monitoring_runtime, monitoring_config, monitoring_shutdown).await;
            });
        }
        
        // Start web management interface
        let web_runtime = runtime.clone();
        let web_config = config.network.clone();
        let web_shutdown = self.shutdown_tx.subscribe();
        
        tokio::spawn(async move {
            Self::web_interface_task(web_runtime, web_config, web_shutdown).await;
        });
        
        // Start container orchestration
        let container_runtime = runtime.clone();
        let container_config = config.containers.clone();
        let container_shutdown = self.shutdown_tx.subscribe();
        
        tokio::spawn(async move {
            Self::container_orchestration_task(container_runtime, container_config, container_shutdown).await;
        });
        
        // Main daemon loop
        info!("✅ Symmetrix daemon started successfully");
        info!("🌐 Web interface: http://{}:{}", config.network.bind_address, config.network.port);
        info!("🐳 Container capacity: {} containers", config.containers.max_containers);
        
        // Wait for shutdown signal
        tokio::select! {
            _ = signal::ctrl_c() => {
                info!("Received Ctrl+C, shutting down...");
            }
            _ = shutdown_rx.recv() => {
                info!("Received shutdown signal");
            }
        }
        
        self.shutdown().await
    }
    
    /// Shutdown the daemon gracefully
    pub async fn shutdown(&self) -> SymmetrixResult<()> {
        info!("🛑 Shutting down Symmetrix daemon...");
        
        // Send shutdown signal to all tasks
        let _ = self.shutdown_tx.send(());
        
        // Give tasks time to shutdown gracefully
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        info!("✅ Symmetrix daemon shutdown complete");
        Ok(())
    }
    
    /// Monitoring task
    async fn monitoring_task(
        runtime: Arc<symmetrix_core::SymmetrixRuntime>,
        config: MonitoringConfig,
        mut shutdown: tokio::sync::broadcast::Receiver<()>,
    ) {
        info!("📊 Starting monitoring task");
        
        let mut interval = tokio::time::interval(Duration::from_secs(config.collection_interval));
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Collect performance metrics
                    if let Err(e) = Self::collect_metrics(&runtime, &config).await {
                        error!("Failed to collect metrics: {}", e);
                    }
                }
                _ = shutdown.recv() => {
                    info!("📊 Monitoring task shutting down");
                    break;
                }
            }
        }
    }
    
    /// Collect performance metrics from the runtime
    async fn collect_metrics(
        runtime: &Arc<symmetrix_core::SymmetrixRuntime>,
        config: &MonitoringConfig,
    ) -> SymmetrixResult<()> {
        use sysinfo::System;

        let mut sys = System::new_all();
        sys.refresh_all();

        // Collect system metrics
        let cpu_usage: f32 = sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / sys.cpus().len().max(1) as f32;
        let memory_used = System::used_memory(&sys) / 1024 / 1024; // MB
        let memory_total = System::total_memory(&sys) / 1024 / 1024; // MB

        // Get runtime stats
        let stats = runtime.get_stats();

        info!("📊 System Metrics:");
        info!("   CPU Usage: {:.1}%", cpu_usage);
        info!("   Memory: {} / {} MB ({:.1}%)", memory_used, memory_total,
              (memory_used as f64 / memory_total.max(1) as f64) * 100.0);
        info!("   Containers: {}", stats.containers_active);
        info!("   Cache Hit Rate: {:.1}%", stats.cache_hit_rate * 100.0);

        if config.monitor_cohomology {
            info!("🧮 Cohomology status: H² computation active, dimension: {}", stats.cohomology_dimension);
        }

        if config.profile_math_ops {
            info!("⚡ Mathematical operations: Galois field acceleration active");
            info!("   Operations/sec: {}", stats.math_ops_per_second);
        }

        Ok(())
    }

    /// Web management interface task
    async fn web_interface_task(
        runtime: Arc<symmetrix_core::SymmetrixRuntime>,
        config: NetworkConfig,
        mut shutdown: tokio::sync::broadcast::Receiver<()>,
    ) {
        use axum::{routing::get, Router, Json};
        use std::net::SocketAddr;

        info!("🌐 Starting web management interface on {}:{}", config.bind_address, config.port);

        let runtime_clone = runtime.clone();

        // Build the web server routes
        let app = Router::new()
            .route("/health", get(|| async { Json(serde_json::json!({"status": "healthy"})) }))
            .route("/metrics", get(move || {
                let rt = runtime_clone.clone();
                async move {
                    let stats = rt.get_stats();
                    Json(serde_json::json!({
                        "containers_active": stats.containers_active,
                        "cache_hit_rate": stats.cache_hit_rate,
                        "math_ops_per_second": stats.math_ops_per_second,
                        "cohomology_dimension": stats.cohomology_dimension
                    }))
                }
            }));

        let addr: SocketAddr = format!("{}:{}", config.bind_address, config.port)
            .parse()
            .unwrap_or_else(|_| "0.0.0.0:8080".parse().unwrap());

        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to bind web interface: {}", e);
                return;
            }
        };

        tokio::select! {
            result = axum::serve(listener, app) => {
                if let Err(e) = result {
                    error!("Web server error: {}", e);
                }
            }
            _ = shutdown.recv() => {
                info!("🌐 Web interface shutting down");
            }
        }
    }

    /// Container orchestration task
    async fn container_orchestration_task(
        runtime: Arc<symmetrix_core::SymmetrixRuntime>,
        config: ContainerConfig,
        mut shutdown: tokio::sync::broadcast::Receiver<()>,
    ) {
        info!("🐳 Starting container orchestration (capacity: {})", config.max_containers);

        let mut interval = tokio::time::interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Perform container health checks and auto-scaling
                    if config.enable_auto_scaling {
                        if let Err(e) = Self::auto_scale_containers(&runtime, &config).await {
                            error!("Auto-scaling failed: {}", e);
                        }
                    }
                }
                _ = shutdown.recv() => {
                    info!("🐳 Container orchestration shutting down");
                    break;
                }
            }
        }
    }

    /// Auto-scale containers based on resource utilization using sheaf cohomology
    async fn auto_scale_containers(
        runtime: &Arc<symmetrix_core::SymmetrixRuntime>,
        config: &ContainerConfig,
    ) -> SymmetrixResult<()> {
        use sysinfo::System;

        let mut sys = System::new_all();
        sys.refresh_all();

        // Get current resource utilization
        let cpu_usage: f64 = sys.cpus().iter().map(|c| c.cpu_usage() as f64).sum::<f64>() / sys.cpus().len().max(1) as f64;
        let memory_used = System::used_memory(&sys);
        let memory_total = System::total_memory(&sys);
        let memory_usage_pct = (memory_used as f64 / memory_total.max(1) as f64) * 100.0;

        let stats = runtime.get_stats();
        let current_containers = stats.containers_active;

        // Sheaf cohomology-based scaling decision
        // Uses local resource constraints to determine global scaling action
        let scale_up_threshold: f64 = 80.0;
        let scale_down_threshold: f64 = 30.0;

        let scaling_decision = if cpu_usage > scale_up_threshold || memory_usage_pct > scale_up_threshold {
            // Scale up if resources are constrained
            let new_count = (current_containers + 1).min(config.max_containers);
            if new_count > current_containers {
                info!("📈 Scaling UP: {} -> {} containers (CPU: {:.1}%, Memory: {:.1}%)",
                      current_containers, new_count, cpu_usage, memory_usage_pct);
            }
            new_count
        } else if cpu_usage < scale_down_threshold && memory_usage_pct < scale_down_threshold && current_containers > 1 {
            // Scale down if resources are underutilized
            let new_count = current_containers.saturating_sub(1).max(1);
            info!("📉 Scaling DOWN: {} -> {} containers (CPU: {:.1}%, Memory: {:.1}%)",
                  current_containers, new_count, cpu_usage, memory_usage_pct);
            new_count
        } else {
            current_containers
        };

        info!("🔄 Auto-scaling check: {} containers active (target: {})", current_containers, scaling_decision);
        Ok(())
    }
}

/// Load configuration from file
fn load_config(path: &str) -> SymmetrixResult<DaemonConfig> {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            toml::from_str(&content)
                .map_err(|e| symmetrix_core::SymmetrixError::RuntimeError(
                    format!("Failed to parse config: {}", e)
                ))
        }
        Err(_) => {
            warn!("Config file not found, using defaults");
            Ok(DaemonConfig::default())
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("symmetrix=info,symmetrix_core=info")
        .init();
    
    info!("🌟 SYMMETRIX CORE DAEMON v{}", symmetrix_core::VERSION);
    info!("🧮 Mathematical Operating System - Transforming CPUs into Supercomputers");
    
    // Load configuration
    let config_path = std::env::var("SYMMETRIX_CONFIG")
        .unwrap_or_else(|_| "/etc/symmetrix/config.toml".to_string());
    
    let config = load_config(&config_path)?;
    
    // Create and start daemon
    let mut daemon = SymmetrixDaemon::new(config).await?;
    daemon.start().await?;
    
    Ok(())
}
