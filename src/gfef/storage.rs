//! GFEF Index Storage
//! 
//! Secure storage for GFEF indices.
//! Indices are stored encrypted and NEVER sent to customers.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::gfef_index::{GFEFIndex, IndexMetadata};

/// Storage backend for GFEF indices
pub struct IndexStorage {
    /// In-memory cache
    cache: HashMap<Uuid, GFEFIndex>,
    /// Metadata index
    metadata: HashMap<Uuid, IndexMetadata>,
    /// Storage path (for persistent storage)
    storage_path: PathBuf,
}

impl IndexStorage {
    pub fn new(storage_path: PathBuf) -> Self {
        Self {
            cache: HashMap::new(),
            metadata: HashMap::new(),
            storage_path,
        }
    }
    
    /// Store a GFEF index
    pub fn store(&mut self, index: GFEFIndex) -> IndexMetadata {
        let id = index.id;
        
        // Create metadata
        let mut meta = IndexMetadata::from(&index);
        meta.index_size_bytes = self.estimate_size(&index);
        
        // Store in cache
        self.cache.insert(id, index);
        self.metadata.insert(id, meta.clone());
        
        // TODO: Persist to disk/database
        
        meta
    }
    
    /// Get index by ID
    pub fn get(&self, id: &Uuid) -> Option<&GFEFIndex> {
        self.cache.get(id)
    }
    
    /// Get index by model ID
    pub fn get_by_model(&self, model_id: &str) -> Option<&GFEFIndex> {
        self.cache.values().find(|idx| idx.model_id == model_id)
    }
    
    /// Get metadata for all indices
    pub fn list_metadata(&self) -> Vec<IndexMetadata> {
        self.metadata.values().cloned().collect()
    }
    
    /// Get metadata for a customer
    pub fn list_for_customer(&self, customer_id: &Uuid) -> Vec<IndexMetadata> {
        self.metadata.values()
            .filter(|m| m.customer_id == *customer_id)
            .cloned()
            .collect()
    }
    
    /// Estimate index size in bytes
    fn estimate_size(&self, index: &GFEFIndex) -> u64 {
        let mut size = 0u64;
        
        for layer in &index.layers {
            // Principal components
            size += (layer.principal_components.len() * 4) as u64;
            
            // Signatures
            for sig in &layer.signatures {
                size += 8; // layer_id + neuron_idx
                size += 4; // energy
                size += (sig.projection.len() * 4) as u64;
                size += (sig.spectral_hash.len() * 4) as u64;
            }
        }
        
        size
    }
    
    /// Delete an index
    pub fn delete(&mut self, id: &Uuid) -> bool {
        self.cache.remove(id).is_some() && self.metadata.remove(id).is_some()
    }
}
