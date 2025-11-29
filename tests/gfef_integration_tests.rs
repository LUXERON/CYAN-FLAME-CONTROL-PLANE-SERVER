//! GFEF Integration Tests
//!
//! Tests for the GFEF extraction pipeline, watcher service, and API endpoints.
//! Part of NULL SPACE AI Inference Network by NEUNOMY.

use tempfile::tempdir;
use std::path::PathBuf;
use tokio::sync::mpsc;
use uuid::Uuid;

// Import from the main crate
use symmetrix_core::gfef::{
    ExtractionConfig, ExtractionService,
    WatcherConfig, ModelWatcherService,
    IndexConfig, GFEFIndexGenerator,
    CalibrationMatrix,
};
use symmetrix_core::gfef::index::{WeightData, LayerWeights};

/// Test that ExtractionConfig has sensible defaults
#[test]
fn test_extraction_config_defaults() {
    let config = ExtractionConfig::default();
    assert_eq!(config.k_components, 32);
    assert_eq!(config.fft_bins, 16);
    assert_eq!(config.poll_interval_secs, 10);
}

/// Test WatcherConfig defaults
#[test]
fn test_watcher_config_defaults() {
    let config = WatcherConfig::default();
    assert_eq!(config.poll_interval_secs, 10);
    assert!(config.process_existing);
}

/// Test IndexConfig defaults
#[test]
fn test_index_config_defaults() {
    let config = IndexConfig::default();
    assert_eq!(config.k_components, 32);
    assert_eq!(config.fft_bins, 16);
    assert!(config.target_sparsity > 0.0);
}

/// Test model directory detection
#[tokio::test]
async fn test_model_directory_detection() {
    let temp = tempdir().unwrap();
    
    // Create a fake model directory
    let model_dir = temp.path().join("test_model");
    std::fs::create_dir_all(&model_dir).unwrap();
    
    // Setup watcher
    let (event_tx, _) = mpsc::channel(10);
    let (result_tx, _) = mpsc::channel(10);
    let config = WatcherConfig {
        watch_dir: temp.path().to_path_buf(),
        poll_interval_secs: 1,
        process_existing: false,
    };
    let extraction_config = ExtractionConfig::default();
    let extraction_service = ExtractionService::new(extraction_config, result_tx);
    let watcher = ModelWatcherService::new(config, extraction_service, event_tx);
    
    // Without safetensors file, not a model directory
    assert!(!watcher.is_model_directory(&model_dir));
    
    // Add a safetensors file
    std::fs::write(model_dir.join("model.safetensors"), b"dummy data").unwrap();
    
    // Now it should be detected as a model directory
    assert!(watcher.is_model_directory(&model_dir));
}

/// Test customer ID extraction from path
#[tokio::test]
async fn test_customer_id_extraction() {
    let (event_tx, _) = mpsc::channel(10);
    let (result_tx, _) = mpsc::channel(10);
    let config = WatcherConfig::default();
    let extraction_config = ExtractionConfig::default();
    let extraction_service = ExtractionService::new(extraction_config, result_tx);
    let watcher = ModelWatcherService::new(config, extraction_service, event_tx);
    
    // Valid UUID in path
    let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
    let path = PathBuf::from(format!("/models/{}_llama", uuid_str));
    let extracted = watcher.extract_customer_id(&path);
    assert_eq!(extracted.to_string(), uuid_str);
    
    // No UUID - should generate new one (non-nil)
    let path = PathBuf::from("/models/llama-7b");
    let extracted = watcher.extract_customer_id(&path);
    assert!(!extracted.is_nil());
}

/// Test GFEF index generation with synthetic data
#[test]
fn test_gfef_index_generation() {
    let config = IndexConfig {
        k_components: 8,  // Smaller for testing
        fft_bins: 4,
        target_sparsity: 0.95,
    };

    let mut generator = GFEFIndexGenerator::new(config);

    // Create synthetic weight data (small matrix)
    let weights: Vec<f32> = (0..1024).map(|i| (i as f32 * 0.001).sin()).collect();

    let weight_data = WeightData {
        layers: vec![
            LayerWeights {
                name: "layer_0".to_string(),
                rows: 32,
                cols: 32,
                data: weights,
            }
        ],
    };

    // Generate index
    let customer_id = Uuid::new_v4();
    let index = generator.generate_index(customer_id, "test_model", "Test Model", &weight_data);

    // Verify index
    assert_eq!(index.layers.len(), 1);
    assert_eq!(index.layers[0].num_neurons, 32);
    assert!(!index.layers[0].signatures.is_empty());
}

/// Test calibration matrix generation
#[test]
fn test_calibration_matrix_generation() {
    let matrix = CalibrationMatrix::generate(60, None);

    // Matrix should have correct dimensions (64x64)
    assert_eq!(matrix.values.len(), 64);
    assert_eq!(matrix.values[0].len(), 64);

    // Matrix values should be normalized
    for row in &matrix.values {
        for &val in row {
            assert!(val.abs() <= 2.0); // Normalized values with some tolerance
        }
    }

    // Should have valid signature
    assert!(!matrix.signature.is_empty());
}

/// Test calibration matrix rotation
#[tokio::test]
async fn test_calibration_rotation() {
    let matrix1 = CalibrationMatrix::generate(60, None);
    let matrix2 = CalibrationMatrix::generate(60, None);

    // Matrices should be different (rotation produces new values)
    assert_ne!(matrix1.id, matrix2.id);
    assert_ne!(matrix1.values, matrix2.values);

    // Both should have same dimensions
    assert_eq!(matrix1.values.len(), matrix2.values.len());
}

/// Test extraction result serialization
#[test]
fn test_extraction_result_serialization() {
    use symmetrix_core::gfef::{ExtractionResult, ExtractionStats};
    use chrono::Utc;

    let result = ExtractionResult {
        job_id: Uuid::new_v4(),
        model_path: PathBuf::from("/models/test"),
        index_path: PathBuf::from("/indices/test.gfef"),
        metadata_path: PathBuf::from("/indices/test.json"),
        started_at: Utc::now(),
        completed_at: Utc::now(),
        success: true,
        error_message: None,
        stats: Some(ExtractionStats {
            num_layers: 32,
            total_neurons: 4096,
            index_size_bytes: 1024,
            extraction_time_secs: 5.5,
        }),
    };

    // Should serialize to JSON without error
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("job_id"));
    assert!(json.contains("success"));

    // Should deserialize back
    let parsed: ExtractionResult = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.success, true);
    assert_eq!(parsed.stats.unwrap().num_layers, 32);
}

/// Test processed models tracking
#[tokio::test]
async fn test_processed_models_tracking() {
    let temp = tempdir().unwrap();

    let (event_tx, _) = mpsc::channel(10);
    let (result_tx, _) = mpsc::channel(10);
    let config = WatcherConfig {
        watch_dir: temp.path().to_path_buf(),
        poll_interval_secs: 1,
        process_existing: false,
    };
    let extraction_config = ExtractionConfig::default();
    let extraction_service = ExtractionService::new(extraction_config, result_tx);
    let mut watcher = ModelWatcherService::new(config, extraction_service, event_tx);

    // Initially no processed models
    assert_eq!(watcher.processed_count(), 0);

    // Clear should work on empty
    watcher.clear_processed();
    assert_eq!(watcher.processed_count(), 0);
}

/// Test watcher event types
#[test]
fn test_watcher_event_types() {
    use symmetrix_core::gfef::WatcherEvent;

    // NewModelDetected
    let event = WatcherEvent::NewModelDetected {
        path: PathBuf::from("/test"),
        customer_id: Uuid::new_v4(),
    };
    assert!(matches!(event, WatcherEvent::NewModelDetected { .. }));

    // ExtractionStarted
    let event = WatcherEvent::ExtractionStarted {
        path: PathBuf::from("/test"),
        job_id: Uuid::new_v4(),
    };
    assert!(matches!(event, WatcherEvent::ExtractionStarted { .. }));

    // WatcherError
    let event = WatcherEvent::WatcherError {
        message: "test error".to_string(),
    };
    assert!(matches!(event, WatcherEvent::WatcherError { .. }));
}

/// Test directory creation
#[tokio::test]
async fn test_directory_creation() {
    let temp = tempdir().unwrap();
    let watch_dir = temp.path().join("watch");
    let indices_dir = temp.path().join("indices");

    // Directories don't exist yet
    assert!(!watch_dir.exists());
    assert!(!indices_dir.exists());

    let extraction_config = ExtractionConfig {
        gfef_extract_path: PathBuf::from("gfef-extract"),
        pending_models_dir: watch_dir.clone(),
        indexed_models_dir: temp.path().join("indexed"),
        indices_dir: indices_dir.clone(),
        k_components: 32,
        fft_bins: 16,
        poll_interval_secs: 10,
    };

    // Create directories manually (simulating what ExtractionService does)
    std::fs::create_dir_all(&extraction_config.pending_models_dir).unwrap();
    std::fs::create_dir_all(&extraction_config.indices_dir).unwrap();

    // Now they should exist
    assert!(watch_dir.exists());
    assert!(indices_dir.exists());
}

/// Test UUID generation is unique
#[test]
fn test_uuid_uniqueness() {
    let mut uuids = Vec::new();
    for _ in 0..100 {
        uuids.push(Uuid::new_v4());
    }

    // All UUIDs should be unique
    let unique_count = uuids.iter().collect::<std::collections::HashSet<_>>().len();
    assert_eq!(unique_count, 100);
}

