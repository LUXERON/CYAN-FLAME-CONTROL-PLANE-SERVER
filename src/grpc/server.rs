//! CYAN FLAME™ gRPC Server
//!
//! Main server implementation that combines all gRPC services.
//! Includes authentication interceptor for API key validation and mTLS support.

use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Server;
use tracing::{info, warn, error};

use super::{
    GrpcServerConfig,
    CalibrationServiceServer,
    ComputeCalibrationServiceServer,
    PcIeAmplificationServiceServer,
    TelemetryServiceServer,
    AllocationServiceServer,
    OperationsServiceServer,
    GpuDetectionServiceServer,
    CertificateServiceServer,
    DashboardMetricsServiceServer,
    AuthManager,
    AuthInterceptor,
    TlsConfiguration,
};
use super::calibration::CalibrationServiceImpl;
use super::compute_calibration::ComputeCalibrationServiceImpl;
use super::pcie_amplification::PCIeAmplificationServiceImpl;
use super::telemetry::TelemetryServiceImpl;
use super::allocation::AllocationServiceImpl;
use super::operations::OperationsServiceImpl;
use super::gpu_service::GpuDetectionServiceImpl;
use super::certificate_service::CertificateServiceImpl;
use super::dashboard_metrics::DashboardMetricsServiceImpl;

/// CYAN FLAME gRPC Server
pub struct CyanFlameGrpcServer {
    config: GrpcServerConfig,
    calibration_service: Arc<CalibrationServiceImpl>,
    compute_calibration_service: Arc<ComputeCalibrationServiceImpl>,
    pcie_amplification_service: Arc<PCIeAmplificationServiceImpl>,
    telemetry_service: TelemetryServiceImpl,
    allocation_service: AllocationServiceImpl,
    operations_service: OperationsServiceImpl,
    gpu_detection_service: GpuDetectionServiceImpl,
    certificate_service: CertificateServiceImpl,
    dashboard_metrics_service: DashboardMetricsServiceImpl,
    auth_manager: Arc<AuthManager>,
}

impl CyanFlameGrpcServer {
    /// Create new gRPC server with default configuration (auth DISABLED for testing)
    pub fn new() -> Self {
        Self::with_config(GrpcServerConfig::default())
    }

    /// Create new gRPC server with authentication enabled
    pub fn new_with_auth() -> Self {
        Self::with_config_and_auth(GrpcServerConfig::default(), true)
    }

    /// Create new gRPC server with custom configuration (auth DISABLED)
    pub fn with_config(config: GrpcServerConfig) -> Self {
        Self::with_config_and_auth(config, false)
    }

    /// Create new gRPC server with custom configuration and auth setting
    pub fn with_config_and_auth(config: GrpcServerConfig, auth_enabled: bool) -> Self {
        let calibration_service = Arc::new(CalibrationServiceImpl::new());
        let compute_calibration_service = Arc::new(ComputeCalibrationServiceImpl::new());
        let pcie_amplification_service = Arc::new(PCIeAmplificationServiceImpl::new());
        let auth_manager = Arc::new(AuthManager::new(auth_enabled));

        Self {
            config,
            calibration_service,
            compute_calibration_service,
            pcie_amplification_service,
            telemetry_service: TelemetryServiceImpl::new(),
            allocation_service: AllocationServiceImpl::new(),
            operations_service: OperationsServiceImpl::new(),
            gpu_detection_service: GpuDetectionServiceImpl::new(),
            certificate_service: CertificateServiceImpl::new(),
            dashboard_metrics_service: DashboardMetricsServiceImpl::new(),
            auth_manager,
        }
    }

    /// Get a reference to the auth manager
    pub fn auth_manager(&self) -> Arc<AuthManager> {
        self.auth_manager.clone()
    }

    /// Start the gRPC server
    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error>> {
        let addr: SocketAddr = self.config.bind_addr.parse()?;
        let auth_enabled = self.auth_manager.is_auth_enabled();
        let mtls_enabled = self.config.enable_mtls;

        info!("╔══════════════════════════════════════════════════════════════════╗");
        info!("║           CYAN FLAME™ gRPC Control Plane Server                  ║");
        info!("╠══════════════════════════════════════════════════════════════════╣");
        info!("║  Address:    {:50} ║", addr);
        info!("║  TLS:        {:50} ║", if self.config.enable_tls { "🔒 ENABLED" } else { "🔓 DISABLED" });
        info!("║  mTLS:       {:50} ║", if mtls_enabled { "🔐 ENABLED (client certs required)" } else { "🔓 DISABLED" });
        info!("║  API Auth:   {:50} ║", if auth_enabled { "🔐 ENABLED (API keys required)" } else { "🔓 DISABLED" });
        info!("║  Reflection: {:50} ║", if self.config.enable_reflection { "Enabled" } else { "Disabled" });
        info!("╚══════════════════════════════════════════════════════════════════╝");

        // Show security layer summary
        if mtls_enabled && auth_enabled {
            info!("╔══════════════════════════════════════════════════════════════════╗");
            info!("║                  🛡️  TWO-LAYER SECURITY ACTIVE                   ║");
            info!("╠══════════════════════════════════════════════════════════════════╣");
            info!("║  Layer 1 (Transport): mTLS - Client certificate required         ║");
            info!("║  Layer 2 (Application): API Key - Valid key required             ║");
            info!("║                                                                  ║");
            info!("║  WITHOUT valid certificate → Cannot connect at all               ║");
            info!("║  WITHOUT valid API key → Cannot access services                  ║");
            info!("╚══════════════════════════════════════════════════════════════════╝");
        }

        // Register default API keys if auth is enabled
        if auth_enabled {
            self.auth_manager.register_default_keys().await;
            info!("╔══════════════════════════════════════════════════════════════════╗");
            info!("║                    TIER CONFIGURATION                            ║");
            info!("╠══════════════════════════════════════════════════════════════════╣");
            info!("║  Free:       100× amplification, 2 TB max, 100 req/min           ║");
            info!("║  Starter:    1,000× amplification, 24 TB max, 1,000 req/min      ║");
            info!("║  Pro:        10,000× amplification, 240 TB max, 10,000 req/min   ║");
            info!("║  Enterprise: 24,500× amplification, 574 TB max, UNLIMITED        ║");
            info!("╚══════════════════════════════════════════════════════════════════╝");
        }

        // Start calibration matrix rotation tasks
        let calibration_clone = self.calibration_service.clone();
        calibration_clone.start_rotation_task();

        let compute_calibration_clone = self.compute_calibration_service.clone();
        compute_calibration_clone.start_rotation_task();

        let pcie_amplification_clone = self.pcie_amplification_service.clone();
        pcie_amplification_clone.start_rotation_task();

        // Create auth interceptor
        let auth_interceptor = AuthInterceptor::new(self.auth_manager.clone());

        // Build TLS configuration if enabled
        let tls_config = if self.config.enable_tls {
            let tls = TlsConfiguration::new_server(
                self.config.cert_path.clone().unwrap_or_default(),
                self.config.key_path.clone().unwrap_or_default(),
                self.config.ca_cert_path.clone(),
                self.config.enable_mtls,
            );
            match tls.build_server_config() {
                Ok(config) => config,
                Err(e) => {
                    error!("❌ Failed to build TLS configuration: {}", e);
                    return Err(Box::new(e));
                }
            }
        } else {
            None
        };

        // Build the server
        let mut builder = if let Some(tls) = tls_config {
            info!("🔒 TLS/mTLS configured for gRPC server");
            Server::builder()
                .tls_config(tls)?
                .max_concurrent_streams(self.config.max_concurrent_streams)
        } else {
            Server::builder()
                .max_concurrent_streams(self.config.max_concurrent_streams)
        };

        // Add gRPC reflection if enabled
        let reflection_service = if self.config.enable_reflection {
            Some(
                tonic_reflection::server::Builder::configure()
                    .register_encoded_file_descriptor_set(super::proto::FILE_DESCRIPTOR_SET)
                    .build_v1()?
            )
        } else {
            None
        };

        // Create services with auth interceptor
        let calibration_svc = CalibrationServiceServer::with_interceptor(
            CalibrationServiceImpl::new(),
            auth_interceptor.clone()
        );
        let compute_calibration_svc = ComputeCalibrationServiceServer::with_interceptor(
            ComputeCalibrationServiceImpl::new(),
            auth_interceptor.clone()
        );
        let pcie_amplification_svc = PcIeAmplificationServiceServer::with_interceptor(
            PCIeAmplificationServiceImpl::new(),
            auth_interceptor.clone()
        );
        let telemetry_svc = TelemetryServiceServer::with_interceptor(
            self.telemetry_service,
            auth_interceptor.clone()
        );
        let allocation_svc = AllocationServiceServer::with_interceptor(
            self.allocation_service,
            auth_interceptor.clone()
        );
        let operations_svc = OperationsServiceServer::with_interceptor(
            self.operations_service,
            auth_interceptor.clone()
        );
        let gpu_detection_svc = GpuDetectionServiceServer::with_interceptor(
            self.gpu_detection_service,
            auth_interceptor.clone()
        );
        let certificate_svc = CertificateServiceServer::with_interceptor(
            self.certificate_service,
            auth_interceptor.clone()
        );
        let dashboard_metrics_svc = DashboardMetricsServiceServer::with_interceptor(
            self.dashboard_metrics_service,
            auth_interceptor.clone()
        );

        // Create service router
        let router = builder
            .add_service(calibration_svc)
            .add_service(compute_calibration_svc)
            .add_service(pcie_amplification_svc)
            .add_service(telemetry_svc)
            .add_service(allocation_svc)
            .add_service(operations_svc)
            .add_service(gpu_detection_svc)
            .add_service(certificate_svc)
            .add_service(dashboard_metrics_svc);

        // Add reflection service if enabled (no auth for reflection)
        let router = if let Some(reflection) = reflection_service {
            router.add_service(reflection)
        } else {
            router
        };

        info!("🔥 CYAN FLAME gRPC server starting on {}", addr);
        if auth_enabled {
            info!("🔐 All requests require valid API key in 'x-api-key' header");
        }

        router.serve(addr).await?;

        Ok(())
    }
}

impl Default for CyanFlameGrpcServer {
    fn default() -> Self {
        Self::new()
    }
}

