//! CYAN FLAME™ gRPC Server Module
//!
//! Provides gRPC server implementation for the Control Plane.
//! Implements all services defined in the cyan_flame.proto file.

pub mod server;
pub mod calibration;
pub mod telemetry;
pub mod allocation;
pub mod operations;
pub mod auth;
pub mod tls;
pub mod gpu_detection;
pub mod gpu_service;
pub mod certificate;
pub mod certificate_service;
pub mod dashboard_metrics;
pub mod compute_calibration;
pub mod pcie_amplification;

// Re-export generated protobuf types
pub mod proto {
    tonic::include_proto!("cyan_flame.v1");

    /// File descriptor set for gRPC reflection
    pub const FILE_DESCRIPTOR_SET: &[u8] =
        tonic::include_file_descriptor_set!("cyan_flame_descriptor");
}

// Re-export commonly used server types
pub use proto::{
    // Memory Calibration Service Server
    calibration_service_server::{CalibrationService, CalibrationServiceServer},
    // Compute Calibration Service Server
    compute_calibration_service_server::{ComputeCalibrationService, ComputeCalibrationServiceServer},
    // PCIe Amplification Service Server
    pc_ie_amplification_service_server::{PcIeAmplificationService, PcIeAmplificationServiceServer},
    // Telemetry Service Server
    telemetry_service_server::{TelemetryService, TelemetryServiceServer},
    // Allocation Service Server
    allocation_service_server::{AllocationService, AllocationServiceServer},
    // Operations Service Server
    operations_service_server::{OperationsService, OperationsServiceServer},
    // GPU Detection Service Server
    gpu_detection_service_server::{GpuDetectionService, GpuDetectionServiceServer},
    // Certificate Service Server
    certificate_service_server::{CertificateService, CertificateServiceServer},
    // Dashboard Metrics Service Server
    dashboard_metrics_service_server::{DashboardMetricsService, DashboardMetricsServiceServer},
};

// Re-export all message types
pub use proto::*;

/// Default gRPC server port
pub const DEFAULT_GRPC_PORT: u16 = 50051;

/// gRPC server configuration
#[derive(Debug, Clone)]
pub struct GrpcServerConfig {
    /// Server bind address
    pub bind_addr: String,
    /// Enable TLS
    pub enable_tls: bool,
    /// Server TLS certificate path (PEM)
    pub cert_path: Option<String>,
    /// Server TLS private key path (PEM)
    pub key_path: Option<String>,
    /// CA certificate path for mTLS client verification (PEM)
    pub ca_cert_path: Option<String>,
    /// Enable mTLS (mutual TLS) - requires client certificates
    pub enable_mtls: bool,
    /// Maximum concurrent streams per connection
    pub max_concurrent_streams: u32,
    /// Enable gRPC reflection (for debugging tools)
    pub enable_reflection: bool,
}

impl Default for GrpcServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: format!("0.0.0.0:{}", DEFAULT_GRPC_PORT),
            enable_tls: false,
            cert_path: None,
            key_path: None,
            ca_cert_path: None,
            enable_mtls: false,
            max_concurrent_streams: 100,
            enable_reflection: true,
        }
    }
}

impl GrpcServerConfig {
    /// Create production config with TLS (no mTLS)
    pub fn production(cert_path: String, key_path: String) -> Self {
        Self {
            bind_addr: format!("0.0.0.0:{}", DEFAULT_GRPC_PORT),
            enable_tls: true,
            cert_path: Some(cert_path),
            key_path: Some(key_path),
            ca_cert_path: None,
            enable_mtls: false,
            max_concurrent_streams: 1000,
            enable_reflection: false,  // Disable in production
        }
    }

    /// Create production config with mTLS (maximum security)
    pub fn production_mtls(cert_path: String, key_path: String, ca_cert_path: String) -> Self {
        Self {
            bind_addr: format!("0.0.0.0:{}", DEFAULT_GRPC_PORT),
            enable_tls: true,
            cert_path: Some(cert_path),
            key_path: Some(key_path),
            ca_cert_path: Some(ca_cert_path),
            enable_mtls: true,
            max_concurrent_streams: 1000,
            enable_reflection: false,  // Disable in production
        }
    }
}

// Re-export authentication types
pub use auth::{AuthManager, ApiKeyEntry, TierConfig, AuthInterceptor};
pub use tls::{TlsConfiguration, TlsError};

// Re-export GPU detection types
pub use gpu_detection::{
    BaselineGpuType, GpuSpecifications, GpuTierConfig,
    AmplificationTargets, GpuRegistration, GpuDetectionManager
};

// Re-export certificate types
pub use certificate::{CertificateManager, CertificateEntry, RevocationReason};

// Re-export GPU service implementation
pub use gpu_service::GpuDetectionServiceImpl;

// Re-export certificate service implementation
pub use certificate_service::CertificateServiceImpl;

// Re-export dashboard metrics service
pub use dashboard_metrics::{DashboardMetricsServiceImpl, AgentMetrics, SystemMetricsData};

// Re-export compute calibration service
pub use compute_calibration::ComputeCalibrationServiceImpl;

// Re-export PCIe amplification service
pub use pcie_amplification::PCIeAmplificationServiceImpl;
