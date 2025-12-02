//! CYAN FLAME™ GPU Detection & Tiered Pricing Module
//!
//! Detects physical GPU specifications and applies tiered pricing based on
//! baseline GPU type. Supports enterprise GPUs:
//!
//! | GPU Type      | Architecture | Tier             | TFLOPS Amp. | BW Amp. |
//! |---------------|--------------|------------------|-------------|---------|
//! | V100          | Volta        | Legacy           | 7.9×        | 3.7×    |
//! | A100          | Ampere       | Workhorse        | 3.2×        | 1.67×   |
//! | H100          | Hopper       | Target Benchmark | 1.0×        | 1.0×    |
//! | H200          | Hopper+      | Premium          | 1.0×        | 0.70×   |
//! | L40S          | Ada          | Inference Pro    | 0.67×       | 3.9×    |
//! | RTX 4090      | Ada          | Consumer Pro     | 3.0×        | 3.35×   |
//! | RTX 5090      | Blackwell    | Consumer Premium | TBD         | TBD     |
//! | MI100         | CDNA 1       | AMD Legacy       | 5.4×        | 2.8×    |
//! | MI250         | CDNA 2       | AMD Workhorse    | 2.6×        | 1.05×   |
//! | MI300X        | CDNA 3       | AMD Flagship     | 0.38×       | 0.63×   |

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Supported baseline GPU types for tiered pricing
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BaselineGpuType {
    Unknown,
    // NVIDIA Data Center
    NvidiaV100,
    NvidiaA100,
    NvidiaH100,
    NvidiaH200,
    NvidiaL40S,
    NvidiaA10,
    NvidiaA30,
    NvidiaA40,
    NvidiaTeslaT4,
    // NVIDIA Consumer - Ada Lovelace
    NvidiaRtx4090,
    NvidiaRtx4080,
    NvidiaRtx4070Ti,
    // NVIDIA Consumer - Blackwell
    NvidiaRtx5090,
    NvidiaRtx5080,
    // NVIDIA Legacy Consumer - Ampere
    NvidiaRtx3090,
    NvidiaRtx3090Ti,
    NvidiaRtx3080,
    // AMD Instinct
    AmdMi100,
    AmdMi250,
    AmdMi300X,
    AmdMi325X,
    // AMD Consumer - RDNA 3
    AmdRx7900Xtx,
    AmdRx7900Xt,
}

/// Target GPU types that can be emulated
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TargetGpuType {
    Unknown,
    H100,       // NVIDIA H100 80GB HBM3 - Default target
    H200,       // NVIDIA H200 141GB HBM3e
    Mi300X,     // AMD MI300X 192GB HBM3
    A100,       // NVIDIA A100 80GB HBM2e
    L40S,       // NVIDIA L40S 48GB GDDR6
    Custom,     // Custom specifications
}

impl TargetGpuType {
    pub fn from_proto(value: i32) -> Self {
        match value {
            1 => Self::H100,
            2 => Self::H200,
            3 => Self::Mi300X,
            4 => Self::A100,
            5 => Self::L40S,
            99 => Self::Custom,
            _ => Self::Unknown,
        }
    }

    pub fn to_proto(&self) -> i32 {
        match self {
            Self::Unknown => 0,
            Self::H100 => 1,
            Self::H200 => 2,
            Self::Mi300X => 3,
            Self::A100 => 4,
            Self::L40S => 5,
            Self::Custom => 99,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Unknown => "Unknown Target",
            Self::H100 => "NVIDIA H100 80GB HBM3",
            Self::H200 => "NVIDIA H200 141GB HBM3e",
            Self::Mi300X => "AMD MI300X 192GB HBM3",
            Self::A100 => "NVIDIA A100 80GB HBM2e",
            Self::L40S => "NVIDIA L40S 48GB GDDR6",
            Self::Custom => "Custom Target",
        }
    }
}

impl BaselineGpuType {
    /// Convert from proto enum value
    pub fn from_proto(value: i32) -> Self {
        match value {
            1 => Self::NvidiaV100,
            2 => Self::NvidiaA100,
            3 => Self::NvidiaH100,
            4 => Self::NvidiaH200,
            5 => Self::NvidiaL40S,
            6 => Self::NvidiaA10,
            7 => Self::NvidiaA30,
            8 => Self::NvidiaA40,
            9 => Self::NvidiaTeslaT4,
            10 => Self::NvidiaRtx4090,
            11 => Self::NvidiaRtx4080,
            12 => Self::NvidiaRtx4070Ti,
            13 => Self::NvidiaRtx5090,
            14 => Self::NvidiaRtx5080,
            15 => Self::NvidiaRtx3090,
            16 => Self::NvidiaRtx3090Ti,
            17 => Self::NvidiaRtx3080,
            20 => Self::AmdMi100,
            21 => Self::AmdMi250,
            22 => Self::AmdMi300X,
            23 => Self::AmdMi325X,
            25 => Self::AmdRx7900Xtx,
            26 => Self::AmdRx7900Xt,
            _ => Self::Unknown,
        }
    }

    /// Convert to proto enum value
    pub fn to_proto(&self) -> i32 {
        match self {
            Self::Unknown => 0,
            Self::NvidiaV100 => 1,
            Self::NvidiaA100 => 2,
            Self::NvidiaH100 => 3,
            Self::NvidiaH200 => 4,
            Self::NvidiaL40S => 5,
            Self::NvidiaA10 => 6,
            Self::NvidiaA30 => 7,
            Self::NvidiaA40 => 8,
            Self::NvidiaTeslaT4 => 9,
            Self::NvidiaRtx4090 => 10,
            Self::NvidiaRtx4080 => 11,
            Self::NvidiaRtx4070Ti => 12,
            Self::NvidiaRtx5090 => 13,
            Self::NvidiaRtx5080 => 14,
            Self::NvidiaRtx3090 => 15,
            Self::NvidiaRtx3090Ti => 16,
            Self::NvidiaRtx3080 => 17,
            Self::AmdMi100 => 20,
            Self::AmdMi250 => 21,
            Self::AmdMi300X => 22,
            Self::AmdMi325X => 23,
            Self::AmdRx7900Xtx => 25,
            Self::AmdRx7900Xt => 26,
        }
    }

    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Unknown => "Unknown GPU",
            Self::NvidiaV100 => "NVIDIA V100",
            Self::NvidiaA100 => "NVIDIA A100",
            Self::NvidiaH100 => "NVIDIA H100",
            Self::NvidiaH200 => "NVIDIA H200",
            Self::NvidiaL40S => "NVIDIA L40S",
            Self::NvidiaA10 => "NVIDIA A10",
            Self::NvidiaA30 => "NVIDIA A30",
            Self::NvidiaA40 => "NVIDIA A40",
            Self::NvidiaTeslaT4 => "NVIDIA Tesla T4",
            Self::NvidiaRtx4090 => "NVIDIA RTX 4090",
            Self::NvidiaRtx4080 => "NVIDIA RTX 4080",
            Self::NvidiaRtx4070Ti => "NVIDIA RTX 4070 Ti",
            Self::NvidiaRtx5090 => "NVIDIA RTX 5090",
            Self::NvidiaRtx5080 => "NVIDIA RTX 5080",
            Self::NvidiaRtx3090 => "NVIDIA RTX 3090",
            Self::NvidiaRtx3090Ti => "NVIDIA RTX 3090 Ti",
            Self::NvidiaRtx3080 => "NVIDIA RTX 3080",
            Self::AmdMi100 => "AMD Instinct MI100",
            Self::AmdMi250 => "AMD Instinct MI250",
            Self::AmdMi300X => "AMD Instinct MI300X",
            Self::AmdMi325X => "AMD Instinct MI325X",
            Self::AmdRx7900Xtx => "AMD RX 7900 XTX",
            Self::AmdRx7900Xt => "AMD RX 7900 XT",
        }
    }

    /// Check if this GPU type is supported
    pub fn is_supported(&self) -> bool {
        !matches!(self, Self::Unknown)
    }
}

/// GPU Specifications for baseline calculations
#[derive(Clone, Debug)]
pub struct GpuSpecifications {
    pub gpu_type: BaselineGpuType,
    pub vendor: String,
    pub architecture: String,
    pub vram_gb: u64,
    pub memory_bandwidth_gbs: f64,
    pub fp16_tflops: f64,
    pub fp32_tflops: f64,
    pub tf32_tflops: f64,
    pub fp8_tflops: f64,
    pub compute_capability: (u32, u32),
    pub supports_fp8: bool,
    pub supports_sparsity: bool,
    pub supports_nvlink: bool,
}

impl GpuSpecifications {
    /// H100 specifications - the target benchmark
    pub fn h100() -> Self {
        Self {
            gpu_type: BaselineGpuType::NvidiaH100,
            vendor: "NVIDIA".to_string(),
            architecture: "Hopper".to_string(),
            vram_gb: 80,
            memory_bandwidth_gbs: 3350.0,
            fp16_tflops: 989.0,
            fp32_tflops: 67.0,
            tf32_tflops: 495.0,
            fp8_tflops: 1979.0,
            compute_capability: (9, 0),
            supports_fp8: true,
            supports_sparsity: true,
            supports_nvlink: true,
        }
    }

    /// V100 specifications - Legacy tier
    pub fn v100() -> Self {
        Self {
            gpu_type: BaselineGpuType::NvidiaV100,
            vendor: "NVIDIA".to_string(),
            architecture: "Volta".to_string(),
            vram_gb: 32,
            memory_bandwidth_gbs: 900.0,
            fp16_tflops: 125.0,
            fp32_tflops: 15.7,
            tf32_tflops: 0.0, // Not supported
            fp8_tflops: 0.0,  // Not supported
            compute_capability: (7, 0),
            supports_fp8: false,
            supports_sparsity: false,
            supports_nvlink: true,
        }
    }

    /// A100 specifications - Workhorse tier
    pub fn a100() -> Self {
        Self {
            gpu_type: BaselineGpuType::NvidiaA100,
            vendor: "NVIDIA".to_string(),
            architecture: "Ampere".to_string(),
            vram_gb: 80,
            memory_bandwidth_gbs: 2000.0,
            fp16_tflops: 312.0,
            fp32_tflops: 19.5,
            tf32_tflops: 156.0,
            fp8_tflops: 0.0, // Not native
            compute_capability: (8, 0),
            supports_fp8: false,
            supports_sparsity: true,
            supports_nvlink: true,
        }
    }

    /// H200 specifications - Enhanced Hopper
    pub fn h200() -> Self {
        Self {
            gpu_type: BaselineGpuType::NvidiaH200,
            vendor: "NVIDIA".to_string(),
            architecture: "Hopper".to_string(),
            vram_gb: 141,
            memory_bandwidth_gbs: 4800.0,
            fp16_tflops: 989.0,
            fp32_tflops: 67.0,
            tf32_tflops: 495.0,
            fp8_tflops: 1979.0,
            compute_capability: (9, 0),
            supports_fp8: true,
            supports_sparsity: true,
            supports_nvlink: true,
        }
    }

    /// L40S specifications - Inference Pro tier
    pub fn l40s() -> Self {
        Self {
            gpu_type: BaselineGpuType::NvidiaL40S,
            vendor: "NVIDIA".to_string(),
            architecture: "Ada".to_string(),
            vram_gb: 48,
            memory_bandwidth_gbs: 864.0,
            fp16_tflops: 362.0,
            fp32_tflops: 91.6,
            tf32_tflops: 181.0,
            fp8_tflops: 1466.0,
            compute_capability: (8, 9),
            supports_fp8: true,
            supports_sparsity: false,
            supports_nvlink: false,
        }
    }

    /// RTX 4090 specifications - Consumer Pro tier
    pub fn rtx_4090() -> Self {
        Self {
            gpu_type: BaselineGpuType::NvidiaRtx4090,
            vendor: "NVIDIA".to_string(),
            architecture: "Ada".to_string(),
            vram_gb: 24,
            memory_bandwidth_gbs: 1008.0,
            fp16_tflops: 330.0,
            fp32_tflops: 82.6,
            tf32_tflops: 165.0,
            fp8_tflops: 660.0,
            compute_capability: (8, 9),
            supports_fp8: true,
            supports_sparsity: false,
            supports_nvlink: false,
        }
    }

    /// RTX 5090 specifications - Consumer Premium tier (estimated)
    pub fn rtx_5090() -> Self {
        Self {
            gpu_type: BaselineGpuType::NvidiaRtx5090,
            vendor: "NVIDIA".to_string(),
            architecture: "Blackwell".to_string(),
            vram_gb: 32,
            memory_bandwidth_gbs: 1792.0,
            fp16_tflops: 838.0,
            fp32_tflops: 104.8,
            tf32_tflops: 419.0,
            fp8_tflops: 1676.0,
            compute_capability: (10, 0),
            supports_fp8: true,
            supports_sparsity: true,
            supports_nvlink: false,
        }
    }

    /// MI100 specifications - AMD Legacy tier
    pub fn mi100() -> Self {
        Self {
            gpu_type: BaselineGpuType::AmdMi100,
            vendor: "AMD".to_string(),
            architecture: "CDNA1".to_string(),
            vram_gb: 32,
            memory_bandwidth_gbs: 1200.0,
            fp16_tflops: 184.6,
            fp32_tflops: 23.1,
            tf32_tflops: 0.0,
            fp8_tflops: 0.0,
            compute_capability: (0, 0),
            supports_fp8: false,
            supports_sparsity: false,
            supports_nvlink: false,
        }
    }

    /// MI250 specifications - AMD Workhorse tier
    pub fn mi250() -> Self {
        Self {
            gpu_type: BaselineGpuType::AmdMi250,
            vendor: "AMD".to_string(),
            architecture: "CDNA2".to_string(),
            vram_gb: 128,
            memory_bandwidth_gbs: 3200.0,
            fp16_tflops: 383.0,
            fp32_tflops: 47.9,
            tf32_tflops: 0.0,
            fp8_tflops: 0.0,
            compute_capability: (0, 0),
            supports_fp8: false,
            supports_sparsity: true,
            supports_nvlink: false,
        }
    }

    /// MI300X specifications - AMD Flagship tier
    pub fn mi300x() -> Self {
        Self {
            gpu_type: BaselineGpuType::AmdMi300X,
            vendor: "AMD".to_string(),
            architecture: "CDNA3".to_string(),
            vram_gb: 192,
            memory_bandwidth_gbs: 5300.0,
            fp16_tflops: 1307.0,
            fp32_tflops: 163.0,
            tf32_tflops: 0.0,
            fp8_tflops: 2615.0,
            compute_capability: (0, 0),
            supports_fp8: true,
            supports_sparsity: true,
            supports_nvlink: false,
        }
    }

    /// MI325X specifications - AMD Ultra tier
    pub fn mi325x() -> Self {
        Self {
            gpu_type: BaselineGpuType::AmdMi325X,
            vendor: "AMD".to_string(),
            architecture: "CDNA3+".to_string(),
            vram_gb: 256,
            memory_bandwidth_gbs: 6000.0,
            fp16_tflops: 1500.0,
            fp32_tflops: 187.5,
            tf32_tflops: 0.0,
            fp8_tflops: 3000.0,
            compute_capability: (0, 0),
            supports_fp8: true,
            supports_sparsity: true,
            supports_nvlink: false,
        }
    }

    /// Tesla T4 specifications - Budget Inference tier
    pub fn tesla_t4() -> Self {
        Self {
            gpu_type: BaselineGpuType::NvidiaTeslaT4,
            vendor: "NVIDIA".to_string(),
            architecture: "Turing".to_string(),
            vram_gb: 16,
            memory_bandwidth_gbs: 300.0,
            fp16_tflops: 65.0,
            fp32_tflops: 8.1,
            tf32_tflops: 0.0,
            fp8_tflops: 0.0,
            compute_capability: (7, 5),
            supports_fp8: false,
            supports_sparsity: false,
            supports_nvlink: false,
        }
    }

    /// A10 specifications - Entry Inference tier
    pub fn a10() -> Self {
        Self {
            gpu_type: BaselineGpuType::NvidiaA10,
            vendor: "NVIDIA".to_string(),
            architecture: "Ampere".to_string(),
            vram_gb: 24,
            memory_bandwidth_gbs: 600.0,
            fp16_tflops: 125.0,
            fp32_tflops: 31.2,
            tf32_tflops: 62.5,
            fp8_tflops: 0.0,
            compute_capability: (8, 6),
            supports_fp8: false,
            supports_sparsity: true,
            supports_nvlink: false,
        }
    }

    /// RTX 4080 specifications
    pub fn rtx_4080() -> Self {
        Self {
            gpu_type: BaselineGpuType::NvidiaRtx4080,
            vendor: "NVIDIA".to_string(),
            architecture: "Ada".to_string(),
            vram_gb: 16,
            memory_bandwidth_gbs: 717.0,
            fp16_tflops: 242.0,
            fp32_tflops: 48.7,
            tf32_tflops: 121.0,
            fp8_tflops: 484.0,
            compute_capability: (8, 9),
            supports_fp8: true,
            supports_sparsity: false,
            supports_nvlink: false,
        }
    }

    /// RTX 3090 specifications
    pub fn rtx_3090() -> Self {
        Self {
            gpu_type: BaselineGpuType::NvidiaRtx3090,
            vendor: "NVIDIA".to_string(),
            architecture: "Ampere".to_string(),
            vram_gb: 24,
            memory_bandwidth_gbs: 936.0,
            fp16_tflops: 142.0,
            fp32_tflops: 35.6,
            tf32_tflops: 71.0,
            fp8_tflops: 0.0,
            compute_capability: (8, 6),
            supports_fp8: false,
            supports_sparsity: true,
            supports_nvlink: true,
        }
    }

    /// RX 7900 XTX specifications - AMD Consumer Pro
    pub fn rx_7900_xtx() -> Self {
        Self {
            gpu_type: BaselineGpuType::AmdRx7900Xtx,
            vendor: "AMD".to_string(),
            architecture: "RDNA3".to_string(),
            vram_gb: 24,
            memory_bandwidth_gbs: 960.0,
            fp16_tflops: 123.0,
            fp32_tflops: 61.4,
            tf32_tflops: 0.0,
            fp8_tflops: 0.0,
            compute_capability: (0, 0),
            supports_fp8: false,
            supports_sparsity: false,
            supports_nvlink: false,
        }
    }

    /// Get specifications by GPU type
    pub fn from_type(gpu_type: BaselineGpuType) -> Self {
        match gpu_type {
            BaselineGpuType::NvidiaV100 => Self::v100(),
            BaselineGpuType::NvidiaA100 => Self::a100(),
            BaselineGpuType::NvidiaH100 => Self::h100(),
            BaselineGpuType::NvidiaH200 => Self::h200(),
            BaselineGpuType::NvidiaL40S => Self::l40s(),
            BaselineGpuType::NvidiaA10 => Self::a10(),
            BaselineGpuType::NvidiaA30 => Self::a10(),  // Similar to A10
            BaselineGpuType::NvidiaA40 => Self::a10(),  // Similar to A10
            BaselineGpuType::NvidiaTeslaT4 => Self::tesla_t4(),
            BaselineGpuType::NvidiaRtx4090 => Self::rtx_4090(),
            BaselineGpuType::NvidiaRtx4080 => Self::rtx_4080(),
            BaselineGpuType::NvidiaRtx4070Ti => Self::rtx_4080(),  // Similar to 4080
            BaselineGpuType::NvidiaRtx5090 => Self::rtx_5090(),
            BaselineGpuType::NvidiaRtx5080 => Self::rtx_5090(),  // Similar to 5090
            BaselineGpuType::NvidiaRtx3090 => Self::rtx_3090(),
            BaselineGpuType::NvidiaRtx3090Ti => Self::rtx_3090(),  // Similar to 3090
            BaselineGpuType::NvidiaRtx3080 => Self::rtx_3090(),  // Similar to 3090
            BaselineGpuType::AmdMi100 => Self::mi100(),
            BaselineGpuType::AmdMi250 => Self::mi250(),
            BaselineGpuType::AmdMi300X => Self::mi300x(),
            BaselineGpuType::AmdMi325X => Self::mi325x(),
            BaselineGpuType::AmdRx7900Xtx => Self::rx_7900_xtx(),
            BaselineGpuType::AmdRx7900Xt => Self::rx_7900_xtx(),  // Similar to XTX
            BaselineGpuType::Unknown => Self::h100(), // Default to H100 for unknown
        }
    }

    /// Get all supported GPU specifications
    pub fn all_supported() -> Vec<Self> {
        vec![
            Self::v100(),
            Self::a100(),
            Self::h100(),
            Self::h200(),
            Self::l40s(),
            Self::rtx_4090(),
            Self::rtx_5090(),
            Self::mi100(),
            Self::mi250(),
            Self::mi300x(),
        ]
    }
}


/// GPU-based tier configuration
#[derive(Clone, Debug)]
pub struct GpuTierConfig {
    /// Tier name
    pub tier_name: String,
    /// Memory bandwidth amplification factor (to reach H100 level)
    pub memory_bandwidth_amplification: f64,
    /// Effective memory multiplier
    pub effective_memory_multiplier: f64,
    /// TFLOPS amplification target (to reach H100 level)
    pub tflops_amplification_target: f64,
    /// Pricing multiplier (relative to base price)
    pub pricing_multiplier: f64,
    /// Pricing tier name
    pub pricing_tier: String,
    /// Maximum effective memory in TB
    pub max_effective_memory_tb: u64,
    /// Max concurrent sessions
    pub max_concurrent_sessions: u32,
    /// Rate limit per minute (0 = unlimited)
    pub rate_limit_per_minute: u32,
    /// Recommended optimization strategies
    pub optimization_strategies: Vec<String>,
}

impl GpuTierConfig {
    /// Calculate tier config based on GPU specifications
    pub fn from_gpu(specs: &GpuSpecifications) -> Self {
        let h100 = GpuSpecifications::h100();

        // Calculate amplification factors needed to reach H100
        let bw_amp = h100.memory_bandwidth_gbs / specs.memory_bandwidth_gbs;
        let tflops_amp = h100.fp16_tflops / specs.fp16_tflops;

        // Determine tier name and pricing based on GPU type
        let (tier_name, pricing_tier, pricing_mult, max_mem, max_sessions, rate_limit) =
            match specs.gpu_type {
                BaselineGpuType::NvidiaV100 => (
                    "legacy".to_string(),
                    "economy".to_string(),
                    0.5,    // Lower price due to older hardware
                    100,    // 100 TB max effective
                    5,
                    1000,
                ),
                BaselineGpuType::NvidiaA100 => (
                    "workhorse".to_string(),
                    "standard".to_string(),
                    1.0,    // Base price
                    240,    // 240 TB max effective
                    20,
                    5000,
                ),
                BaselineGpuType::NvidiaH100 => (
                    "benchmark".to_string(),
                    "premium".to_string(),
                    1.5,    // Premium price (already at target)
                    574,    // 574 TB max effective (full capacity)
                    100,
                    0,      // Unlimited
                ),
                BaselineGpuType::NvidiaH200 => (
                    "ultra".to_string(),
                    "premium_plus".to_string(),
                    2.0,    // Higher due to enhanced specs
                    800,    // 800 TB max effective
                    100,
                    0,
                ),
                BaselineGpuType::NvidiaL40S => (
                    "inference_pro".to_string(),
                    "standard".to_string(),
                    0.9,
                    200,
                    15,
                    3000,
                ),
                BaselineGpuType::NvidiaA10 | BaselineGpuType::NvidiaA30 | BaselineGpuType::NvidiaA40 => (
                    "inference_entry".to_string(),
                    "economy".to_string(),
                    0.6,
                    100,
                    10,
                    2000,
                ),
                BaselineGpuType::NvidiaTeslaT4 => (
                    "budget_inference".to_string(),
                    "economy".to_string(),
                    0.3,
                    40,
                    3,
                    500,
                ),
                BaselineGpuType::NvidiaRtx4090 => (
                    "consumer_pro".to_string(),
                    "economy".to_string(),
                    0.4,    // Lower due to consumer hardware
                    50,     // 50 TB max (limited VRAM)
                    3,
                    500,
                ),
                BaselineGpuType::NvidiaRtx4080 | BaselineGpuType::NvidiaRtx4070Ti => (
                    "consumer_mid".to_string(),
                    "economy".to_string(),
                    0.35,
                    40,
                    2,
                    400,
                ),
                BaselineGpuType::NvidiaRtx5090 => (
                    "consumer_premium".to_string(),
                    "standard".to_string(),
                    0.6,
                    80,
                    5,
                    1000,
                ),
                BaselineGpuType::NvidiaRtx5080 => (
                    "consumer_premium_mid".to_string(),
                    "economy".to_string(),
                    0.5,
                    60,
                    4,
                    800,
                ),
                BaselineGpuType::NvidiaRtx3090 | BaselineGpuType::NvidiaRtx3090Ti => (
                    "consumer_legacy_pro".to_string(),
                    "economy".to_string(),
                    0.35,
                    50,
                    3,
                    500,
                ),
                BaselineGpuType::NvidiaRtx3080 => (
                    "consumer_legacy".to_string(),
                    "economy".to_string(),
                    0.3,
                    30,
                    2,
                    300,
                ),
                BaselineGpuType::AmdMi100 => (
                    "amd_legacy".to_string(),
                    "economy".to_string(),
                    0.4,
                    80,
                    5,
                    1000,
                ),
                BaselineGpuType::AmdMi250 => (
                    "amd_workhorse".to_string(),
                    "standard".to_string(),
                    0.8,
                    300,    // High VRAM
                    20,
                    5000,
                ),
                BaselineGpuType::AmdMi300X => (
                    "amd_flagship".to_string(),
                    "premium".to_string(),
                    1.3,    // High due to excellent specs
                    500,    // Very high VRAM
                    50,
                    0,
                ),
                BaselineGpuType::AmdMi325X => (
                    "amd_ultra".to_string(),
                    "premium_plus".to_string(),
                    1.5,
                    650,
                    75,
                    0,
                ),
                BaselineGpuType::AmdRx7900Xtx | BaselineGpuType::AmdRx7900Xt => (
                    "amd_consumer".to_string(),
                    "economy".to_string(),
                    0.35,
                    50,
                    3,
                    500,
                ),
                BaselineGpuType::Unknown => (
                    "unsupported".to_string(),
                    "blocked".to_string(),
                    0.0,
                    0,
                    0,
                    0,
                ),
            };

        // Determine optimization strategies
        let strategies = Self::get_optimization_strategies(specs);

        Self {
            tier_name,
            memory_bandwidth_amplification: bw_amp,
            effective_memory_multiplier: specs.vram_gb as f64 * 24500.0 / 24.0 / 1024.0,
            tflops_amplification_target: tflops_amp,
            pricing_multiplier: pricing_mult,
            pricing_tier,
            max_effective_memory_tb: max_mem,
            max_concurrent_sessions: max_sessions,
            rate_limit_per_minute: rate_limit,
            optimization_strategies: strategies,
        }
    }

    /// Get recommended optimization strategies based on GPU capabilities
    fn get_optimization_strategies(specs: &GpuSpecifications) -> Vec<String> {
        let mut strategies = Vec::new();

        // FP8 emulation needed if not natively supported
        if !specs.supports_fp8 {
            strategies.push("fp8_software_emulation".to_string());
        } else {
            strategies.push("native_fp8_utilization".to_string());
        }

        // Sparsity activation if supported
        if specs.supports_sparsity {
            strategies.push("structured_sparsity_activation".to_string());
        } else {
            strategies.push("software_sparsity_optimization".to_string());
        }

        // Memory-specific strategies
        if specs.memory_bandwidth_gbs < 1500.0 {
            strategies.push("aggressive_memory_caching".to_string());
            strategies.push("data_compression".to_string());
        }

        // VRAM-specific strategies
        if specs.vram_gb < 48 {
            strategies.push("model_offloading".to_string());
            strategies.push("ultra_low_bit_quantization".to_string()); // QLoRA, GPTQ
        }

        // Architecture-specific
        match specs.architecture.as_str() {
            "Volta" => {
                strategies.push("aggressive_int8_quantization".to_string());
                strategies.push("kernel_fusion".to_string());
            }
            "Ampere" => {
                strategies.push("tf32_tensor_optimization".to_string());
                strategies.push("tensorrt_llm_integration".to_string());
            }
            "Hopper" => {
                strategies.push("transformer_engine_optimization".to_string());
            }
            "Ada" => {
                strategies.push("dynamic_batching".to_string());
                strategies.push("cuda_graph_optimization".to_string());
            }
            "CDNA1" | "CDNA2" | "CDNA3" => {
                strategies.push("rocm_kernel_optimization".to_string());
                strategies.push("hip_stream_parallelism".to_string());
            }
            _ => {}
        }

        strategies
    }
}



/// Amplification targets to reach H100-equivalent performance
#[derive(Clone, Debug)]
pub struct AmplificationTargets {
    /// Memory bandwidth multiplier needed
    pub memory_bandwidth_multiplier: f64,
    /// Current bandwidth in GB/s
    pub current_bandwidth_gbs: f64,
    /// Target bandwidth (H100 = 3350 GB/s)
    pub target_bandwidth_gbs: f64,
    /// TFLOPS multiplier needed
    pub tflops_multiplier: f64,
    /// Current FP16 TFLOPS
    pub current_fp16_tflops: f64,
    /// Target FP16 TFLOPS (H100 = 989)
    pub target_fp16_tflops: f64,
    /// Current VRAM in GB
    pub current_vram_gb: u64,
    /// Target VRAM (H100 = 80 GB)
    pub target_vram_gb: u64,
    /// VRAM multiplier
    pub vram_multiplier: f64,
    /// Needs FP8 emulation
    pub needs_fp8_emulation: bool,
    /// Needs software sparsity
    pub needs_sparsity_software: bool,
    /// Has NVLink
    pub has_nvlink: bool,
}

impl AmplificationTargets {
    /// Calculate amplification targets from GPU specs
    pub fn from_gpu(specs: &GpuSpecifications) -> Self {
        let h100 = GpuSpecifications::h100();

        Self {
            memory_bandwidth_multiplier: h100.memory_bandwidth_gbs / specs.memory_bandwidth_gbs,
            current_bandwidth_gbs: specs.memory_bandwidth_gbs,
            target_bandwidth_gbs: h100.memory_bandwidth_gbs,
            tflops_multiplier: h100.fp16_tflops / specs.fp16_tflops,
            current_fp16_tflops: specs.fp16_tflops,
            target_fp16_tflops: h100.fp16_tflops,
            current_vram_gb: specs.vram_gb,
            target_vram_gb: h100.vram_gb,
            vram_multiplier: h100.vram_gb as f64 / specs.vram_gb as f64,
            needs_fp8_emulation: !specs.supports_fp8,
            needs_sparsity_software: !specs.supports_sparsity,
            has_nvlink: specs.supports_nvlink,
        }
    }
}

/// GPU Registration entry for tracking connected GPUs
#[derive(Clone, Debug)]
pub struct GpuRegistration {
    pub agent_id: String,
    pub api_key_hash: String,
    pub detected_gpu: BaselineGpuType,
    pub gpu_specs: GpuSpecifications,
    pub tier_config: GpuTierConfig,
    pub amplification_targets: AmplificationTargets,
    pub certificate_fingerprint: Option<String>,
    pub registered_at: chrono::DateTime<chrono::Utc>,
    pub last_seen: chrono::DateTime<chrono::Utc>,
}

/// GPU Detection Manager
pub struct GpuDetectionManager {
    /// Registered GPUs by agent ID
    registrations: Arc<RwLock<HashMap<String, GpuRegistration>>>,
    /// GPU name patterns for detection
    gpu_patterns: HashMap<&'static str, BaselineGpuType>,
}

impl GpuDetectionManager {
    /// Create a new GPU detection manager
    pub fn new() -> Self {
        let mut patterns = HashMap::new();

        // NVIDIA patterns
        patterns.insert("V100", BaselineGpuType::NvidiaV100);
        patterns.insert("Tesla V100", BaselineGpuType::NvidiaV100);
        patterns.insert("A100", BaselineGpuType::NvidiaA100);
        patterns.insert("Tesla A100", BaselineGpuType::NvidiaA100);
        patterns.insert("H100", BaselineGpuType::NvidiaH100);
        patterns.insert("H200", BaselineGpuType::NvidiaH200);
        patterns.insert("L40S", BaselineGpuType::NvidiaL40S);
        patterns.insert("L40", BaselineGpuType::NvidiaL40S);
        patterns.insert("RTX 4090", BaselineGpuType::NvidiaRtx4090);
        patterns.insert("GeForce RTX 4090", BaselineGpuType::NvidiaRtx4090);
        patterns.insert("RTX 5090", BaselineGpuType::NvidiaRtx5090);
        patterns.insert("GeForce RTX 5090", BaselineGpuType::NvidiaRtx5090);

        // AMD patterns
        patterns.insert("MI100", BaselineGpuType::AmdMi100);
        patterns.insert("Instinct MI100", BaselineGpuType::AmdMi100);
        patterns.insert("MI250", BaselineGpuType::AmdMi250);
        patterns.insert("MI250X", BaselineGpuType::AmdMi250);
        patterns.insert("Instinct MI250", BaselineGpuType::AmdMi250);
        patterns.insert("MI300X", BaselineGpuType::AmdMi300X);
        patterns.insert("Instinct MI300X", BaselineGpuType::AmdMi300X);

        Self {
            registrations: Arc::new(RwLock::new(HashMap::new())),
            gpu_patterns: patterns,
        }
    }

    /// Detect GPU type from name string
    pub fn detect_gpu_type(&self, gpu_name: &str) -> BaselineGpuType {
        let name_upper = gpu_name.to_uppercase();

        for (pattern, gpu_type) in &self.gpu_patterns {
            if name_upper.contains(&pattern.to_uppercase()) {
                info!("🎮 Detected GPU: {} -> {:?}", gpu_name, gpu_type);
                return *gpu_type;
            }
        }

        // Try compute capability detection as fallback
        warn!("⚠️ Unknown GPU: {}", gpu_name);
        BaselineGpuType::Unknown
    }

    /// Detect GPU type from compute capability
    pub fn detect_from_compute_capability(&self, major: u32, minor: u32) -> Option<BaselineGpuType> {
        match (major, minor) {
            (7, 0) => Some(BaselineGpuType::NvidiaV100),
            (8, 0) => Some(BaselineGpuType::NvidiaA100),
            (8, 9) => Some(BaselineGpuType::NvidiaRtx4090), // Could be L40S or 4090
            (9, 0) => Some(BaselineGpuType::NvidiaH100),    // Could be H100 or H200
            (10, 0) => Some(BaselineGpuType::NvidiaRtx5090),
            _ => None,
        }
    }

    /// Register a GPU and return tier configuration
    pub async fn register_gpu(
        &self,
        agent_id: &str,
        api_key_hash: &str,
        gpu_name: &str,
        certificate_fingerprint: Option<String>,
    ) -> Result<GpuRegistration, String> {
        let gpu_type = self.detect_gpu_type(gpu_name);

        if !gpu_type.is_supported() {
            return Err(format!(
                "GPU '{}' is not supported. Supported GPUs: V100, A100, H100, H200, L40S, \
                RTX 4090, RTX 5090, MI100, MI250, MI300X",
                gpu_name
            ));
        }

        let specs = GpuSpecifications::from_type(gpu_type);
        let tier_config = GpuTierConfig::from_gpu(&specs);
        let amp_targets = AmplificationTargets::from_gpu(&specs);
        let now = chrono::Utc::now();

        let registration = GpuRegistration {
            agent_id: agent_id.to_string(),
            api_key_hash: api_key_hash.to_string(),
            detected_gpu: gpu_type,
            gpu_specs: specs,
            tier_config,
            amplification_targets: amp_targets,
            certificate_fingerprint,
            registered_at: now,
            last_seen: now,
        };

        self.registrations.write().await.insert(
            agent_id.to_string(),
            registration.clone(),
        );

        info!(
            "🎮 GPU registered: agent={}, gpu={}, tier={}, pricing={}",
            agent_id,
            gpu_type.name(),
            registration.tier_config.tier_name,
            registration.tier_config.pricing_tier
        );

        Ok(registration)
    }

    /// Get registration for an agent
    pub async fn get_registration(&self, agent_id: &str) -> Option<GpuRegistration> {
        self.registrations.read().await.get(agent_id).cloned()
    }

    /// Update last seen timestamp
    pub async fn update_last_seen(&self, agent_id: &str) {
        if let Some(reg) = self.registrations.write().await.get_mut(agent_id) {
            reg.last_seen = chrono::Utc::now();
        }
    }

    /// Get all registrations
    pub async fn get_all_registrations(&self) -> Vec<GpuRegistration> {
        self.registrations.read().await.values().cloned().collect()
    }

    /// Get supported GPUs list
    pub fn get_supported_gpus(&self) -> Vec<(BaselineGpuType, GpuSpecifications, GpuTierConfig)> {
        GpuSpecifications::all_supported()
            .into_iter()
            .map(|specs| {
                let tier = GpuTierConfig::from_gpu(&specs);
                (specs.gpu_type, specs, tier)
            })
            .collect()
    }
}