//! CYAN FLAME™ gRPC TLS Configuration
//!
//! Provides mTLS (mutual TLS) support for secure gRPC communications.
//!
//! ## Security Model
//!
//! CYAN FLAME™ implements two-layer security:
//!
//! 1. **Transport Layer (mTLS)**: Client must present valid certificate to connect
//! 2. **Application Layer (API Key)**: Client must provide valid API key for services
//!
//! This ensures both identity verification AND authorization.

use std::fs;
use tonic::transport::{Certificate, Identity, ServerTlsConfig, ClientTlsConfig};
use tracing::info;

/// TLS Configuration for CYAN FLAME gRPC
#[derive(Clone, Debug)]
pub struct TlsConfiguration {
    /// Server certificate path (PEM format)
    pub server_cert_path: Option<String>,
    /// Server private key path (PEM format)
    pub server_key_path: Option<String>,
    /// CA certificate path for verifying client certificates (PEM format)
    pub ca_cert_path: Option<String>,
    /// Client certificate path for mTLS (PEM format)
    pub client_cert_path: Option<String>,
    /// Client private key path for mTLS (PEM format)
    pub client_key_path: Option<String>,
    /// Enable mTLS (require client certificates)
    pub enable_mtls: bool,
    /// Domain name for TLS verification
    pub domain: String,
}

impl Default for TlsConfiguration {
    fn default() -> Self {
        Self {
            server_cert_path: None,
            server_key_path: None,
            ca_cert_path: None,
            client_cert_path: None,
            client_key_path: None,
            enable_mtls: false,
            domain: "localhost".to_string(),
        }
    }
}

impl TlsConfiguration {
    /// Create new TLS configuration for server
    pub fn new_server(
        server_cert_path: String,
        server_key_path: String,
        ca_cert_path: Option<String>,
        enable_mtls: bool,
    ) -> Self {
        Self {
            server_cert_path: Some(server_cert_path),
            server_key_path: Some(server_key_path),
            ca_cert_path,
            client_cert_path: None,
            client_key_path: None,
            enable_mtls,
            domain: "localhost".to_string(),
        }
    }

    /// Create new TLS configuration for client
    pub fn new_client(
        ca_cert_path: String,
        client_cert_path: Option<String>,
        client_key_path: Option<String>,
        domain: String,
    ) -> Self {
        let enable_mtls = client_cert_path.is_some() && client_key_path.is_some();
        Self {
            server_cert_path: None,
            server_key_path: None,
            ca_cert_path: Some(ca_cert_path),
            client_cert_path,
            client_key_path,
            enable_mtls,
            domain,
        }
    }

    /// Check if server TLS is enabled
    pub fn is_server_tls_enabled(&self) -> bool {
        self.server_cert_path.is_some() && self.server_key_path.is_some()
    }

    /// Check if client TLS is enabled
    pub fn is_client_tls_enabled(&self) -> bool {
        self.ca_cert_path.is_some()
    }

    /// Build server TLS configuration
    pub fn build_server_config(&self) -> Result<Option<ServerTlsConfig>, TlsError> {
        if !self.is_server_tls_enabled() {
            info!("🔓 TLS disabled - using insecure connection");
            return Ok(None);
        }

        let cert_path = self.server_cert_path.as_ref().unwrap();
        let key_path = self.server_key_path.as_ref().unwrap();

        // Read server certificate and key
        let cert_pem = fs::read_to_string(cert_path)
            .map_err(|e| TlsError::CertificateRead(cert_path.clone(), e.to_string()))?;
        let key_pem = fs::read_to_string(key_path)
            .map_err(|e| TlsError::KeyRead(key_path.clone(), e.to_string()))?;

        let identity = Identity::from_pem(&cert_pem, &key_pem);
        let mut tls_config = ServerTlsConfig::new().identity(identity);

        // Configure mTLS if enabled
        if self.enable_mtls {
            if let Some(ca_path) = &self.ca_cert_path {
                let ca_pem = fs::read_to_string(ca_path)
                    .map_err(|e| TlsError::CaRead(ca_path.clone(), e.to_string()))?;
                let ca_cert = Certificate::from_pem(&ca_pem);
                tls_config = tls_config.client_ca_root(ca_cert);
                info!("🔐 mTLS ENABLED - Client certificates REQUIRED for connection");
            } else {
                return Err(TlsError::MtlsNoCa);
            }
        }

        info!("🔒 TLS enabled for gRPC server");
        Ok(Some(tls_config))
    }

    /// Build client TLS configuration
    pub fn build_client_config(&self) -> Result<Option<ClientTlsConfig>, TlsError> {
        if !self.is_client_tls_enabled() {
            return Ok(None);
        }

        let mut tls_config = ClientTlsConfig::new().domain_name(&self.domain);

        // Add CA certificate to verify server
        if let Some(ca_path) = &self.ca_cert_path {
            let ca_pem = fs::read_to_string(ca_path)
                .map_err(|e| TlsError::CaRead(ca_path.clone(), e.to_string()))?;
            let ca_cert = Certificate::from_pem(&ca_pem);
            tls_config = tls_config.ca_certificate(ca_cert);
            info!("🔒 Client TLS: CA certificate loaded for server verification");
        }

        // Add client identity for mTLS
        if self.enable_mtls {
            let cert_path = self.client_cert_path.as_ref()
                .ok_or(TlsError::MtlsNoClientCert)?;
            let key_path = self.client_key_path.as_ref()
                .ok_or(TlsError::MtlsNoClientKey)?;

            let cert_pem = fs::read_to_string(cert_path)
                .map_err(|e| TlsError::CertificateRead(cert_path.clone(), e.to_string()))?;
            let key_pem = fs::read_to_string(key_path)
                .map_err(|e| TlsError::KeyRead(key_path.clone(), e.to_string()))?;

            let identity = Identity::from_pem(&cert_pem, &key_pem);
            tls_config = tls_config.identity(identity);
            info!("🔐 mTLS: Client certificate loaded for mutual authentication");
        }

        Ok(Some(tls_config))
    }
}

/// TLS-related errors
#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("Failed to read certificate from {0}: {1}")]
    CertificateRead(String, String),
    #[error("Failed to read key from {0}: {1}")]
    KeyRead(String, String),
    #[error("Failed to read CA certificate from {0}: {1}")]
    CaRead(String, String),
    #[error("mTLS enabled but no CA certificate provided")]
    MtlsNoCa,
    #[error("mTLS enabled but no client certificate provided")]
    MtlsNoClientCert,
    #[error("mTLS enabled but no client key provided")]
    MtlsNoClientKey,
    #[error("TLS configuration error: {0}")]
    Configuration(String),
}

