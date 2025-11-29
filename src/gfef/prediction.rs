//! Real-Time Activation Prediction Service
//! 
//! Provides per-inference neuron activation predictions.
//! This is the API that customers call during inference.
//! 
//! The customer NEVER receives the full GFEF index - only activation predictions.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

use crate::gfef_index::{GFEFIndex, LayerIndex};
use crate::calibration::CalibrationMatrix;
use crate::subscription::{SubscriptionTier, Subscription};

/// Request for activation prediction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionRequest {
    /// Session token (from auth)
    pub session_token: String,
    /// Customer ID
    pub customer_id: Uuid,
    /// Model being used
    pub model_id: String,
    /// Layer to predict for
    pub layer_id: u32,
    /// Hash of input embedding (privacy-preserving)
    pub input_embedding_hash: String,
    /// Optional: actual embedding for higher accuracy (encrypted)
    pub encrypted_embedding: Option<Vec<u8>>,
}

/// Response with activation predictions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionResponse {
    /// Request ID for tracking
    pub request_id: Uuid,
    /// Predicted active neuron indices
    pub active_neurons: Vec<u32>,
    /// Confidence scores (optional)
    pub confidence_scores: Option<Vec<f32>>,
    /// Calibration slice for these neurons
    pub calibration_slice: Vec<f64>,
    /// When this prediction expires
    pub valid_until: DateTime<Utc>,
    /// Remaining prediction credits
    pub credits_remaining: u64,
    /// Achieved sparsity
    pub sparsity: f32,
}

/// Error types for prediction service
#[derive(Debug, thiserror::Error, Serialize, Deserialize)]
pub enum PredictionError {
    #[error("Invalid session token")]
    InvalidSession,
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    #[error("Layer not found: {0}")]
    LayerNotFound(u32),
    #[error("Subscription expired")]
    SubscriptionExpired,
    #[error("Quota exceeded")]
    QuotaExceeded,
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Activation predictor service
pub struct ActivationPredictor {
    /// Loaded GFEF indices (keyed by model_id)
    indices: HashMap<String, GFEFIndex>,
    /// Sparsity target
    target_sparsity: f32,
}

impl ActivationPredictor {
    pub fn new(target_sparsity: f32) -> Self {
        Self {
            indices: HashMap::new(),
            target_sparsity,
        }
    }
    
    /// Register a GFEF index for predictions
    pub fn register_index(&mut self, index: GFEFIndex) {
        self.indices.insert(index.model_id.clone(), index);
    }
    
    /// Predict active neurons for a layer
    pub fn predict(
        &self,
        request: &PredictionRequest,
        subscription: &Subscription,
        calibration: &CalibrationMatrix,
    ) -> Result<PredictionResponse, PredictionError> {
        // Validate subscription
        if !subscription.is_active() {
            return Err(PredictionError::SubscriptionExpired);
        }
        if !subscription.has_quota() {
            return Err(PredictionError::QuotaExceeded);
        }
        
        // Get index
        let index = self.indices.get(&request.model_id)
            .ok_or_else(|| PredictionError::ModelNotFound(request.model_id.clone()))?;
        
        // Get layer
        let layer = index.layers.iter()
            .find(|l| l.layer_id == request.layer_id)
            .ok_or(PredictionError::LayerNotFound(request.layer_id))?;
        
        // Predict active neurons
        let (active_neurons, confidence_scores) = self.compute_activations(
            layer,
            &request.input_embedding_hash,
        );
        
        // Extract calibration slice for active neurons
        let calibration_slice = self.extract_calibration_slice(
            &active_neurons,
            calibration,
        );
        
        // Calculate achieved sparsity
        let sparsity = 1.0 - (active_neurons.len() as f32 / layer.num_neurons as f32);
        
        Ok(PredictionResponse {
            request_id: Uuid::new_v4(),
            active_neurons,
            confidence_scores: Some(confidence_scores),
            calibration_slice,
            valid_until: Utc::now() + chrono::Duration::milliseconds(100),
            credits_remaining: subscription.predictions_quota - subscription.predictions_used - 1,
            sparsity,
        })
    }
    
    /// Compute which neurons should activate
    fn compute_activations(
        &self,
        layer: &LayerIndex,
        input_hash: &str,
    ) -> (Vec<u32>, Vec<f32>) {
        // Derive pseudo-random projection from input hash
        let seed = Self::hash_to_seed(input_hash);
        let input_projection = self.derive_projection(seed, layer.k_components as usize);
        
        // Score each neuron by alignment with input
        let mut scores: Vec<(u32, f32)> = layer.signatures.iter()
            .map(|sig| {
                let score = self.compute_alignment(&sig.projection, &input_projection, sig.energy);
                (sig.neuron_idx, score)
            })
            .collect();
        
        // Sort by score descending
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        
        // Select top (1 - sparsity) neurons
        let num_active = ((1.0 - self.target_sparsity) * layer.num_neurons as f32).ceil() as usize;
        let num_active = num_active.max(1);
        
        let active: Vec<u32> = scores.iter().take(num_active).map(|(idx, _)| *idx).collect();
        let confidences: Vec<f32> = scores.iter().take(num_active).map(|(_, score)| *score).collect();

        (active, confidences)
    }

    /// Compute alignment between neuron projection and input projection
    fn compute_alignment(&self, neuron_proj: &[f32], input_proj: &[f32], energy: f32) -> f32 {
        let dot: f32 = neuron_proj.iter()
            .zip(input_proj.iter())
            .map(|(a, b)| a * b)
            .sum();

        // Weight by energy (higher energy neurons are more "specialized")
        dot.abs() * energy.sqrt()
    }

    /// Convert input hash to seed for deterministic projection
    fn hash_to_seed(hash: &str) -> u64 {
        let mut seed = 0u64;
        for (i, byte) in hash.bytes().take(8).enumerate() {
            seed |= (byte as u64) << (i * 8);
        }
        seed
    }

    /// Derive pseudo-random projection from seed
    fn derive_projection(&self, seed: u64, k: usize) -> Vec<f32> {
        let mut state = seed;
        (0..k)
            .map(|_| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                (state as f32) / (u64::MAX as f32) * 2.0 - 1.0
            })
            .collect()
    }

    /// Extract calibration values for active neurons
    fn extract_calibration_slice(
        &self,
        active_neurons: &[u32],
        calibration: &CalibrationMatrix,
    ) -> Vec<f64> {
        // Map neuron indices to calibration matrix rows
        let matrix_size = calibration.values.len();

        active_neurons.iter()
            .map(|&idx| {
                let row = (idx as usize) % matrix_size;
                let col = (idx as usize / matrix_size) % matrix_size;
                calibration.values[row][col]
            })
            .collect()
    }

    /// Get statistics about loaded indices
    pub fn stats(&self) -> PredictorStats {
        PredictorStats {
            models_loaded: self.indices.len(),
            total_neurons: self.indices.values().map(|i| i.total_neurons).sum(),
            total_layers: self.indices.values().map(|i| i.layers.len()).sum(),
            target_sparsity: self.target_sparsity,
        }
    }
}

/// Statistics about the predictor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictorStats {
    pub models_loaded: usize,
    pub total_neurons: u64,
    pub total_layers: usize,
    pub target_sparsity: f32,
}
