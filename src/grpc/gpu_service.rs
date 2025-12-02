//! CYAN FLAME™ GPU Detection gRPC Service Implementation
//!
//! Implements the GpuDetectionService for:
//! - GPU registration with automatic tier detection
//! - Tiered pricing based on baseline GPU
//! - Supported GPU enumeration

use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{info, warn};

use super::proto::{
    gpu_detection_service_server::GpuDetectionService,
    GpuRegistrationRequest,
    GpuRegistrationResponse,
    GpuTierRequest,
    GpuTierResponse,
    ListSupportedGpusRequest,
    ListSupportedGpusResponse,
    GpuBasedTierConfig as ProtoTierConfig,
    AmplificationTargets as ProtoAmpTargets,
    SupportedGpuInfo,
    BaselineGpuType as ProtoGpuType,
    CostEstimate,
};

use super::gpu_detection::{GpuDetectionManager, BaselineGpuType, GpuSpecifications};

/// GPU Detection Service Implementation
pub struct GpuDetectionServiceImpl {
    detection_manager: Arc<GpuDetectionManager>,
}

impl GpuDetectionServiceImpl {
    pub fn new() -> Self {
        Self {
            detection_manager: Arc::new(GpuDetectionManager::new()),
        }
    }

    pub fn with_manager(manager: Arc<GpuDetectionManager>) -> Self {
        Self {
            detection_manager: manager,
        }
    }

    /// Detect GPU type from registration request
    fn detect_gpu_type(&self, req: &GpuRegistrationRequest) -> BaselineGpuType {
        if let Some(gpu) = &req.detected_gpu {
            // Match by GPU name
            let name = gpu.name.to_lowercase();
            
            if name.contains("h200") {
                BaselineGpuType::NvidiaH200
            } else if name.contains("h100") {
                BaselineGpuType::NvidiaH100
            } else if name.contains("a100") {
                BaselineGpuType::NvidiaA100
            } else if name.contains("v100") {
                BaselineGpuType::NvidiaV100
            } else if name.contains("l40s") {
                BaselineGpuType::NvidiaL40S
            } else if name.contains("5090") {
                BaselineGpuType::NvidiaRtx5090
            } else if name.contains("4090") {
                BaselineGpuType::NvidiaRtx4090
            } else if name.contains("mi300") {
                BaselineGpuType::AmdMi300X
            } else if name.contains("mi250") {
                BaselineGpuType::AmdMi250
            } else if name.contains("mi100") {
                BaselineGpuType::AmdMi100
            } else {
                // Try to detect by compute capability for NVIDIA
                if gpu.vendor.to_uppercase() == "NVIDIA" {
                    match (gpu.compute_capability_major, gpu.compute_capability_minor) {
                        (9, 0) => BaselineGpuType::NvidiaH100,
                        (8, 9) => BaselineGpuType::NvidiaRtx4090, // Ada Lovelace
                        (8, 0) => BaselineGpuType::NvidiaA100,
                        (7, 0) => BaselineGpuType::NvidiaV100,
                        _ => BaselineGpuType::Unknown,
                    }
                } else {
                    BaselineGpuType::Unknown
                }
            }
        } else {
            BaselineGpuType::Unknown
        }
    }

    /// Convert internal tier config to proto
    fn to_proto_tier_config(&self, specs: &GpuSpecifications) -> ProtoTierConfig {
        let tier = super::gpu_detection::GpuTierConfig::from_gpu(specs);
        let amp = super::gpu_detection::AmplificationTargets::from_gpu(specs);

        ProtoTierConfig {
            tier_name: tier.tier_name,
            memory_bandwidth_amplification: amp.memory_bandwidth_multiplier,
            effective_memory_multiplier: tier.memory_bandwidth_amplification,
            tflops_amplification_target: amp.tflops_multiplier,
            pricing_multiplier: tier.pricing_multiplier,
            pricing_tier: tier.pricing_tier,
            max_effective_memory_tb: tier.max_effective_memory_tb,
            max_concurrent_sessions: tier.max_concurrent_sessions,
            rate_limit_per_minute: tier.rate_limit_per_minute,
            optimization_strategies: tier.optimization_strategies,
        }
    }

    /// Convert to proto amplification targets
    fn to_proto_amp_targets(&self, specs: &GpuSpecifications) -> ProtoAmpTargets {
        let amp = super::gpu_detection::AmplificationTargets::from_gpu(specs);

        ProtoAmpTargets {
            memory_bandwidth_multiplier: amp.memory_bandwidth_multiplier,
            current_bandwidth_gbs: amp.current_bandwidth_gbs,
            target_bandwidth_gbs: amp.target_bandwidth_gbs,
            tflops_multiplier: amp.tflops_multiplier,
            current_fp16_tflops: amp.current_fp16_tflops,
            target_fp16_tflops: amp.target_fp16_tflops,
            current_vram_gb: specs.vram_gb,
            target_vram_gb: 80, // H100 80GB
            vram_multiplier: 80.0 / specs.vram_gb as f64,
            needs_fp8_emulation: !specs.supports_fp8,
            needs_sparsity_software: !specs.supports_sparsity,
            has_nvlink: specs.supports_nvlink,
        }
    }

    /// Calculate cost estimate based on GPU specs and amplification
    fn calculate_cost_estimate(&self, specs: &GpuSpecifications) -> CostEstimate {
        // H100 as reference target
        let h100_specs = GpuSpecifications::h100();

        // Calculate amplification factor
        let memory_amp = h100_specs.vram_gb as f64 / specs.vram_gb as f64;
        let compute_amp = h100_specs.fp16_tflops / specs.fp16_tflops;
        let total_amp = (memory_amp + compute_amp) / 2.0;

        // Base pricing tiers (USD per hour)
        let (hourly_rate, tier_name) = match total_amp {
            x if x <= 1.5 => (0.50, "economy"),
            x if x <= 3.0 => (1.50, "standard"),
            x if x <= 5.0 => (3.50, "premium"),
            _ => (7.50, "enterprise"),
        };

        CostEstimate {
            hourly_rate_usd: hourly_rate,
            daily_rate_usd: hourly_rate * 24.0,
            monthly_rate_usd: hourly_rate * 24.0 * 30.0,
            pricing_tier: tier_name.to_string(),
            amplification_factor: total_amp,
            cost_breakdown: format!(
                "Memory: {:.1}× | Compute: {:.1}× | Total: {:.1}×",
                memory_amp, compute_amp, total_amp
            ),
        }
    }
}

impl Default for GpuDetectionServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[tonic::async_trait]
impl GpuDetectionService for GpuDetectionServiceImpl {
    /// Register GPU and get tiered pricing based on detected hardware
    async fn register_gpu(
        &self,
        request: Request<GpuRegistrationRequest>,
    ) -> Result<Response<GpuRegistrationResponse>, Status> {
        let req = request.into_inner();
        info!("📝 GPU registration request from agent: {}", req.agent_id);

        // Detect GPU type from request
        let gpu_type = self.detect_gpu_type(&req);

        if !gpu_type.is_supported() {
            warn!("⚠️  Unsupported GPU detected for agent {}", req.agent_id);
            return Ok(Response::new(GpuRegistrationResponse {
                success: false,
                error_message: "Unsupported GPU. Only V100, A100, H100, H200, L40S, RTX 4090, RTX 5090, MI100, MI250, MI300X are supported.".to_string(),
                detected_baseline: ProtoGpuType::GpuUnknown as i32,
                baseline_name: "Unknown".to_string(),
                target_gpu: 1, // Default to H100
                target_name: "NVIDIA H100 80GB HBM3".to_string(),
                tier_config: None,
                amplification_targets: None,
                certificate_binding: String::new(),
                cost_estimate: None,
            }));
        }

        // Get GPU name from request
        let gpu_name = req.detected_gpu.as_ref()
            .map(|g| g.name.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        // Get specifications and tier config
        let specs = GpuSpecifications::from_type(gpu_type);
        let tier_config = self.to_proto_tier_config(&specs);
        let amp_targets = self.to_proto_amp_targets(&specs);

        // Hash API key for storage
        let api_key_hash = format!("{:x}", md5::compute(&req.api_key));

        // Register with manager
        match self.detection_manager.register_gpu(
            &req.agent_id,
            &api_key_hash,
            &gpu_name,
            None, // No certificate binding yet
        ).await {
            Ok(registration) => {
                info!(
                    "✅ GPU registered: agent={}, gpu={}, tier={}",
                    req.agent_id,
                    gpu_type.name(),
                    tier_config.tier_name
                );

                // Calculate cost estimate based on amplification
                let cost_estimate = self.calculate_cost_estimate(&specs);

                Ok(Response::new(GpuRegistrationResponse {
                    success: true,
                    error_message: String::new(),
                    detected_baseline: gpu_type.to_proto(),
                    baseline_name: gpu_type.name().to_string(),
                    target_gpu: 1, // Default to H100
                    target_name: "NVIDIA H100 80GB HBM3".to_string(),
                    tier_config: Some(tier_config),
                    amplification_targets: Some(amp_targets),
                    certificate_binding: registration.certificate_fingerprint.unwrap_or_default(),
                    cost_estimate: Some(cost_estimate),
                }))
            }
            Err(e) => {
                warn!("❌ GPU registration failed: {}", e);
                Ok(Response::new(GpuRegistrationResponse {
                    success: false,
                    error_message: e,
                    detected_baseline: ProtoGpuType::GpuUnknown as i32,
                    baseline_name: "Unknown".to_string(),
                    target_gpu: 1, // Default to H100
                    target_name: "NVIDIA H100 80GB HBM3".to_string(),
                    tier_config: None,
                    amplification_targets: None,
                    certificate_binding: String::new(),
                    cost_estimate: None,
                }))
            }
        }
    }

    /// Get current GPU tier configuration
    async fn get_gpu_tier_config(
        &self,
        request: Request<GpuTierRequest>,
    ) -> Result<Response<GpuTierResponse>, Status> {
        let req = request.into_inner();

        // Look up existing registration
        if let Some(registration) = self.detection_manager.get_registration(&req.agent_id).await {
            let specs = GpuSpecifications::from_type(registration.detected_gpu);
            let tier_config = self.to_proto_tier_config(&specs);
            let amp_targets = self.to_proto_amp_targets(&specs);

            Ok(Response::new(GpuTierResponse {
                current_baseline: registration.detected_gpu.to_proto(),
                tier_config: Some(tier_config),
                amplification_targets: Some(amp_targets),
            }))
        } else {
            Err(Status::not_found("No GPU registered for this agent. Call RegisterGpu first."))
        }
    }

    /// List all supported GPU tiers
    async fn list_supported_gpus(
        &self,
        _request: Request<ListSupportedGpusRequest>,
    ) -> Result<Response<ListSupportedGpusResponse>, Status> {
        let supported = self.detection_manager.get_supported_gpus();

        let supported_gpus: Vec<SupportedGpuInfo> = supported
            .into_iter()
            .map(|(gpu_type, specs, tier)| {
                let amp = super::gpu_detection::AmplificationTargets::from_gpu(&specs);
                SupportedGpuInfo {
                    gpu_type: gpu_type.to_proto(),
                    name: gpu_type.name().to_string(),
                    architecture: specs.architecture.clone(),
                    vendor: if specs.architecture.contains("CDNA") { "AMD" } else { "NVIDIA" }.to_string(),
                    vram_gb: specs.vram_gb,
                    memory_bandwidth_gbs: specs.memory_bandwidth_gbs,
                    fp16_tflops: specs.fp16_tflops,
                    pricing_tier: tier.pricing_tier,
                    tflops_amplification_to_h100: amp.tflops_multiplier,
                    bandwidth_amplification_to_h100: amp.memory_bandwidth_multiplier,
                }
            })
            .collect();

        Ok(Response::new(ListSupportedGpusResponse {
            supported_gpus,
        }))
    }
}

