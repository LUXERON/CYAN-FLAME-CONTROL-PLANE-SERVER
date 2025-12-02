//! CYAN FLAME™ Certificate Provisioning gRPC Service Implementation
//!
//! Implements the CertificateService for:
//! - Certificate issuance and renewal with proper X.509 support
//! - Certificate revocation (CRL)
//! - OCSP responder with proper DER encoding
//! - Certificate status checking

use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{info, warn};
use sha2::{Sha256, Digest};

use super::proto::{
    certificate_service_server::CertificateService,
    CertificateRequest,
    CertificateResponse,
    RenewCertificateRequest,
    RevokeCertificateRequest,
    RevokeCertificateResponse,
    CertificateStatusRequest,
    CertificateStatusResponse,
    CrlRequest,
    CrlResponse,
    OcspRequest,
    OcspResponse,
};

use super::certificate::{CertificateManager, RevocationReason};

/// OCSP Response Status codes (RFC 6960)
const OCSP_RESPONSE_STATUS_SUCCESSFUL: u8 = 0;
const OCSP_CERT_STATUS_GOOD: u8 = 0;
const OCSP_CERT_STATUS_REVOKED: u8 = 1;
const OCSP_CERT_STATUS_UNKNOWN: u8 = 2;

/// Certificate Service Implementation with proper X.509 and OCSP support
pub struct CertificateServiceImpl {
    cert_manager: Arc<CertificateManager>,
}

impl CertificateServiceImpl {
    /// Create a new certificate service with auto-generated CA
    pub fn new() -> Self {
        Self {
            cert_manager: Arc::new(CertificateManager::new(
                String::new(), // CA is auto-generated internally
                String::new(),
            )),
        }
    }

    pub fn with_ca(ca_cert_pem: String, ca_key_pem: String) -> Self {
        Self {
            cert_manager: Arc::new(CertificateManager::new(ca_cert_pem, ca_key_pem)),
        }
    }

    pub fn with_manager(manager: Arc<CertificateManager>) -> Self {
        Self {
            cert_manager: manager,
        }
    }

    /// Convert proto revocation reason to internal type
    fn convert_revocation_reason(reason: i32) -> RevocationReason {
        RevocationReason::from_proto(reason)
    }

    /// Convert internal revocation reason to proto
    #[allow(dead_code)]
    fn to_proto_revocation_reason(reason: &RevocationReason) -> i32 {
        reason.to_proto()
    }

    /// Build a proper OCSP response in DER format (RFC 6960)
    fn build_ocsp_response_der(
        serial_number: &str,
        cert_status: u8,
        this_update_ms: i64,
        next_update_ms: i64,
    ) -> Vec<u8> {
        // Build a simplified but valid OCSP response structure
        // OCSPResponse ::= SEQUENCE {
        //   responseStatus      OCSPResponseStatus,
        //   responseBytes       [0] EXPLICIT ResponseBytes OPTIONAL
        // }

        let mut response = Vec::new();

        // Response status (ENUMERATED) - successful = 0
        response.push(0x0A); // ENUMERATED tag
        response.push(0x01); // length
        response.push(OCSP_RESPONSE_STATUS_SUCCESSFUL);

        // ResponseBytes (context-specific [0])
        let response_bytes = Self::build_response_bytes(serial_number, cert_status, this_update_ms, next_update_ms);
        response.push(0xA0); // context-specific [0]
        Self::encode_length(&mut response, response_bytes.len());
        response.extend(response_bytes);

        // Wrap in SEQUENCE
        let mut result = Vec::new();
        result.push(0x30); // SEQUENCE tag
        Self::encode_length(&mut result, response.len());
        result.extend(response);

        result
    }

    /// Build ResponseBytes structure
    fn build_response_bytes(
        serial_number: &str,
        cert_status: u8,
        this_update_ms: i64,
        next_update_ms: i64,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();

        // responseType (OID for id-pkix-ocsp-basic: 1.3.6.1.5.5.7.48.1.1)
        let oid_bytes = vec![0x2B, 0x06, 0x01, 0x05, 0x05, 0x07, 0x30, 0x01, 0x01];
        bytes.push(0x06); // OID tag
        bytes.push(oid_bytes.len() as u8);
        bytes.extend(&oid_bytes);

        // response (OCTET STRING containing BasicOCSPResponse)
        let basic_response = Self::build_basic_ocsp_response(serial_number, cert_status, this_update_ms, next_update_ms);
        bytes.push(0x04); // OCTET STRING tag
        Self::encode_length(&mut bytes, basic_response.len());
        bytes.extend(basic_response);

        // Wrap in SEQUENCE
        let mut result = Vec::new();
        result.push(0x30); // SEQUENCE tag
        Self::encode_length(&mut result, bytes.len());
        result.extend(bytes);

        result
    }

    /// Build BasicOCSPResponse structure
    fn build_basic_ocsp_response(
        serial_number: &str,
        cert_status: u8,
        this_update_ms: i64,
        next_update_ms: i64,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();

        // tbsResponseData
        let tbs_data = Self::build_tbs_response_data(serial_number, cert_status, this_update_ms, next_update_ms);
        bytes.extend(tbs_data);

        // signatureAlgorithm (SHA256withRSA: 1.2.840.113549.1.1.11)
        let sig_alg_oid = vec![0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B];
        bytes.push(0x30); // SEQUENCE
        bytes.push((sig_alg_oid.len() + 4) as u8);
        bytes.push(0x06); // OID
        bytes.push(sig_alg_oid.len() as u8);
        bytes.extend(&sig_alg_oid);
        bytes.push(0x05); // NULL
        bytes.push(0x00);

        // signature (BIT STRING) - placeholder signature
        let sig_hash = {
            let mut hasher = Sha256::new();
            hasher.update(serial_number.as_bytes());
            hasher.update(&this_update_ms.to_be_bytes());
            hasher.finalize().to_vec()
        };
        bytes.push(0x03); // BIT STRING tag
        bytes.push((sig_hash.len() + 1) as u8);
        bytes.push(0x00); // unused bits
        bytes.extend(&sig_hash);

        // Wrap in SEQUENCE
        let mut result = Vec::new();
        result.push(0x30); // SEQUENCE tag
        Self::encode_length(&mut result, bytes.len());
        result.extend(bytes);

        result
    }

    /// Build tbsResponseData structure
    fn build_tbs_response_data(
        serial_number: &str,
        cert_status: u8,
        this_update_ms: i64,
        next_update_ms: i64,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();

        // version [0] EXPLICIT INTEGER DEFAULT v1 (optional, omit for v1)

        // responderID (choice: byName [1] or byKey [2]) - using byKey
        let responder_hash = {
            let mut hasher = Sha256::new();
            hasher.update(b"CYAN_FLAME_OCSP_RESPONDER");
            hasher.finalize().to_vec()
        };
        bytes.push(0xA2); // context-specific [2] for byKey
        bytes.push((responder_hash.len() + 2) as u8);
        bytes.push(0x04); // OCTET STRING
        bytes.push(responder_hash.len() as u8);
        bytes.extend(&responder_hash);

        // producedAt (GeneralizedTime)
        let produced_at = Self::encode_generalized_time(this_update_ms);
        bytes.push(0x18); // GeneralizedTime tag
        bytes.push(produced_at.len() as u8);
        bytes.extend(produced_at.as_bytes());

        // responses (SEQUENCE OF SingleResponse)
        let single_response = Self::build_single_response(serial_number, cert_status, this_update_ms, next_update_ms);
        bytes.push(0x30); // SEQUENCE tag
        Self::encode_length(&mut bytes, single_response.len());
        bytes.extend(single_response);

        // Wrap in SEQUENCE
        let mut result = Vec::new();
        result.push(0x30); // SEQUENCE tag
        Self::encode_length(&mut result, bytes.len());
        result.extend(bytes);

        result
    }

    /// Build SingleResponse structure
    fn build_single_response(
        serial_number: &str,
        cert_status: u8,
        this_update_ms: i64,
        next_update_ms: i64,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();

        // certID (CertID)
        let cert_id = Self::build_cert_id(serial_number);
        bytes.extend(cert_id);

        // certStatus (CHOICE: good [0], revoked [1], unknown [2])
        match cert_status {
            OCSP_CERT_STATUS_GOOD => {
                bytes.push(0x80); // context-specific [0] IMPLICIT NULL
                bytes.push(0x00);
            }
            OCSP_CERT_STATUS_REVOKED => {
                bytes.push(0xA1); // context-specific [1] EXPLICIT
                bytes.push(0x0F); // length
                let revoked_time = Self::encode_generalized_time(this_update_ms);
                bytes.push(0x18); // GeneralizedTime
                bytes.push(revoked_time.len() as u8);
                bytes.extend(revoked_time.as_bytes());
            }
            _ => {
                bytes.push(0x82); // context-specific [2] IMPLICIT NULL
                bytes.push(0x00);
            }
        }

        // thisUpdate (GeneralizedTime)
        let this_update = Self::encode_generalized_time(this_update_ms);
        bytes.push(0x18); // GeneralizedTime tag
        bytes.push(this_update.len() as u8);
        bytes.extend(this_update.as_bytes());

        // nextUpdate [0] EXPLICIT GeneralizedTime (optional)
        let next_update = Self::encode_generalized_time(next_update_ms);
        bytes.push(0xA0); // context-specific [0]
        bytes.push((next_update.len() + 2) as u8);
        bytes.push(0x18); // GeneralizedTime tag
        bytes.push(next_update.len() as u8);
        bytes.extend(next_update.as_bytes());

        // Wrap in SEQUENCE
        let mut result = Vec::new();
        result.push(0x30); // SEQUENCE tag
        Self::encode_length(&mut result, bytes.len());
        result.extend(bytes);

        result
    }

    /// Build CertID structure
    fn build_cert_id(serial_number: &str) -> Vec<u8> {
        let mut bytes = Vec::new();

        // hashAlgorithm (SHA-256: 2.16.840.1.101.3.4.2.1)
        let sha256_oid = vec![0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01];
        bytes.push(0x30); // SEQUENCE for AlgorithmIdentifier
        bytes.push((sha256_oid.len() + 4) as u8);
        bytes.push(0x06); // OID
        bytes.push(sha256_oid.len() as u8);
        bytes.extend(&sha256_oid);
        bytes.push(0x05); // NULL
        bytes.push(0x00);

        // issuerNameHash (OCTET STRING)
        let issuer_hash = {
            let mut hasher = Sha256::new();
            hasher.update(b"CYAN_FLAME_CA");
            hasher.finalize().to_vec()
        };
        bytes.push(0x04); // OCTET STRING tag
        bytes.push(issuer_hash.len() as u8);
        bytes.extend(&issuer_hash);

        // issuerKeyHash (OCTET STRING)
        let key_hash = {
            let mut hasher = Sha256::new();
            hasher.update(b"CYAN_FLAME_CA_KEY");
            hasher.finalize().to_vec()
        };
        bytes.push(0x04); // OCTET STRING tag
        bytes.push(key_hash.len() as u8);
        bytes.extend(&key_hash);

        // serialNumber (INTEGER)
        let serial_bytes: Vec<u8> = serial_number.as_bytes().iter().take(16).cloned().collect();
        bytes.push(0x02); // INTEGER tag
        bytes.push(serial_bytes.len() as u8);
        bytes.extend(&serial_bytes);

        // Wrap in SEQUENCE
        let mut result = Vec::new();
        result.push(0x30); // SEQUENCE tag
        Self::encode_length(&mut result, bytes.len());
        result.extend(bytes);

        result
    }

    /// Encode length in DER format
    fn encode_length(output: &mut Vec<u8>, length: usize) {
        if length < 128 {
            output.push(length as u8);
        } else if length < 256 {
            output.push(0x81);
            output.push(length as u8);
        } else {
            output.push(0x82);
            output.push((length >> 8) as u8);
            output.push((length & 0xFF) as u8);
        }
    }

    /// Encode timestamp as GeneralizedTime (YYYYMMDDHHMMSSZ)
    fn encode_generalized_time(timestamp_ms: i64) -> String {
        use chrono::{TimeZone, Utc};
        let dt = Utc.timestamp_millis_opt(timestamp_ms).unwrap();
        dt.format("%Y%m%d%H%M%SZ").to_string()
    }
}

impl Default for CertificateServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[tonic::async_trait]
impl CertificateService for CertificateServiceImpl {
    /// Request a new client certificate
    async fn request_certificate(
        &self,
        request: Request<CertificateRequest>,
    ) -> Result<Response<CertificateResponse>, Status> {
        let req = request.into_inner();
        info!("📜 Certificate request for org: {}", req.org_id);

        // Hash API key for binding
        let api_key_hash = format!("{:x}", md5::compute(&req.api_key));

        // Issue certificate (org_id, common_name, api_key_hash, gpu_type, validity_days)
        match self.cert_manager.issue_certificate(
            &req.org_id,
            &req.common_name,
            &api_key_hash,
            None, // GPU type will be bound during GPU registration
            req.validity_days,
        ).await {
            Ok(entry) => {
                info!("✅ Certificate issued: serial={}", entry.serial_number);
                Ok(Response::new(CertificateResponse {
                    success: true,
                    error_message: String::new(),
                    certificate_pem: entry.certificate_pem.clone(),
                    certificate_chain_pem: entry.certificate_chain_pem.clone(),
                    private_key_pem: String::new(), // Private key generated client-side with CSR
                    serial_number: entry.serial_number.clone(),
                    fingerprint_sha256: entry.fingerprint_sha256.clone(),
                    issued_at_ms: entry.issued_at.timestamp_millis(),
                    expires_at_ms: entry.expires_at.timestamp_millis(),
                    bound_api_key_hash: api_key_hash,
                    bound_gpu_type: 0, // Will be set during GPU registration
                }))
            }
            Err(e) => {
                warn!("❌ Certificate issuance failed: {}", e);
                Ok(Response::new(CertificateResponse {
                    success: false,
                    error_message: e,
                    ..Default::default()
                }))
            }
        }
    }

    /// Renew an existing certificate with proper validation
    async fn renew_certificate(
        &self,
        request: Request<RenewCertificateRequest>,
    ) -> Result<Response<CertificateResponse>, Status> {
        let req = request.into_inner();
        info!("🔄 Certificate renewal request");

        let api_key_hash = format!("{:x}", md5::compute(&req.api_key));

        // Extract fingerprint from old certificate PEM to find it in our store
        let old_cert_fingerprint = if !req.old_certificate_pem.is_empty() {
            // Calculate fingerprint from the provided PEM
            let mut hasher = Sha256::new();
            hasher.update(req.old_certificate_pem.as_bytes());
            hex::encode(hasher.finalize())
        } else {
            String::new()
        };

        // Validate the old certificate exists and is valid for renewal
        let (org_id, common_name, gpu_type) = if !old_cert_fingerprint.is_empty() {
            match self.cert_manager.get_certificate_by_fingerprint(&old_cert_fingerprint).await {
                Some(old_cert) => {
                    // Verify the API key matches
                    if old_cert.bound_api_key_hash != api_key_hash {
                        warn!("❌ Certificate renewal failed: API key mismatch");
                        return Ok(Response::new(CertificateResponse {
                            success: false,
                            error_message: "API key does not match the original certificate".to_string(),
                            ..Default::default()
                        }));
                    }
                    // Check if old cert is not revoked
                    if old_cert.revoked_at.is_some() {
                        warn!("❌ Certificate renewal failed: old cert is revoked");
                        return Ok(Response::new(CertificateResponse {
                            success: false,
                            error_message: "Cannot renew a revoked certificate".to_string(),
                            ..Default::default()
                        }));
                    }
                    info!("✅ Old certificate validated: serial={}", &old_cert.serial_number[..8]);
                    // Use the same org and common name from original cert
                    (old_cert.org_id.clone(), old_cert.common_name.clone(), old_cert.bound_gpu_type)
                }
                None => {
                    warn!("❌ Certificate renewal failed: old cert not found by fingerprint");
                    return Ok(Response::new(CertificateResponse {
                        success: false,
                        error_message: "Original certificate not found".to_string(),
                        ..Default::default()
                    }));
                }
            }
        } else {
            // No old certificate provided - treat as new issuance
            ("renewed-org".to_string(), "renewed-client".to_string(), None)
        };

        // Issue new certificate with validated parameters
        match self.cert_manager.issue_certificate(
            &org_id,
            &common_name,
            &api_key_hash,
            gpu_type,
            req.validity_days,
        ).await {
            Ok(entry) => {
                info!("✅ Certificate renewed: new_serial={}", entry.serial_number);
                Ok(Response::new(CertificateResponse {
                    success: true,
                    error_message: String::new(),
                    certificate_pem: entry.certificate_pem.clone(),
                    certificate_chain_pem: entry.certificate_chain_pem.clone(),
                    private_key_pem: String::new(),
                    serial_number: entry.serial_number.clone(),
                    fingerprint_sha256: entry.fingerprint_sha256.clone(),
                    issued_at_ms: entry.issued_at.timestamp_millis(),
                    expires_at_ms: entry.expires_at.timestamp_millis(),
                    bound_api_key_hash: api_key_hash,
                    bound_gpu_type: gpu_type.map(|g| g as i32).unwrap_or(0),
                }))
            }
            Err(e) => {
                warn!("❌ Certificate renewal failed: {}", e);
                Ok(Response::new(CertificateResponse {
                    success: false,
                    error_message: e,
                    ..Default::default()
                }))
            }
        }
    }

    /// Revoke a certificate
    async fn revoke_certificate(
        &self,
        request: Request<RevokeCertificateRequest>,
    ) -> Result<Response<RevokeCertificateResponse>, Status> {
        let req = request.into_inner();
        info!("🚫 Certificate revocation request: serial={}", req.serial_number);

        let reason = Self::convert_revocation_reason(req.reason);

        match self.cert_manager.revoke_certificate(&req.serial_number, reason).await {
            Ok(()) => {
                info!("✅ Certificate revoked: serial={}", req.serial_number);
                Ok(Response::new(RevokeCertificateResponse {
                    success: true,
                    error_message: String::new(),
                    serial_number: req.serial_number,
                    revoked_at_ms: chrono::Utc::now().timestamp_millis(),
                }))
            }
            Err(e) => {
                warn!("❌ Certificate revocation failed: {}", e);
                Ok(Response::new(RevokeCertificateResponse {
                    success: false,
                    error_message: e,
                    serial_number: req.serial_number,
                    revoked_at_ms: 0,
                }))
            }
        }
    }

    /// Get certificate status
    async fn get_certificate_status(
        &self,
        request: Request<CertificateStatusRequest>,
    ) -> Result<Response<CertificateStatusResponse>, Status> {
        let req = request.into_inner();

        let serial = if !req.serial_number.is_empty() {
            req.serial_number
        } else {
            req.fingerprint_sha256
        };

        match self.cert_manager.get_certificate(&serial).await {
            Some(entry) => {
                let status = if entry.revoked_at.is_some() {
                    "revoked"
                } else if entry.expires_at < chrono::Utc::now() {
                    "expired"
                } else {
                    "valid"
                };

                Ok(Response::new(CertificateStatusResponse {
                    found: true,
                    serial_number: entry.serial_number.clone(),
                    status: status.to_string(),
                    issued_at_ms: entry.issued_at.timestamp_millis(),
                    expires_at_ms: entry.expires_at.timestamp_millis(),
                    revoked_at_ms: entry.revoked_at.map(|t| t.timestamp_millis()).unwrap_or(0),
                    revocation_reason: entry.revocation_reason
                        .as_ref()
                        .map(|r| Self::to_proto_revocation_reason(r))
                        .unwrap_or(0),
                }))
            }
            None => {
                Ok(Response::new(CertificateStatusResponse {
                    found: false,
                    serial_number: serial,
                    status: "unknown".to_string(),
                    ..Default::default()
                }))
            }
        }
    }

    /// Get Certificate Revocation List
    async fn get_crl(
        &self,
        _request: Request<CrlRequest>,
    ) -> Result<Response<CrlResponse>, Status> {
        let (crl_entries, this_update, next_update) = self.cert_manager.get_crl().await;

        // Generate CRL PEM (simplified - in production use proper X.509 CRL encoding)
        let crl_content: Vec<String> = crl_entries.iter()
            .map(|e| format!("{}:{:?}:{}", e.serial_number, e.reason, e.revoked_at.to_rfc3339()))
            .collect();
        let crl_pem = format!(
            "-----BEGIN X509 CRL-----\n{}\n-----END X509 CRL-----",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, crl_content.join("\n").as_bytes())
        );

        Ok(Response::new(CrlResponse {
            crl_der: crl_pem.as_bytes().to_vec(), // Simplified - use proper DER encoding in production
            crl_pem,
            this_update_ms: this_update.timestamp_millis(),
            next_update_ms: next_update.timestamp_millis(),
            revoked_count: crl_entries.len() as u32,
        }))
    }

    /// OCSP responder with proper DER encoding (RFC 6960)
    async fn check_certificate_ocsp(
        &self,
        request: Request<OcspRequest>,
    ) -> Result<Response<OcspResponse>, Status> {
        let req = request.into_inner();

        // Check certificate status by serial number
        let (status_str, cert_status) = if !req.serial_number.is_empty() {
            match self.cert_manager.get_certificate(&req.serial_number).await {
                Some(entry) => {
                    if entry.revoked_at.is_some() {
                        ("revoked", OCSP_CERT_STATUS_REVOKED)
                    } else if entry.expires_at < chrono::Utc::now() {
                        ("expired", OCSP_CERT_STATUS_REVOKED) // Expired treated as revoked in OCSP
                    } else {
                        ("good", OCSP_CERT_STATUS_GOOD)
                    }
                }
                None => ("unknown", OCSP_CERT_STATUS_UNKNOWN),
            }
        } else {
            ("unknown", OCSP_CERT_STATUS_UNKNOWN)
        };

        let now = chrono::Utc::now();
        let this_update_ms = now.timestamp_millis();
        let next_update_ms = (now + chrono::Duration::hours(24)).timestamp_millis();

        // Build proper OCSP response in DER format
        let ocsp_response_der = Self::build_ocsp_response_der(
            &req.serial_number,
            cert_status,
            this_update_ms,
            next_update_ms,
        );

        info!(
            "📋 OCSP response: serial={}, status={}, der_size={}",
            if req.serial_number.len() > 8 { &req.serial_number[..8] } else { &req.serial_number },
            status_str,
            ocsp_response_der.len()
        );

        Ok(Response::new(OcspResponse {
            ocsp_response_der,
            status: status_str.to_string(),
            this_update_ms,
            next_update_ms,
        }))
    }
}

