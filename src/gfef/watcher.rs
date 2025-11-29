//! Model Directory Watcher Service
//! 
//! Watches the VXLAN pending models directory and auto-triggers GFEF extraction.
//! Part of Lock 1 of the Triple IP Lock™ Architecture.

use std::path::{Path, PathBuf};
use std::time::Duration;
use std::collections::HashSet;
use anyhow::{Result, Context};
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{info, warn, error, debug};
use uuid::Uuid;

use super::extraction::{ExtractionConfig, ExtractionResult, ExtractionService};

/// Events from the model watcher
#[derive(Debug, Clone)]
pub enum WatcherEvent {
    /// New model detected at path
    NewModelDetected { path: PathBuf, customer_id: Uuid },
    /// Extraction started for model
    ExtractionStarted { path: PathBuf, job_id: Uuid },
    /// Extraction completed
    ExtractionCompleted { result: ExtractionResult },
    /// Error during watch/extraction
    WatcherError { message: String },
}

/// Configuration for the watcher service
#[derive(Debug, Clone)]
pub struct WatcherConfig {
    /// Directory to watch for pending models
    pub watch_dir: PathBuf,
    /// Polling interval in seconds
    pub poll_interval_secs: u64,
    /// Whether to process existing models on startup
    pub process_existing: bool,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            watch_dir: PathBuf::from("/var/lib/vxlan/models/pending"),
            poll_interval_secs: 10,
            process_existing: true,
        }
    }
}

/// Model directory watcher service
pub struct ModelWatcherService {
    config: WatcherConfig,
    extraction_service: ExtractionService,
    event_tx: mpsc::Sender<WatcherEvent>,
    /// Track which models we've already processed
    processed_models: HashSet<PathBuf>,
}

impl ModelWatcherService {
    pub fn new(
        config: WatcherConfig,
        extraction_service: ExtractionService,
        event_tx: mpsc::Sender<WatcherEvent>,
    ) -> Self {
        Self {
            config,
            extraction_service,
            event_tx,
            processed_models: HashSet::new(),
        }
    }

    /// Start the watcher service (runs indefinitely)
    pub async fn run(&mut self) -> Result<()> {
        info!("🔍 Starting Model Watcher Service");
        info!("   Watch directory: {}", self.config.watch_dir.display());
        info!("   Poll interval: {}s", self.config.poll_interval_secs);

        // Ensure watch directory exists
        std::fs::create_dir_all(&self.config.watch_dir)
            .context("Failed to create watch directory")?;

        // Process existing models if configured
        if self.config.process_existing {
            self.process_existing_models().await?;
        }

        // Start polling loop
        let mut poll_interval = interval(Duration::from_secs(self.config.poll_interval_secs));

        loop {
            poll_interval.tick().await;
            if let Err(e) = self.check_for_new_models().await {
                error!("Error checking for new models: {}", e);
                let _ = self.event_tx.send(WatcherEvent::WatcherError {
                    message: e.to_string(),
                }).await;
            }
        }
    }

    /// Process models that already exist in the watch directory
    async fn process_existing_models(&mut self) -> Result<()> {
        info!("Processing existing models in watch directory...");
        
        let entries = std::fs::read_dir(&self.config.watch_dir)
            .context("Failed to read watch directory")?;

        for entry in entries.flatten() {
            let path = entry.path();
            if self.is_model_directory(&path) && !self.processed_models.contains(&path) {
                self.process_new_model(path).await?;
            }
        }
        Ok(())
    }

    /// Check for new models in the watch directory
    async fn check_for_new_models(&mut self) -> Result<()> {
        let entries = std::fs::read_dir(&self.config.watch_dir)
            .context("Failed to read watch directory")?;

        for entry in entries.flatten() {
            let path = entry.path();
            if self.is_model_directory(&path) && !self.processed_models.contains(&path) {
                debug!("New model detected: {}", path.display());
                self.process_new_model(path).await?;
            }
        }
        Ok(())
    }

    /// Check if a path is a model directory (contains .safetensors files)
    pub fn is_model_directory(&self, path: &Path) -> bool {
        if !path.is_dir() {
            return false;
        }
        
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if name.to_string_lossy().ends_with(".safetensors") {
                    return true;
                }
            }
        }
        false
    }

    /// Process a newly detected model
    async fn process_new_model(&mut self, path: PathBuf) -> Result<()> {
        // Extract customer ID from path or generate new one
        let customer_id = self.extract_customer_id(&path);

        // Send detection event
        let _ = self.event_tx.send(WatcherEvent::NewModelDetected {
            path: path.clone(),
            customer_id,
        }).await;

        // Mark as processed to avoid re-processing
        self.processed_models.insert(path.clone());

        // Trigger extraction
        info!("🚀 Auto-triggering GFEF extraction for: {}", path.display());

        match self.extraction_service.extract_model(&path, &customer_id).await {
            Ok(result) => {
                let _ = self.event_tx.send(WatcherEvent::ExtractionCompleted {
                    result,
                }).await;
            }
            Err(e) => {
                error!("Extraction failed for {}: {}", path.display(), e);
                let _ = self.event_tx.send(WatcherEvent::WatcherError {
                    message: format!("Extraction failed: {}", e),
                }).await;
            }
        }
        Ok(())
    }

    /// Extract customer ID from model path (format: customer_uuid_modelname)
    pub fn extract_customer_id(&self, path: &Path) -> Uuid {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            // Try to parse UUID from start of directory name
            if name.len() >= 36 {
                if let Ok(uuid) = Uuid::parse_str(&name[..36]) {
                    return uuid;
                }
            }
        }
        // Generate new UUID if not found in path
        Uuid::new_v4()
    }

    /// Get count of processed models
    pub fn processed_count(&self) -> usize {
        self.processed_models.len()
    }

    /// Clear processed models cache (for testing)
    pub fn clear_processed(&mut self) {
        self.processed_models.clear();
    }
}

/// Spawn the watcher service as a background task
pub fn spawn_watcher_service(
    config: WatcherConfig,
    extraction_config: ExtractionConfig,
) -> (tokio::task::JoinHandle<()>, mpsc::Receiver<WatcherEvent>) {
    let (event_tx, event_rx) = mpsc::channel(100);
    let (result_tx, _result_rx) = mpsc::channel(100);

    let extraction_service = ExtractionService::new(extraction_config, result_tx);

    let handle = tokio::spawn(async move {
        let mut watcher = ModelWatcherService::new(config, extraction_service, event_tx);
        if let Err(e) = watcher.run().await {
            error!("Watcher service error: {}", e);
        }
    });

    (handle, event_rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_watcher_config_default() {
        let config = WatcherConfig::default();
        assert_eq!(config.poll_interval_secs, 10);
        assert!(config.process_existing);
    }

    #[test]
    fn test_is_model_directory() {
        let temp = tempdir().unwrap();
        let model_dir = temp.path().join("test_model");
        std::fs::create_dir_all(&model_dir).unwrap();

        // Not a model dir (no safetensors)
        let (event_tx, _) = mpsc::channel(10);
        let (result_tx, _) = mpsc::channel(10);
        let config = WatcherConfig::default();
        let extraction_config = ExtractionConfig::default();
        let extraction_service = ExtractionService::new(extraction_config, result_tx);
        let watcher = ModelWatcherService::new(config, extraction_service, event_tx);

        assert!(!watcher.is_model_directory(&model_dir));

        // Add safetensors file
        std::fs::write(model_dir.join("model.safetensors"), b"dummy").unwrap();
        assert!(watcher.is_model_directory(&model_dir));
    }

    #[test]
    fn test_extract_customer_id() {
        let (event_tx, _) = mpsc::channel(10);
        let (result_tx, _) = mpsc::channel(10);
        let config = WatcherConfig::default();
        let extraction_config = ExtractionConfig::default();
        let extraction_service = ExtractionService::new(extraction_config, result_tx);
        let watcher = ModelWatcherService::new(config, extraction_service, event_tx);

        // Valid UUID in path
        let path = PathBuf::from("/models/550e8400-e29b-41d4-a716-446655440000_llama");
        let uuid = watcher.extract_customer_id(&path);
        assert_eq!(uuid.to_string(), "550e8400-e29b-41d4-a716-446655440000");

        // No UUID - should generate new one
        let path = PathBuf::from("/models/llama-7b");
        let uuid = watcher.extract_customer_id(&path);
        assert!(!uuid.is_nil()); // Should be a valid non-nil UUID
    }
}

