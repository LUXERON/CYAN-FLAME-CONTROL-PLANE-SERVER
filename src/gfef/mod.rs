//! GFEF Control Plane Module
//! 
//! Triple IP Lock™ Architecture for NULL SPACE AI Inference Network
//! 
//! GFEF (Galois Field Eigenmode Folding) enables:
//! - 19.6× weight reduction through spectral sparsity
//! - Combined with UAO-QTCAM: 24,500× total compression
//! 
//! This module provides:
//! - GFEF Index generation from customer model weights
//! - Real-time activation prediction API
//! - Calibration matrix rotation (60-second cycle)
//! - Subscription management for tiered access

pub mod api;
pub mod calibration;
pub mod index;
pub mod prediction;
pub mod storage;
pub mod subscription;

// Re-export main types
pub use api::{create_router as create_gfef_router, AppState as GFEFAppState};
pub use calibration::{CalibrationService, CalibrationMatrix};
pub use index::{GFEFIndexGenerator, IndexConfig, IndexMetadata};
pub use prediction::{ActivationPredictor, PredictionRequest, PredictionResponse};
pub use storage::IndexStorage;
pub use subscription::{SubscriptionManager, SubscriptionTier, Subscription};

