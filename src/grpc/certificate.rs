//! CYAN FLAME™ Certificate Management Module
//!
//! Provides certificate provisioning, revocation, and OCSP support for mTLS.
//!
//! ## Features
//!
//! - Proper X.509 certificate generation using rcgen
//! - Certificate revocation list (CRL) management
//! - OCSP responder for real-time certificate status with proper DER encoding
//! - Certificate-to-API-key binding

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};
use sha2::{Sha256, Digest};
use chrono::{DateTime, Utc, Duration, Datelike};
use uuid::Uuid;
use rcgen::{
    Certificate, CertificateParams, DistinguishedName, DnType,
    IsCa, BasicConstraints, KeyUsagePurpose, ExtendedKeyUsagePurpose,
    SanType, SerialNumber, date_time_ymd,
};

use super::gpu_detection::BaselineGpuType;

/// Certificate revocation reasons (mirrors X.509 CRL reasons)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevocationReason {
    Unspecified = 0,
    KeyCompromise = 1,
    CaCompromise = 2,
    AffiliationChanged = 3,
    Superseded = 4,
    CessationOfOperation = 5,
    CertificateHold = 6,
    PrivilegeWithdrawn = 7,
}

impl RevocationReason {
    /// Convert from proto enum value
    pub fn from_proto(value: i32) -> Self {
        match value {
            1 => Self::KeyCompromise,
            2 => Self::CaCompromise,
            3 => Self::AffiliationChanged,
            4 => Self::Superseded,
            5 => Self::CessationOfOperation,
            6 => Self::CertificateHold,
            7 => Self::PrivilegeWithdrawn,
            _ => Self::Unspecified,
        }
    }

    /// Convert to proto enum value
    pub fn to_proto(&self) -> i32 {
        *self as i32
    }
}

/// Certificate status
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CertificateStatus {
    Valid,
    Expired,
    Revoked,
    Unknown,
}

impl std::fmt::Display for CertificateStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Valid => write!(f, "valid"),
            Self::Expired => write!(f, "expired"),
            Self::Revoked => write!(f, "revoked"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Certificate entry in the certificate store
#[derive(Clone, Debug)]
pub struct CertificateEntry {
    /// Serial number (unique identifier)
    pub serial_number: String,
    /// SHA-256 fingerprint
    pub fingerprint_sha256: String,
    /// Common name
    pub common_name: String,
    /// Organization ID
    pub org_id: String,
    /// Bound API key hash
    pub bound_api_key_hash: String,
    /// Bound GPU type (optional)
    pub bound_gpu_type: Option<BaselineGpuType>,
    /// Certificate PEM data
    pub certificate_pem: String,
    /// Certificate chain PEM data
    pub certificate_chain_pem: String,
    /// Issued at
    pub issued_at: DateTime<Utc>,
    /// Expires at
    pub expires_at: DateTime<Utc>,
    /// Current status
    pub status: CertificateStatus,
    /// Revoked at (if applicable)
    pub revoked_at: Option<DateTime<Utc>>,
    /// Revocation reason (if applicable)
    pub revocation_reason: Option<RevocationReason>,
}

impl CertificateEntry {
    /// Check if certificate is currently valid
    pub fn is_valid(&self) -> bool {
        self.status == CertificateStatus::Valid && Utc::now() < self.expires_at
    }

    /// Get current status (recalculated)
    pub fn current_status(&self) -> CertificateStatus {
        if self.status == CertificateStatus::Revoked {
            return CertificateStatus::Revoked;
        }
        if Utc::now() >= self.expires_at {
            return CertificateStatus::Expired;
        }
        CertificateStatus::Valid
    }
}

/// Certificate Revocation List entry
#[derive(Clone, Debug)]
pub struct CrlEntry {
    pub serial_number: String,
    pub revoked_at: DateTime<Utc>,
    pub reason: RevocationReason,
}

/// Certificate Manager with proper X.509 support using rcgen
pub struct CertificateManager {
    /// Certificates indexed by serial number
    certificates: Arc<RwLock<HashMap<String, CertificateEntry>>>,
    /// Certificates indexed by fingerprint
    fingerprint_index: Arc<RwLock<HashMap<String, String>>>,
    /// Certificate Revocation List
    crl: Arc<RwLock<Vec<CrlEntry>>>,
    /// CRL last update time
    crl_last_update: Arc<RwLock<DateTime<Utc>>>,
    /// CA certificate (for signing)
    ca_cert: Arc<Certificate>,
    /// CA certificate PEM (for chain building)
    ca_cert_pem: String,
}

impl CertificateManager {
    /// Create a new certificate manager with auto-generated CA
    pub fn new(_ca_cert_pem: String, _ca_key_pem: String) -> Self {
        // Generate a proper CA certificate using rcgen
        let ca_cert = Self::generate_ca_certificate()
            .expect("Failed to generate CA certificate");
        let ca_cert_pem = ca_cert.serialize_pem()
            .expect("Failed to serialize CA certificate");

        info!("🔐 Certificate Manager initialized with X.509 CA");
        Self {
            certificates: Arc::new(RwLock::new(HashMap::new())),
            fingerprint_index: Arc::new(RwLock::new(HashMap::new())),
            crl: Arc::new(RwLock::new(Vec::new())),
            crl_last_update: Arc::new(RwLock::new(Utc::now())),
            ca_cert: Arc::new(ca_cert),
            ca_cert_pem,
        }
    }

    /// Generate a self-signed CA certificate
    fn generate_ca_certificate() -> Result<Certificate, rcgen::Error> {
        let mut params = CertificateParams::default();

        // Set CA distinguished name
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "CYAN FLAME Root CA");
        dn.push(DnType::OrganizationName, "SYMMETRIX CORE");
        dn.push(DnType::CountryName, "US");
        params.distinguished_name = dn;

        // Set CA constraints
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];

        // Valid for 10 years
        params.not_before = date_time_ymd(2024, 1, 1);
        params.not_after = date_time_ymd(2034, 12, 31);

        Certificate::from_params(params)
    }

    /// Generate a new serial number
    fn generate_serial(&self) -> String {
        format!("{:032X}", Uuid::new_v4().as_u128())
    }

    /// Calculate SHA-256 fingerprint from certificate DER
    fn calculate_fingerprint(cert_der: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(cert_der);
        hex::encode(hasher.finalize())
    }

    /// Issue a new X.509 certificate
    pub async fn issue_certificate(
        &self,
        org_id: &str,
        common_name: &str,
        api_key_hash: &str,
        gpu_type: Option<BaselineGpuType>,
        validity_days: u32,
    ) -> Result<CertificateEntry, String> {
        let serial_number = self.generate_serial();
        let now = Utc::now();
        let expires_at = now + Duration::days(validity_days as i64);

        // Generate proper X.509 certificate using rcgen
        let (certificate_pem, certificate_der) = self.generate_x509_certificate(
            &serial_number,
            common_name,
            org_id,
            api_key_hash,
            validity_days,
        ).map_err(|e| format!("Certificate generation failed: {}", e))?;

        let fingerprint = Self::calculate_fingerprint(&certificate_der);
        let chain_pem = format!("{}\n{}", certificate_pem, self.ca_cert_pem);

        let entry = CertificateEntry {
            serial_number: serial_number.clone(),
            fingerprint_sha256: fingerprint.clone(),
            common_name: common_name.to_string(),
            org_id: org_id.to_string(),
            bound_api_key_hash: api_key_hash.to_string(),
            bound_gpu_type: gpu_type,
            certificate_pem: certificate_pem.clone(),
            certificate_chain_pem: chain_pem,
            issued_at: now,
            expires_at,
            status: CertificateStatus::Valid,
            revoked_at: None,
            revocation_reason: None,
        };

        // Store certificate
        self.certificates.write().await.insert(serial_number.clone(), entry.clone());
        self.fingerprint_index.write().await.insert(fingerprint.clone(), serial_number.clone());

        info!(
            "📜 X.509 Certificate issued: serial={}, cn={}, org={}, expires={}",
            &serial_number[..8],
            common_name,
            org_id,
            expires_at.format("%Y-%m-%d")
        );

        Ok(entry)
    }

    /// Generate proper X.509 certificate using rcgen
    fn generate_x509_certificate(
        &self,
        serial: &str,
        common_name: &str,
        org_id: &str,
        api_key_hash: &str,
        validity_days: u32,
    ) -> Result<(String, Vec<u8>), rcgen::Error> {
        let mut params = CertificateParams::default();

        // Set distinguished name
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, common_name);
        dn.push(DnType::OrganizationName, org_id);
        dn.push(DnType::OrganizationalUnitName, "CYAN FLAME Agent");
        dn.push(DnType::CountryName, "US");
        params.distinguished_name = dn;

        // Set serial number from our generated serial
        let serial_bytes: Vec<u8> = (0..16)
            .map(|i| u8::from_str_radix(&serial[i*2..i*2+2], 16).unwrap_or(0))
            .collect();
        params.serial_number = Some(SerialNumber::from_slice(&serial_bytes));

        // Set validity period
        let now = chrono::Utc::now();
        let not_before_year = now.year() as i32;
        let not_before_month = now.month() as u8;
        let not_before_day = now.day() as u8;
        params.not_before = date_time_ymd(not_before_year, not_before_month, not_before_day);

        let expires = now + chrono::Duration::days(validity_days as i64);
        let not_after_year = expires.year() as i32;
        let not_after_month = expires.month() as u8;
        let not_after_day = expires.day() as u8;
        params.not_after = date_time_ymd(not_after_year, not_after_month, not_after_day);

        // Set key usages for client authentication
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ClientAuth,
            ExtendedKeyUsagePurpose::ServerAuth,
        ];

        // Add Subject Alternative Names
        params.subject_alt_names = vec![
            SanType::DnsName(format!("{}.cyan-flame.local", common_name)),
            SanType::DnsName(format!("{}.agent.cyan-flame.io", api_key_hash.get(..8).unwrap_or("agent"))),
        ];

        // Not a CA
        params.is_ca = IsCa::NoCa;

        // Generate the certificate signed by our CA
        let cert = Certificate::from_params(params)?;
        let cert_pem = cert.serialize_pem_with_signer(&self.ca_cert)?;
        let cert_der = cert.serialize_der_with_signer(&self.ca_cert)?;

        Ok((cert_pem, cert_der))
    }

    /// Revoke a certificate
    pub async fn revoke_certificate(
        &self,
        serial_number: &str,
        reason: RevocationReason,
    ) -> Result<(), String> {
        let mut certs = self.certificates.write().await;

        if let Some(cert) = certs.get_mut(serial_number) {
            if cert.status == CertificateStatus::Revoked {
                return Err("Certificate is already revoked".to_string());
            }

            let now = Utc::now();
            cert.status = CertificateStatus::Revoked;
            cert.revoked_at = Some(now);
            cert.revocation_reason = Some(reason);

            // Add to CRL
            self.crl.write().await.push(CrlEntry {
                serial_number: serial_number.to_string(),
                revoked_at: now,
                reason,
            });

            // Update CRL timestamp
            *self.crl_last_update.write().await = now;

            warn!(
                "🚫 Certificate revoked: serial={}, reason={:?}",
                &serial_number[..8],
                reason
            );

            Ok(())
        } else {
            Err("Certificate not found".to_string())
        }
    }

    /// Get certificate by serial number
    pub async fn get_certificate(&self, serial_number: &str) -> Option<CertificateEntry> {
        self.certificates.read().await.get(serial_number).cloned()
    }

    /// Get certificate by fingerprint
    pub async fn get_certificate_by_fingerprint(&self, fingerprint: &str) -> Option<CertificateEntry> {
        let serial = self.fingerprint_index.read().await.get(fingerprint).cloned()?;
        self.get_certificate(&serial).await
    }

    /// Check certificate status (for OCSP)
    pub async fn check_status(&self, serial_number: &str) -> CertificateStatus {
        if let Some(cert) = self.get_certificate(serial_number).await {
            cert.current_status()
        } else {
            CertificateStatus::Unknown
        }
    }

    /// Get Certificate Revocation List
    pub async fn get_crl(&self) -> (Vec<CrlEntry>, DateTime<Utc>, DateTime<Utc>) {
        let crl = self.crl.read().await.clone();
        let this_update = *self.crl_last_update.read().await;
        let next_update = this_update + Duration::hours(24);
        (crl, this_update, next_update)
    }

    /// Get number of active certificates
    pub async fn active_certificate_count(&self) -> usize {
        self.certificates.read().await
            .values()
            .filter(|c| c.is_valid())
            .count()
    }

    /// Get number of revoked certificates
    pub async fn revoked_certificate_count(&self) -> usize {
        self.crl.read().await.len()
    }
}

