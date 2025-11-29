//! GFEF Extraction Service
//! 
//! Integrates with the gfef-extract CLI to process models uploaded via VXLAN.
//! When the Control Plane detects a new model in the VXLAN pending directory,
//! it triggers extraction and stores the index securely.
//!
//! This is Lock 1 of the Triple IP Lock™ Architecture.

use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error, debug};
use tokio::sync::mpsc;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Configuration for GFEF extraction service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionConfig {
    /// Path to gfef-extract executable
    pub gfef_extract_path: PathBuf,
    /// Directory to watch for pending models
    pub pending_models_dir: PathBuf,
    /// Directory to move models after indexing
    pub indexed_models_dir: PathBuf,
    /// Directory to store GFEF indices
    pub indices_dir: PathBuf,
    /// Number of principal components
    #[serde(default = "default_k_components")]
    pub k_components: u32,
    /// FFT bins for spectral hash
    #[serde(default = "default_fft_bins")]
    pub fft_bins: u32,
    /// Poll interval for new models (seconds)
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
}

fn default_k_components() -> u32 { 32 }
fn default_fft_bins() -> u32 { 16 }
fn default_poll_interval() -> u64 { 10 }

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            gfef_extract_path: PathBuf::from("gfef-extract"),
            pending_models_dir: PathBuf::from("/var/lib/vxlan/models/pending"),
            indexed_models_dir: PathBuf::from("/var/lib/vxlan/models/indexed"),
            indices_dir: PathBuf::from("/var/lib/vxlan/indices"),
            k_components: 32,
            fft_bins: 16,
            poll_interval_secs: 10,
        }
    }
}

/// Result of a GFEF extraction job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionResult {
    pub job_id: Uuid,
    pub model_path: PathBuf,
    pub index_path: PathBuf,
    pub metadata_path: PathBuf,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub success: bool,
    pub error_message: Option<String>,
    pub stats: Option<ExtractionStats>,
}

/// Statistics from extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionStats {
    pub num_layers: u32,
    pub total_neurons: u64,
    pub index_size_bytes: u64,
    pub extraction_time_secs: f64,
}

/// GFEF Extraction Service
/// 
/// Monitors VXLAN directories and triggers GFEF extraction when new models arrive.
pub struct ExtractionService {
    config: ExtractionConfig,
    result_tx: mpsc::Sender<ExtractionResult>,
}

impl ExtractionService {
    pub fn new(config: ExtractionConfig, result_tx: mpsc::Sender<ExtractionResult>) -> Self {
        Self { config, result_tx }
    }

    /// Extract GFEF index from a model directory
    pub async fn extract_model(&self, model_path: &Path, customer_id: &Uuid) -> Result<ExtractionResult> {
        let job_id = Uuid::new_v4();
        let started_at = Utc::now();
        
        // Generate output paths
        let model_name = model_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        
        let index_filename = format!("{}_{}.gfef", customer_id, model_name);
        let index_path = self.config.indices_dir.join(&index_filename);
        let metadata_path = index_path.with_extension("json");

        info!("Starting GFEF extraction: job_id={}, model={}", job_id, model_path.display());
        
        // Ensure output directory exists
        std::fs::create_dir_all(&self.config.indices_dir)
            .context("Failed to create indices directory")?;

        // Call gfef-extract CLI
        let output = Command::new(&self.config.gfef_extract_path)
            .arg("--model-path")
            .arg(model_path)
            .arg("--output")
            .arg(&index_path)
            .arg("--k-components")
            .arg(self.config.k_components.to_string())
            .arg("--fft-bins")
            .arg(self.config.fft_bins.to_string())
            .output()
            .context("Failed to execute gfef-extract")?;

        let completed_at = Utc::now();
        let extraction_time = (completed_at - started_at).num_milliseconds() as f64 / 1000.0;

        if output.status.success() {
            // Read metadata from generated JSON
            let stats = if metadata_path.exists() {
                self.read_extraction_stats(&metadata_path, extraction_time).ok()
            } else {
                None
            };

            // Move model to indexed directory
            let indexed_model_path = self.config.indexed_models_dir.join(model_name);
            if let Err(e) = std::fs::rename(model_path, &indexed_model_path) {
                warn!("Failed to move model to indexed directory: {}", e);
            }

            let result = ExtractionResult {
                job_id,
                model_path: model_path.to_path_buf(),
                index_path,
                metadata_path,
                started_at,
                completed_at,
                success: true,
                error_message: None,
                stats,
            };

            info!("✅ GFEF extraction complete: job_id={}, neurons={:?}", 
                job_id, result.stats.as_ref().map(|s| s.total_neurons));

            // Send result to channel
            let _ = self.result_tx.send(result.clone()).await;
            Ok(result)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("❌ GFEF extraction failed: {}", stderr);
            
            let result = ExtractionResult {
                job_id,
                model_path: model_path.to_path_buf(),
                index_path,
                metadata_path,
                started_at,
                completed_at,
                success: false,
                error_message: Some(stderr.to_string()),
                stats: None,
            };

            let _ = self.result_tx.send(result.clone()).await;
            Ok(result)
        }
    }

    fn read_extraction_stats(&self, metadata_path: &Path, extraction_time: f64) -> Result<ExtractionStats> {
        #[derive(Deserialize)]
        struct Metadata {
            num_layers: u32,
            total_neurons: u64,
        }
        
        let content = std::fs::read_to_string(metadata_path)?;
        let meta: Metadata = serde_json::from_str(&content)?;
        let index_size = std::fs::metadata(metadata_path.with_extension("gfef"))
            .map(|m| m.len())
            .unwrap_or(0);

        Ok(ExtractionStats {
            num_layers: meta.num_layers,
            total_neurons: meta.total_neurons,
            index_size_bytes: index_size,
            extraction_time_secs: extraction_time,
        })
    }
}

